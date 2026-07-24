"""Context-window guard: force-stop chat / sub-agent runs at 75 % of the model's window."""
from __future__ import annotations

import logging
import time
from pathlib import Path

from crack_server import models as models_mod
from crack_server.context_stats import session_usage
from crack_server.state import JsonState

logger = logging.getLogger("uvicorn.error")

FORCE_STOP_REASON = "force_stopped_ctx"
FORCE_STOP_THRESHOLD = 0.75  # 75 %


def needs_compaction(sessions_dir: Path, model: str) -> bool:
    """Return True when rolling compaction should run before the next hop."""
    from crack_server import compaction

    return compaction.should_compact(sessions_dir, model)


def build_force_stop_message(elapsed_seconds: float) -> str:
    """Return the canonical 'Force Stopped: … Ran for X minutes' string."""
    minutes = elapsed_seconds / 60.0
    return (
        f"Force Stopped: Reached 75% of context window. "
        f"Ran for {minutes:.1f} minutes"
    )


def _check_once(
    sessions_dir: Path,
    model: str,
    now: float,
    started_at: float | None,
) -> str | None:
    window = models_mod.context_window(model)
    usage = session_usage(sessions_dir)
    if usage is None:
        return None
    tokens = int(usage.get("tokens") or 0)
    if tokens <= 0:
        return None
    ratio = tokens / float(window)
    if ratio < FORCE_STOP_THRESHOLD:
        return None
    elapsed = now - (started_at if started_at else now)
    return build_force_stop_message(elapsed)


def check_force_stop(
    sessions_dir: Path,
    model: str,
    now: float | None = None,
    started_at: float | None = None,
) -> str | None:
    """Return the force-stop message, or None if checks pass."""
    _now = now if now is not None else time.time()
    return _check_once(sessions_dir, model, _now, started_at)


# ----------------- helpers that mutate persistent state (chat / sub-agent) -----------------

def force_stop_chat(
    chat_state: JsonState,
    sessions_dir: Path,
    model: str,
    exchange_idx: int | None,
    started_at: float | None,
) -> str | None:
    msg = check_force_stop(sessions_dir, model, started_at=started_at)
    if msg is None:
        return None
    now = time.time()
    elapsed = now - (started_at if started_at else now)

    def _upd(s: dict) -> dict:
        s["stop_requested"] = True
        if s.get("phase") == "chatting":
            s["phase"] = "idle"
        s["error"] = msg
        s["error_detail"] = ""
        exs = s.get("exchanges") or []
        if exchange_idx is not None and 0 <= exchange_idx < len(exs):
            exs[exchange_idx]["stop_reason"] = FORCE_STOP_REASON
            exs[exchange_idx]["finished_at"] = now
        return s

    chat_state.update(_upd)
    logger.warning("context_guard: force-stopping chat at 75%% ctx (model=%s)", model)
    return msg


def force_stop_subagent(
    state: dict,
    sessions_dir: Path,
    model: str,
    started_at: float | None,
) -> str | None:
    msg = check_force_stop(sessions_dir, model, started_at=started_at)
    if msg is None:
        return None
    state["stop_requested"] = True
    state["error"] = msg
    state["error_detail"] = ""
    state["phase"] = "stopped"
    state["finished_at"] = time.time()
    logger.warning("context_guard: force-stopping sub-agent at 75%% ctx (model=%s)", model)
    return msg
