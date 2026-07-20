"""In-process parent-notification signals for blocking waits (wait_join).

A pure wakeup layer over disk truth: ``runner.finish()`` calls
:func:`notify_parent` *after* the child's inbox entry is durably written, and
the wait route long-polls on the parent's event. Events carry no payload —
waiters always re-read the inbox/run states from disk, so a missed or stale
signal costs at most one poll interval, never a lost result.

Events are keyed by ``(parent_kind, parent_id)`` ("chat"/"run"), matching the
parent addressing used by spawn/finish everywhere else.
"""

from __future__ import annotations

import asyncio
import logging

logger = logging.getLogger("uvicorn.error")

_EVENTS: dict[tuple[str, str], asyncio.Event] = {}
_LOOP: asyncio.AbstractEventLoop | None = None


def bind_loop(loop: asyncio.AbstractEventLoop) -> None:
    """Capture the server loop so notifies from other threads (sync route
    handlers, to_thread stage jobs) can set events safely."""
    global _LOOP
    _LOOP = loop


def event_for(parent_kind: str, parent_id: str) -> asyncio.Event:
    """The (created-on-demand) event for a parent. Waiters should ``clear()``
    it *before* re-reading disk state, then await it — a notify landing between
    the clear and the wait is still observed."""
    key = (parent_kind, parent_id)
    event = _EVENTS.get(key)
    if event is None:
        event = asyncio.Event()
        _EVENTS[key] = event
    return event


def notify_parent(parent_kind: str, parent_id: str) -> None:
    """Wake any waiter long-polling for this parent's children. No-op when no
    waiter ever registered (events are created lazily by waiters)."""
    event = _EVENTS.get((parent_kind, parent_id))
    if event is None:
        return
    loop = _LOOP
    if loop is not None and loop.is_running():
        loop.call_soon_threadsafe(event.set)
    else:
        event.set()
