"""Spawn sub-agent runs and finish them back to the parent."""

from __future__ import annotations

import logging
import time
import uuid
from pathlib import Path

from crack_server import paths, queue
from crack_server.sub_agents.constants import MAX_DEPTH, SUBAGENT_JOB_SLUG
from crack_server.sub_agents import registry, signals

logger = logging.getLogger("uvicorn.error")

_REPORT_EXCERPT_CHARS = 4000


def format_child_result(entry: dict) -> str:
    """One canonical handoff message for a finished child run."""
    run_id = entry.get("run_id", "?")
    status = entry.get("status", "unknown")
    persona = entry.get("persona", "")
    last_message = (entry.get("last_message") or "").strip() or "(no assistant message)"
    excerpt = (entry.get("report_excerpt") or "").strip()
    lines = [
        f"Sub-agent run {run_id} ({persona}) finished with status: {status}.",
        "",
        "Last assistant message:",
        last_message,
    ]
    if excerpt:
        lines += ["", "Report excerpt:", excerpt]
    report_path = entry.get("report_path")
    if report_path:
        lines += ["", f"Full report path: {report_path}"]
    return "\n".join(lines)


def _last_assistant_message(state: dict) -> str:
    turns = state.get("turns") or []
    for turn in reversed(turns):
        text = (turn.get("text") or "").strip()
        if text:
            return text
    return ""


def _report_excerpt(report_path: str) -> str:
    path = Path(report_path)
    if not path.is_file():
        return ""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return ""
    if len(text) <= _REPORT_EXCERPT_CHARS:
        return text
    return text[:_REPORT_EXCERPT_CHARS] + "\n…(truncated)"


def build_entry(run_id: str, state: dict | None = None, status: str | None = None) -> dict:
    """The canonical child-result entry for a run — the exact shape finish()
    inboxes to the parent. wait.py uses it to rebuild a result whose inbox
    entry was already consumed (a ``delivered_earlier`` rebuild): the entry is
    derivable from run state alone, so nothing is lost when the inbox copy is
    gone."""
    if state is None:
        state = paths.run_state_by_id(run_id).read()
    report_path = state.get("report_path", "")
    return {
        "run_id": run_id,
        "persona": state.get("persona", ""),
        "status": status or state.get("phase", "unknown"),
        "last_message": _last_assistant_message(state),
        "report_excerpt": _report_excerpt(report_path),
        "report_path": report_path,
    }


def spawn(
    *,
    chat_id: str,
    persona_slug: str,
    instructions: str,
    parent_kind: str,
    parent_id: str,
    depth: int,
) -> dict:
    """Create a run directory, link it to the parent, and enqueue run_start.

    ``depth`` is the *caller's* depth (0 for a chat). The child run is stored at
    ``depth + 1``. Spawning is rejected when the child would exceed ``MAX_DEPTH``.
    """
    if parent_kind not in ("chat", "run"):
        raise ValueError("parent_kind must be 'chat' or 'run'")
    persona = registry.get(persona_slug)
    if persona is None:
        raise ValueError(f"unknown persona: {persona_slug}")

    # Authoritative child depth from parent (caller-supplied depth is advisory).
    if parent_kind == "chat":
        if not paths.chat_dir(chat_id).is_dir():
            raise ValueError(f"chat not found: {chat_id}")
        parent_depth = 0
    else:
        parent_state = paths.run_state_by_id(parent_id).read()
        if not parent_state:
            raise ValueError(f"parent run not found: {parent_id}")
        if parent_state.get("phase") in ("done", "error", "stopped"):
            raise ValueError(f"parent run is terminal: {parent_id}")
        parent_depth = int(parent_state.get("depth", 0))
    child_depth = parent_depth + 1
    if child_depth > MAX_DEPTH:
        raise ValueError(
            f"depth {child_depth} exceeds maximum {MAX_DEPTH} for spawning"
        )

    run_id = paths.generate_run_id()
    report_path = paths.run_report_path(chat_id, run_id).resolve()
    run_directory = paths.run_dir(chat_id, run_id)
    run_directory.mkdir(parents=True, exist_ok=True)
    (run_directory / "sessions").mkdir(parents=True, exist_ok=True)

    token = uuid.uuid4().hex
    now = time.time()
    state = {
        "run_id": run_id,
        "persona": persona_slug,
        "chat_id": chat_id,
        "parent_kind": parent_kind,
        "parent_id": parent_id,
        "depth": child_depth,
        "instructions": instructions,
        "report_path": str(report_path),
        "phase": "running",
        "started_token": token,
        "stop_requested": False,
        "nudge_count": 0,
        "hops_completed": 0,
        "children": [],
        "rounds": [],
        "turns": [],
        "child_inbox": [],
        "error": "",
        "error_detail": "",
        "error_step": "",
        "finished_at": None,
        "created_at": now,
    }
    paths.run_state(chat_id, run_id).write(state)

    if parent_kind == "run":
        parent_state = paths.run_state_by_id(parent_id)

        def _link_parent(s: dict) -> dict:
            children = list(s.get("children") or [])
            if run_id not in children:
                children.append(run_id)
            s["children"] = children
            return s

        parent_state.update(_link_parent)

    queue.enqueue_exclusive(
        chat_id,
        SUBAGENT_JOB_SLUG,
        "run_start",
        {"run_id": run_id, "started_token": token},
        run_id=run_id,
    )
    return dict(state)


def finish(run_id: str, status: str) -> None:
    """Idempotent terminal handoff: inbox the result and resume the parent."""
    state_obj = paths.run_state_by_id(run_id)
    state = state_obj.read()
    if not state:
        return
    if state.get("parent_notified"):
        return

    chat_id = state.get("chat_id", "")

    def _terminal(s: dict) -> dict:
        if s.get("phase") not in ("done", "error", "stopped"):
            if status == "done":
                s["phase"] = "done"
            elif status == "stopped":
                s["phase"] = "stopped"
            else:
                s["phase"] = "error"
        s.setdefault("finished_at", time.time())
        s["parent_notified"] = True
        return s

    state_obj.update(_terminal)
    state = state_obj.read()

    entry = build_entry(run_id, state, status=status)

    parent_kind = state.get("parent_kind")
    parent_id = state.get("parent_id")

    if parent_kind == "chat":
        chat_state = paths.chat_state(chat_id)

        def _inbox_chat(s: dict) -> dict:
            inbox = list(s.get("child_inbox") or [])
            inbox.append(entry)
            s["child_inbox"] = inbox
            return s

        chat_state.update(_inbox_chat)
        from crack_server import chats

        queue.enqueue_exclusive(chat_id, chats.CHAT_JOB_SLUG, "drain_children")
        # Only after the inbox write is durable: wake blocking wait_join polls.
        signals.notify_parent(parent_kind, parent_id)
        return

    if parent_kind == "run":
        parent_state = paths.run_state_by_id(parent_id)

        def _inbox_run(s: dict) -> dict:
            inbox = list(s.get("child_inbox") or [])
            inbox.append(entry)
            s["child_inbox"] = inbox
            children = list(s.get("children") or [])
            if run_id in children:
                children.remove(run_id)
            s["children"] = children
            return s

        parent_state.update(_inbox_run)
        parent_persona = registry.get(parent_state.read().get("persona", ""))
        if parent_persona is not None:
            parent_persona.enqueue_step(
                parent_id,
                "drain_children",
                {
                    "run_id": parent_id,
                    "started_token": parent_state.read().get("started_token"),
                },
            )
        # Only after the inbox write is durable: wake blocking wait_join polls.
        signals.notify_parent(parent_kind, parent_id)
        return

    logger.warning("finish: run %s has unknown parent_kind %r", run_id, parent_kind)
