"""Chat engine (plan 4.3 A3): the exchange runner shared by the unscripted
chats (``chats.run_chat``) and the Finished stage's review-session chat
(``S06Finished._run_chat``) — the same algorithm with different state files,
session dirs, and toolsets. Both callers are thin adapters over
:func:`run_exchange`.

State shape: ``state["exchanges"]`` is a list of ``{"user": str, "turns": []}``;
the agent's turns for the latest exchange are persisted into
``exchanges[-1]["turns"]`` via the shared TurnPersister (A2), and the chat
variant of the canonical error write (``record_chat_errors``) applies.
"""

from __future__ import annotations

import logging
import time
from collections.abc import Awaitable
from pathlib import Path
from typing import Callable

from crack_server import pi_runner
from crack_server.state import JsonState
from crack_server.stages.steprun import (
    error_recorder,
    prompt_recorder,
    record_chat_errors,
    turn_persister,
)

logger = logging.getLogger("uvicorn.error")


def run_exchange_sync(**kwargs) -> None:
    """Sync wrapper over :func:`run_exchange` for thread-based callers (the
    Finished stage's review chat, dispatched via ``asyncio.to_thread``).
    Must NOT be called from inside a running event loop."""
    import asyncio

    return asyncio.run(run_exchange(**kwargs))


async def run_exchange(
    *,
    state: JsonState,
    ident: str,
    message_builder: Callable[[str], str],
    record_template: str,
    log_prefix: str,
    model: str,
    session_id: str,
    sessions_dir: Path,
    tools: str | None,
    timeout_seconds: int,
    hop_kwargs: dict | None = None,
    pre_stop_check: Callable[[], bool] | None = None,
    on_first_exchange: "Callable[[str], Awaitable[None]] | None" = None,
    on_no_exchanges: Callable[[], None] | None = None,
    stopped_phase: str = "idle",
    env_extra: dict[str, str] | None = None,
    media_dir: Path | None = None,
    media_url_prefix: str = "",
) -> None:
    """Run the agent for the latest entry in ``state["exchanges"]``.

    ``message_builder`` compiles the exchange's raw user text into the hop's
    message (identity for unscripted chats, the ``chat.md`` template for the
    Finished stage). ``hop_kwargs`` carries the ``pid_file``/``stop_check``
    pair through to run_agent_hop. ``pre_stop_check`` (unscripted chats only)
    skips the hop entirely when a stop is already flagged. ``on_first_exchange``
    runs before the first exchange's hop (chat titling); ``on_no_exchanges``
    runs when there is nothing to do. ``stopped_phase`` is the phase written
    when the hop was externally stopped ("idle" for unscripted chats,
    "stopped" for the Finished stage). ``media_dir`` / ``media_url_prefix``
    (optional) enable image-thumbnail persistence for read/analyze_image tool
    calls (see ``stages.steprun.attach_media_to_blocks``).
    """
    start = time.monotonic()
    with record_chat_errors(state, log_message=f"{log_prefix}: exchange failed for {ident}"):
        exchanges = state.read().get("exchanges", [])
        if not exchanges:
            if on_no_exchanges is not None:
                on_no_exchanges()
            return
        idx = len(exchanges) - 1
        user_msg = exchanges[idx].get("user", "")
        message = message_builder(user_msg)

        if idx == 0 and on_first_exchange is not None:
            await on_first_exchange(user_msg)

        persister = turn_persister(
            state, subpath=["exchanges", idx],
            media_dir=media_dir, media_url_prefix=media_url_prefix,
        )
        record = prompt_recorder(persister, "chat", record_template, original=user_msg)

        if pre_stop_check is not None and pre_stop_check():
            reason = "stopped"
        else:
            reason = await pi_runner.arun_agent_hop(
                log_prefix=log_prefix,
                model=model,
                session_id=session_id,
                sessions_dir=sessions_dir,
                tools=tools,
                message=message,
                start=start,
                sentinel=None,
                timeout_seconds=timeout_seconds,
                persist_turn=persister.persist,
                hop=1,
                record_prompt=record,
                record_error=error_recorder(state, subpath=["exchanges", idx]),
                env_extra=env_extra,
                **(hop_kwargs or {}),
            )

        def _finish(s: dict) -> dict:
            s["phase"] = stopped_phase if reason == "stopped" else "idle"
            s["stop_requested"] = False
            if reason == "empty":
                s["error"] = "model returned empty responses"
                s["error_detail"] = ""
            return s

        state.update(_finish)
        logger.info("%s: exchange %d done for %s (reason=%s)", log_prefix, idx, ident, reason)
