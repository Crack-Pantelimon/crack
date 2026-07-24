"""Blocking child-result waits (the server side of the ``wait_join`` tool).

``poll()`` is the single consumption point for a parent's ``child_inbox`` when
an agent is waiting: it resolves the caller's target (``all`` / a run id / a
persona slug), atomically drains the matching inbox entries, and formats them
with ``runner.format_child_result`` — the exact text a ``drain_children``
resume would have produced. Because the wait drains the inbox, the queued
drain job (exclusive-enqueued by ``runner.finish``) later no-ops: no duplicate
delivery.

Disk is truth: everything here re-reads run/chat state; ``signals`` events
(the route's long-poll wakeup) carry no payload.

The finish() races we must not mis-resolve:

1. ``parent_notified=True`` lands in one state write, the inbox entry in a
   later one. A child seen as ``notified`` with no inbox entry is *pending*
   while its ``finished_at`` is recent; past ``NOTIFIED_GAP_SECONDS`` it was
   consumed by an earlier wait, so an explicit target rebuilds
   ``delivered_earlier`` (and the caller's two-strike rule can force the same
   via ``rebuild``).

2. ``phase`` may flip to a terminal value *before* ``finish()`` finishes
   extracting the child patch and merging it into the parent overlay (historically
   ``_after_hop`` stamped ``done`` then called ``finish()``, which blocks on
   extract for tens of seconds). A terminal-but-not-notified child inside
   ``FINISH_GAP_SECONDS`` is therefore *pending* — not a crash-gap rebuild.
   Only after that window do we rebuild (true crash recovery). Returning early
   here is the harmful timing bug: the parent agent resumes before
   "Merged sub-agent" and edits files that are not in its overlay yet.
"""

from __future__ import annotations

import logging
import time

from crack_server import paths
from crack_server.sub_agents import runner

logger = logging.getLogger("uvicorn.error")

# Terminal run phases: a child in one of these will never produce new output.
TERMINAL_PHASES = ("done", "error", "stopped")

# How long a notified-but-entryless child is treated as the transient finish()
# gap (pending) rather than as consumed by an earlier wait.
NOTIFIED_GAP_SECONDS = 60.0

# How long a terminal-but-not-yet-notified child is treated as finish()-in-
# progress (extract + drain + inbox) rather than as a crashed finish().
FINISH_GAP_SECONDS = 120.0


def _state_obj_for(chat_id: str, parent_kind: str, parent_id: str):
    if parent_kind == "run":
        return paths.run_state_by_id(parent_id)
    return paths.chat_state(chat_id)


def _direct_children(chat_id: str, parent_kind: str, parent_id: str) -> list[dict]:
    """Descriptors ``{run_id, persona, phase, notified, finished_at}`` for the
    parent's direct children. A run parent tracks ``children`` + inbox run ids
    (chats have no ``children`` list, so they scan their run tree by parent
    link)."""
    run_ids: set[str] = set()
    if parent_kind == "run":
        state = paths.run_state_by_id(parent_id).read()
        run_ids.update(state.get("children") or [])
        run_ids.update(
            e.get("run_id") for e in state.get("child_inbox") or [] if e.get("run_id")
        )
    else:
        for rid in paths.list_run_ids(chat_id):
            child = paths.run_state(chat_id, rid).read()
            if child.get("parent_kind") == "chat" and child.get("parent_id") == parent_id:
                run_ids.add(rid)

    out: list[dict] = []
    for rid in sorted(run_ids):
        try:
            state = paths.run_state_by_id(rid).read()
        except (ValueError, FileNotFoundError):
            continue
        if not state:
            continue
        out.append({
            "run_id": rid,
            "persona": state.get("persona", ""),
            "phase": state.get("phase", ""),
            "notified": bool(state.get("parent_notified")),
            "finished_at": state.get("finished_at") or 0,
        })
    return out


def drain_matching(
    chat_id: str,
    parent_kind: str,
    parent_id: str,
    run_ids: set[str] | None,
) -> list[dict]:
    """Atomically partition the parent's ``child_inbox``: entries whose run_id
    is in ``run_ids`` (or any, when None) are returned, the rest stay. Single
    consumption point — a drained entry is never delivered twice."""
    taken: list[dict] = []

    def _take(state: dict) -> dict:
        keep = []
        for entry in state.get("child_inbox") or []:
            if run_ids is None or entry.get("run_id") in run_ids:
                taken.append(entry)
            else:
                keep.append(entry)
        state["child_inbox"] = keep
        return state

    _state_obj_for(chat_id, parent_kind, parent_id).update(_take)
    return taken


def _result(entry: dict, *, delivered_earlier: bool = False) -> dict:
    return {
        **entry,
        "delivered_earlier": delivered_earlier,
        "text": runner.format_child_result(entry),
    }


def _rebuild_result(descriptor: dict) -> dict | None:
    """Rebuild a child's result entry from run state (crash gap recovery and
    delivered_earlier rebuilds)."""
    try:
        state = paths.run_state_by_id(descriptor["run_id"]).read()
    except (ValueError, FileNotFoundError):
        return None
    if not state:
        return None
    return _result(
        runner.build_entry(descriptor["run_id"], state), delivered_earlier=True
    )


def _in_notify_gap(descriptor: dict, now: float) -> bool:
    return bool(descriptor.get("notified")) and (
        now - float(descriptor.get("finished_at") or 0) < NOTIFIED_GAP_SECONDS
    )


def _in_finish_gap(descriptor: dict, now: float) -> bool:
    """True while finish() may still be extracting/merging after a terminal phase stamp."""
    finished_at = float(descriptor.get("finished_at") or 0)
    if finished_at <= 0:
        # No finished_at yet (phase flipped without a stamp) — treat as in-progress.
        return True
    return (now - finished_at) < FINISH_GAP_SECONDS


def poll(
    *,
    chat_id: str,
    parent_kind: str,
    parent_id: str,
    target: str | None = None,
    run_ids: list[str] | None = None,
    rebuild: list[str] | None = None,
) -> dict:
    """One non-blocking wait pass. Returns::

        {"results": [...], "pending": [descriptor, ...]}

    ``results`` are freshly drained inbox entries plus rebuilt ones (flagged
    ``delivered_earlier``). ``pending`` lists targets that have not produced a
    result yet: still running, terminal-but-not-notified inside the finish gap
    (extract/drain still in flight), notified-but-entryless within the notify
    gap (two-strike rebuild), and crash-gap rebuilds only after the finish gap
    expires.
    """
    now = time.time()
    children = _direct_children(chat_id, parent_kind, parent_id)
    descriptors = {c["run_id"]: c for c in children}

    # Resolve which runs this poll targets.
    if run_ids:
        wanted: set[str] | None = set(run_ids)
    elif target in (None, "", "all"):
        wanted = None  # all direct children
    else:
        wanted = {c["run_id"] for c in children
                  if c["run_id"] == target or c["persona"] == target}

    results: list[dict] = []
    pending: list[dict] = []
    seen: set[str] = set()

    drained = drain_matching(chat_id, parent_kind, parent_id, wanted)
    for entry in drained:
        seen.add(entry.get("run_id"))
        results.append(_result(entry))

    # Explicit rebuilds (two-strike rule): entry rebuilt from run state.
    for rid in rebuild or []:
        if rid in seen:
            continue
        rebuilt = _rebuild_result({"run_id": rid})
        if rebuilt is not None:
            seen.add(rid)
            results.append(rebuilt)

    targets = (
        [descriptors[rid] for rid in sorted(descriptors)]
        if wanted is None
        else [descriptors[rid] for rid in sorted(wanted) if rid in descriptors]
    )
    for descriptor in targets:
        rid = descriptor["run_id"]
        if rid in seen:
            continue
        if not descriptor["notified"]:
            if descriptor["phase"] in TERMINAL_PHASES:
                if _in_finish_gap(descriptor, now):
                    # finish() still running (extract/drain/inbox) — do NOT
                    # rebuild; that would unblock wait_join before the merge.
                    pending.append(
                        {k: descriptor[k] for k in ("run_id", "persona", "phase", "notified")}
                    )
                    continue
                # Terminal long enough that finish() never completed (crash):
                # rebuild so the caller is not stuck forever.
                rebuilt = _rebuild_result(descriptor)
                if rebuilt is not None:
                    seen.add(rid)
                    results.append(rebuilt)
                    continue
            pending.append({k: descriptor[k] for k in ("run_id", "persona", "phase", "notified")})
        elif _in_notify_gap(descriptor, now):
            # Transient finish() gap — entry lands momentarily.
            pending.append({k: descriptor[k] for k in ("run_id", "persona", "phase", "notified")})
        elif wanted is not None:
            # Explicitly targeted, notified long ago, no entry: consumed by an
            # earlier wait — rebuild and flag it.
            rebuilt = _rebuild_result(descriptor)
            if rebuilt is not None:
                seen.add(rid)
                results.append(rebuilt)
        # else ("all"): consumed by an earlier wait — not outstanding.

    # Ledger the single source of delivery: any run whose result we hand back
    # here (by drain OR by rebuild) is marked delivered_to_parent, so the
    # deferred child_report merge (chats._merge_child_inbox) never re-delivers
    # the same report as a fresh roll. This closes the finish()-gap rebuild race
    # where the inbox entry lands *after* a two-strike rebuild already delivered.
    for r in results:
        runner.mark_delivered_to_parent(r.get("run_id"))

    return {"results": results, "pending": pending}


def stamp_waiting(
    chat_id: str, parent_kind: str, parent_id: str, pending: list[dict]
) -> None:
    """Mark the parent's state as suspended in a wait_join: the orphan sweep
    skips it and the hop's watchdog credits the wait out of its timeout."""
    ids = [c["run_id"] for c in pending]

    def _stamp(state: dict) -> dict:
        state["waiting_on"] = ids
        state["waiting_since"] = time.time()
        return state

    _state_obj_for(chat_id, parent_kind, parent_id).update(_stamp)


def clear_waiting(chat_id: str, parent_kind: str, parent_id: str) -> None:
    def _clear(state: dict) -> dict:
        state.pop("waiting_on", None)
        state.pop("waiting_since", None)
        return state

    _state_obj_for(chat_id, parent_kind, parent_id).update(_clear)
