"""Resolve project paths for chats, sub-agents, harness queue, and media.

JSON state-file I/O lives in ``state.py``; this module keeps path construction
plus one-line :class:`~crack_server.state.JsonState` accessors.
"""

from __future__ import annotations

import os
import re
import time
from pathlib import Path

from crack_server.state import (
    CHAT_STATE_FILENAME,
    INFO_FILENAME,
    RUN_STATE_FILENAME,
    JsonState,
)

CHAT_ID_RE = re.compile(r"^\d{13,}(_\d+)?$")
PERSONA_SLUG_RE = re.compile(r"^[a-z0-9_]+$")
RUN_ID_RE = re.compile(r"^\d{13,}_[0-9a-f]{8}$")


def project_root() -> Path:
    raw = os.environ.get("CRACK_PI_PROJECT_ROOT", os.getcwd())
    return Path(raw).expanduser().resolve()


def harness_data_root(root: Path | None = None) -> Path:
    """Root for all MUTABLE harness state (chats, runs, sessions, queue, hop I/O).

    Tests pass an explicit ``root`` and get a co-located tree under it. In the
    container, CRACK_HARNESS_DATA_DIR points at the shared /crack-harness-data
    volume. Local dev with neither falls back to the in-repo path (old behavior)."""
    if root is not None:
        return root / ".pi" / "crack"
    proj = project_root()
    env = os.environ.get("CRACK_HARNESS_DATA_DIR")
    if env:
        # Tests monkeypatch CRACK_PI_PROJECT_ROOT to tmp_path while the container
        # still exports CRACK_HARNESS_DATA_DIR — keep harness state co-located.
        pi_root = os.environ.get("CRACK_PI_PROJECT_ROOT")
        if pi_root and Path(pi_root).expanduser().resolve() != Path("/workspace").resolve():
            return proj / ".pi" / "crack"
        return Path(env)
    return proj / ".pi" / "crack"


def templates_dir() -> Path:
    """Prompt templates root (prompt_templates/)."""
    return Path(__file__).resolve().parent.parent.parent / "prompt_templates"


def harness_dir(root: Path | None = None) -> Path:
    """Shared infra dir: harness/ (models cache, queue, locks)."""
    return harness_data_root(root) / "harness"


def models_cache_state(root: Path | None = None) -> JsonState:
    harness_dir(root).mkdir(parents=True, exist_ok=True)
    return JsonState(harness_dir(root) / "models_list.json")


def model_latency_state(root: Path | None = None) -> JsonState:
    """Per-model EMA latency cache (sibling of ``models_list.json``)."""
    harness_dir(root).mkdir(parents=True, exist_ok=True)
    return JsonState(harness_dir(root) / "model_latency.json")


def queue_dir(root: Path | None = None) -> Path:
    return harness_dir(root) / "queue"


def queue_pending_dir(root: Path | None = None) -> Path:
    return queue_dir(root) / "pending"


def queue_processing_dir(root: Path | None = None) -> Path:
    return queue_dir(root) / "processing"


# ---------------------------------------------------------------------------
# Unscripted chats
# ---------------------------------------------------------------------------


def unscripted_chats_dir(root: Path | None = None) -> Path:
    return harness_data_root(root) / "unscripted_chats"


def chat_dir(chat_id: str, root: Path | None = None) -> Path:
    if not CHAT_ID_RE.fullmatch(chat_id):
        raise ValueError("invalid chat_id")
    return unscripted_chats_dir(root) / chat_id


def list_chat_ids(root: Path | None = None) -> list[str]:
    """Chat ids sorted newest first (ids are ms-epoch prefixed)."""
    base = unscripted_chats_dir(root)
    if not base.is_dir():
        return []
    return sorted(
        (p.name for p in base.iterdir() if p.is_dir() and CHAT_ID_RE.fullmatch(p.name)),
        reverse=True,
    )


def generate_chat_id() -> str:
    """Chat id: <ms_epoch_timestamp>. Collides only within the same millisecond."""
    base = int(time.time() * 1000)
    chat_id = str(base)
    n = 0
    while chat_dir(chat_id).exists():
        n += 1
        chat_id = f"{base}_{n}"
    return chat_id


def chat_info_state(chat_id: str, root: Path | None = None) -> JsonState:
    return JsonState(chat_dir(chat_id, root) / INFO_FILENAME)


def chat_state_path(chat_id: str, root: Path | None = None) -> Path:
    return chat_dir(chat_id, root) / CHAT_STATE_FILENAME


def chat_state(chat_id: str, root: Path | None = None) -> JsonState:
    return JsonState(chat_state_path(chat_id, root))


def chat_sessions_dir(chat_id: str, root: Path | None = None) -> Path:
    return chat_dir(chat_id, root) / "sessions"


def create_chat(
    chat_id: str,
    model: str,
    root: Path | None = None,
    *,
    plan: bool = False,
    planner_model: str = "",
    implementer_model: str = "",
) -> dict:
    """Create a new chat directory with info.json + chat.json; returns the info
    dict. ``plan``/``planner_model``/``implementer_model`` are the prewalk model
    choices locked at creation (``model`` is the non-plan / fallback model)."""
    directory = chat_dir(chat_id, root)
    directory.mkdir(parents=True, exist_ok=False)
    info = {
        "id": chat_id,
        "title": "",
        "model": model,
        "plan": bool(plan),
        "planner_model": planner_model or model,
        "implementer_model": implementer_model or model,
        "created_at": time.time(),
    }
    chat_info_state(chat_id, root).write(info)
    chat_state(chat_id, root).write({
        "phase": "idle",
        "exchanges": [],
        "pending": [],
        "child_inbox": [],
        "error": "",
        "error_detail": "",
    })
    return info


# ---------------------------------------------------------------------------
# Sub-agents
# ---------------------------------------------------------------------------


def sub_agents_dir(root: Path | None = None) -> Path:
    return (root or project_root()) / ".pi" / "crack" / "sub_agents"


def _validate_persona_slug(slug: str) -> str:
    if not PERSONA_SLUG_RE.fullmatch(slug):
        raise ValueError("invalid persona slug")
    return slug


def sub_agent_persona_dir(slug: str, root: Path | None = None) -> Path:
    return sub_agents_dir(root) / _validate_persona_slug(slug)


def chat_sub_agent_runs_dir(chat_id: str, root: Path | None = None) -> Path:
    return chat_dir(chat_id, root) / "sub_agent_runs"


def generate_run_id() -> str:
    import uuid

    return f"{int(time.time() * 1000)}_{uuid.uuid4().hex[:8]}"


def run_dir(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    if not RUN_ID_RE.fullmatch(run_id):
        raise ValueError("invalid run_id")
    return chat_sub_agent_runs_dir(chat_id, root) / run_id


def find_run_dir(run_id: str, root: Path | None = None) -> Path:
    if not RUN_ID_RE.fullmatch(run_id):
        raise ValueError("invalid run_id")
    base = unscripted_chats_dir(root)
    matches = sorted(base.glob(f"*/sub_agent_runs/{run_id}"))
    if len(matches) != 1:
        raise FileNotFoundError(f"expected exactly one run dir for {run_id!r}, found {len(matches)}")
    return matches[0]


def list_run_ids(chat_id: str, root: Path | None = None) -> list[str]:
    directory = chat_sub_agent_runs_dir(chat_id, root)
    if not directory.is_dir():
        return []
    return sorted(
        (p.name for p in directory.iterdir() if p.is_dir() and RUN_ID_RE.fullmatch(p.name)),
        reverse=True,
    )


def run_state(chat_id: str, run_id: str, root: Path | None = None) -> JsonState:
    return JsonState(run_dir(chat_id, run_id, root) / RUN_STATE_FILENAME)


def run_state_by_id(run_id: str, root: Path | None = None) -> JsonState:
    return JsonState(find_run_dir(run_id, root) / RUN_STATE_FILENAME)


def run_sessions_dir(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    return run_dir(chat_id, run_id, root) / "sessions"


# Cap the per-conversation trajectory-note ring so a long-lived chat can't grow
# its state file without bound (notes are UI-only; oldest fall off first).
MAX_TRAJ_NOTES = 400


def append_traj_note(
    state_obj: JsonState,
    note_type: str,
    text: str,
    *,
    icon: str = "",
    detail: str = "",
    status: str = "",
    at: float | None = None,
) -> None:
    """Append a UI-only trajectory note to a chat/run state's ``traj_notes`` ring.

    Notes are harness-authored markers (a sub-agent returning, a patch being
    built/applied) that interleave into the rendered trajectory by their ``at``
    timestamp — the same sidecar mechanism as recorded errors and ask_user Q&A.
    They are never fed back into agent context. ``kind="note"`` matches the
    render dispatch in ``render.render_turn_msgs``.
    """
    note = {
        "kind": "note",
        "note_type": note_type,
        "text": text,
        "at": float(at) if at is not None else time.time(),
    }
    if icon:
        note["icon"] = icon
    if detail:
        note["detail"] = detail
    if status:
        note["status"] = status

    def _append(state: dict) -> dict:
        notes = list(state.get("traj_notes") or [])
        notes.append(note)
        if len(notes) > MAX_TRAJ_NOTES:
            notes = notes[-MAX_TRAJ_NOTES:]
        state["traj_notes"] = notes
        return state

    state_obj.update(_append)


def run_pid_file(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    return run_dir(chat_id, run_id, root) / "agent.pid"


def run_report_path(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    return run_dir(chat_id, run_id, root) / "report.md"


# ---------------------------------------------------------------------------
# Media and attachments
# ---------------------------------------------------------------------------


def chat_media_dir(chat_id: str, root: Path | None = None) -> Path:
    return chat_dir(chat_id, root) / "media"


def run_media_dir(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    return run_dir(chat_id, run_id, root) / "media"


def chat_attachments_dir(chat_id: str, root: Path | None = None) -> Path:
    return chat_dir(chat_id, root) / "attachments"


def chat_attachments_state(chat_id: str, root: Path | None = None) -> JsonState:
    return JsonState(chat_attachments_dir(chat_id, root) / "images.json")
