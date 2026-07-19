"""Resolve project paths and list prompt markdown files.

JSON state-file I/O (tolerant read / atomic write / locked update) lives in
``state.py``; this module keeps path construction plus one-line
:class:`~crack_server.state.JsonState` accessors for each state file.
"""

from __future__ import annotations

import json
import os
import re
import time
from pathlib import Path

from crack_server.state import (
    CHAT_STATE_FILENAME,
    EXPLORE_FILENAME,
    FINISHED_FILENAME,
    IMPL_REVIEW_FILENAME,
    IMPLEMENTATION_FILENAME,
    INFO_FILENAME,
    PLAN_FILENAME,
    PLAN_REVIEW_FILENAME,
    RUN_STATE_FILENAME,
    TITLE_REGEN_FILENAME,
    JsonState,
)

TASK_ID_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$")
PROMPT_NAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*\.md$")
STAGE_SLUG_RE = re.compile(r"^[a-z0-9_]+$")
PLAN_ARTEFACT_NAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*\.(md|json|txt)$")
CHAT_ID_RE = re.compile(r"^\d{13,}(_\d+)?$")
PERSONA_SLUG_RE = re.compile(r"^[a-z0-9_]+$")
RUN_ID_RE = re.compile(r"^\d{13,}_[0-9a-f]{8}$")


def project_root() -> Path:
    raw = os.environ.get("CRACK_PI_PROJECT_ROOT", os.getcwd())
    return Path(raw).expanduser().resolve()


def tasks_dir(root: Path | None = None) -> Path:
    return (root or project_root()) / ".pi" / "crack" / "tasks"


def task_dir(task_id: str, root: Path | None = None) -> Path:
    if not TASK_ID_RE.fullmatch(task_id):
        raise ValueError("invalid task_id")
    return tasks_dir(root) / task_id


def validate_prompt_filename(name: str) -> str:
    base = Path(name).name
    if not PROMPT_NAME_RE.fullmatch(base):
        raise ValueError("invalid prompt filename")
    return base


def list_task_ids(root: Path | None = None) -> list[str]:
    base = tasks_dir(root)
    if not base.is_dir():
        return []
    return sorted(p.name for p in base.iterdir() if p.is_dir())


def list_prompt_files(task_id: str, root: Path | None = None) -> list[dict[str, str | int]]:
    """Glob *.md in the task directory on every call."""
    directory = task_dir(task_id, root)
    directory.mkdir(parents=True, exist_ok=True)
    paths = sorted(directory.glob("*.md"), key=lambda p: p.name.lower())
    out: list[dict[str, str | int]] = []
    for path in paths:
        try:
            stat = path.stat()
        except OSError:
            continue
        out.append(
            {
                "name": path.name,
                "size": stat.st_size,
                "mtime": int(stat.st_mtime),
            }
        )
    return out


def read_prompt(task_id: str, filename: str, root: Path | None = None) -> str:
    fname = validate_prompt_filename(filename)
    path = task_dir(task_id, root) / fname
    if not path.is_file():
        raise FileNotFoundError(fname)
    return path.read_text(encoding="utf-8")


def write_prompt(task_id: str, filename: str, content: str, root: Path | None = None) -> None:
    fname = validate_prompt_filename(filename)
    directory = task_dir(task_id, root)
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / fname
    path.write_text(content, encoding="utf-8")


def delete_prompt(task_id: str, filename: str, root: Path | None = None) -> None:
    fname = validate_prompt_filename(filename)
    path = task_dir(task_id, root) / fname
    if not path.is_file():
        raise FileNotFoundError(fname)
    path.unlink()


def info_path(task_id: str, root: Path | None = None) -> Path:
    return task_dir(task_id, root) / INFO_FILENAME


def read_info(task_id: str, root: Path | None = None) -> dict:
    path = info_path(task_id, root)
    if not path.is_file():
        return {"created_at": time.time(), "modified_at": time.time(), "title": task_id}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {"created_at": time.time(), "modified_at": time.time(), "title": task_id}


def write_info(task_id: str, info: dict, root: Path | None = None) -> None:
    path = info_path(task_id, root)
    directory = path.parent
    directory.mkdir(parents=True, exist_ok=True)
    info.setdefault("created_at", time.time())
    info["modified_at"] = time.time()
    path.write_text(json.dumps(info, indent=2), encoding="utf-8")


def title_regen_state(task_id: str, root: Path | None = None) -> JsonState:
    return JsonState(task_dir(task_id, root) / TITLE_REGEN_FILENAME)


def explore_state(task_id: str, root: Path | None = None) -> JsonState:
    return JsonState(task_dir(task_id, root) / EXPLORE_FILENAME)


def explore_dir(task_id: str, root: Path | None = None) -> Path:
    """Per-task directory for Explore artefacts: …/<task>/explore/."""
    return task_dir(task_id, root) / "explore"


def explore_sessions_dir(task_id: str, root: Path | None = None) -> Path:
    """Isolated pi session dir used to chain Explore hops: …/<task>/explore/sessions/."""
    return explore_dir(task_id, root) / "sessions"


def write_explore_artefact(task_id: str, name: str, text: str, root: Path | None = None) -> None:
    """Write an Explore artefact as …/<task>/explore/{name}.md (name is sanitized)."""
    safe = re.sub(r"[^a-zA-Z0-9_-]+", "_", name).strip("_") or "artefact"
    directory = explore_dir(task_id, root)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / f"{safe}.md").write_text(text, encoding="utf-8")


def prompts_last_modified(task_id: str, root: Path | None = None) -> float:
    """Newest mtime (epoch seconds) across the task's prompt files; 0.0 when none."""
    latest = 0.0
    for p in list_prompt_files(task_id, root):
        latest = max(latest, float(p["mtime"]))
    return latest


def read_all_prompts_joined(task_id: str, root: Path | None = None) -> str:
    """Read all prompt markdown files in a task and join them with `\n\n---\n\n`."""
    contents = []
    for p in list_prompt_files(task_id, root):
        try:
            contents.append(read_prompt(task_id, str(p["name"]), root))
        except FileNotFoundError:
            continue  # deleted between listing and reading
    return "\n\n---\n\n".join(contents)


def slugify_title(title: str) -> str:
    """Replace runs of non-alphanumeric characters with '_', stripped at the ends."""
    slug = re.sub(r"[^a-zA-Z0-9]+", "_", title).strip("_")
    return slug or "task"


def generate_task_id(title: str) -> str:
    """Task id format: <ms_epoch_timestamp>_<slugified_title>."""
    return f"{int(time.time() * 1000)}_{slugify_title(title)}"


def create_task(task_id: str, title: str | None = None, root: Path | None = None) -> dict:
    """Create a new task directory with info.json."""
    if not TASK_ID_RE.fullmatch(task_id):
        raise ValueError("invalid task_id")
    directory = task_dir(task_id, root)
    if directory.exists():
        raise ValueError("task already exists")
    directory.mkdir(parents=True, exist_ok=True)
    now = time.time()
    info = {
        "created_at": now,
        "modified_at": now,
        "title": title or task_id,
    }
    write_info(task_id, info, root)
    return info


def next_prompt_filename(task_id: str, root: Path | None = None) -> str | None:
    """Return the next available prompt filename (prompt.md, prompt2.md...prompt9.md)."""
    directory = task_dir(task_id, root)
    directory.mkdir(parents=True, exist_ok=True)
    existing = {p.name for p in directory.glob("*.md")}
    for i in range(1, 10):
        name = "prompt.md" if i == 1 else f"prompt{i}.md"
        if name not in existing:
            return name
    return None


def stage_pid_file(task_id: str, slug: str, root: Path | None = None) -> Path:
    """Where a stage's worker publishes the running pi subprocess's pid so the
    web STOP handler can kill it: tasks/<id>/<slug>.agent.pid."""
    return task_dir(task_id, root) / f"{_validate_stage_slug(slug)}.agent.pid"


# ---------------------------------------------------------------------------
# Harness: models cache, per-stage config, stage prompt templates
# ---------------------------------------------------------------------------


def templates_dir() -> Path:
    """Prompt templates root, inside the server package repo (prompt_templates/)."""
    return Path(__file__).resolve().parent.parent.parent / "prompt_templates"


def harness_dir(root: Path | None = None) -> Path:
    """Harness-wide state dir: .pi/crack/harness/ (models cache, stage configs)."""
    return (root or project_root()) / ".pi" / "crack" / "harness"


def models_cache_state(root: Path | None = None) -> JsonState:
    # The harness root is durable (unlike task/chat dirs) — create it so the
    # first cache write has a parent; JsonState.write refuses to create one.
    harness_dir(root).mkdir(parents=True, exist_ok=True)
    return JsonState(harness_dir(root) / "models_list.json")


# ---------------------------------------------------------------------------
# Worker command queue (filesystem job queue under harness/queue/)
# ---------------------------------------------------------------------------


def queue_dir(root: Path | None = None) -> Path:
    """Root of the on-disk worker command queue: .pi/crack/harness/queue/."""
    return harness_dir(root) / "queue"


def queue_pending_dir(root: Path | None = None) -> Path:
    return queue_dir(root) / "pending"


def queue_processing_dir(root: Path | None = None) -> Path:
    return queue_dir(root) / "processing"


def worker_lock_path(root: Path | None = None) -> Path:
    """Single-instance flock file for the worker process."""
    return harness_dir(root) / "worker.lock"


def _validate_stage_slug(slug: str) -> str:
    if not STAGE_SLUG_RE.fullmatch(slug):
        raise ValueError("invalid stage slug")
    return slug


def stage_config_state(slug: str, root: Path | None = None) -> JsonState:
    harness_dir(root).mkdir(parents=True, exist_ok=True)  # see models_cache_state
    return JsonState(harness_dir(root) / f"{_validate_stage_slug(slug)}.json")


def stage_templates_dir(slug: str) -> Path:
    """Per-stage prompt template dir: prompt_templates/<slug>/."""
    return templates_dir() / _validate_stage_slug(slug)


def list_stage_templates(slug: str) -> list[dict[str, str | int]]:
    """Glob *.md in the stage's template dir on every call."""
    directory = stage_templates_dir(slug)
    out: list[dict[str, str | int]] = []
    if not directory.is_dir():
        return out
    for path in sorted(directory.glob("*.md"), key=lambda p: p.name.lower()):
        try:
            stat = path.stat()
        except OSError:
            continue
        out.append({"name": path.name, "size": stat.st_size, "mtime": int(stat.st_mtime)})
    return out


def read_stage_template(slug: str, filename: str) -> str:
    fname = validate_prompt_filename(filename)
    path = stage_templates_dir(slug) / fname
    if not path.is_file():
        raise FileNotFoundError(fname)
    return path.read_text(encoding="utf-8")


def write_stage_template(slug: str, filename: str, content: str) -> None:
    fname = validate_prompt_filename(filename)
    directory = stage_templates_dir(slug)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / fname).write_text(content, encoding="utf-8")


# ---------------------------------------------------------------------------
# Plan stage: per-task state and artefacts
# ---------------------------------------------------------------------------


def plan_state(task_id: str, root: Path | None = None) -> JsonState:
    return JsonState(task_dir(task_id, root) / PLAN_FILENAME)


def plan_dir(task_id: str, root: Path | None = None) -> Path:
    """Per-task directory for Plan artefacts: …/<task>/plan/."""
    return task_dir(task_id, root) / "plan"


def plan_sessions_dir(task_id: str, root: Path | None = None) -> Path:
    """Isolated pi session dir used to chain Plan draft steps: …/<task>/plan/sessions/."""
    return plan_dir(task_id, root) / "sessions"


def write_plan_artefact(task_id: str, name: str, text: str, root: Path | None = None) -> None:
    """Write a Plan artefact as …/<task>/plan/{name} (basename, .md/.json/.txt only)."""
    base = Path(name).name
    if not PLAN_ARTEFACT_NAME_RE.fullmatch(base):
        raise ValueError("invalid plan artefact name")
    directory = plan_dir(task_id, root)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / base).write_text(text, encoding="utf-8")


def read_plan_artefact(task_id: str, name: str, root: Path | None = None) -> str:
    """Read a Plan artefact from …/<task>/plan/{name} (basename, .md/.json/.txt only)."""
    base = Path(name).name
    if not PLAN_ARTEFACT_NAME_RE.fullmatch(base):
        raise ValueError("invalid plan artefact name")
    path = plan_dir(task_id, root) / base
    if not path.is_file():
        raise FileNotFoundError(base)
    return path.read_text(encoding="utf-8")


def plan_todo_path(task_id: str, root: Path | None = None) -> Path:
    """Path to the generated implementation checklist: …/<task>/plan/todo.md."""
    return plan_dir(task_id, root) / "todo.md"


def plan_review_state(task_id: str, root: Path | None = None) -> JsonState:
    return JsonState(task_dir(task_id, root) / PLAN_REVIEW_FILENAME)


def plan_review_sessions_dir(task_id: str, root: Path | None = None) -> Path:
    """Pi session dir for plan-review critic hops: …/<task>/plan/review_sessions/."""
    return plan_dir(task_id, root) / "review_sessions"


def walkthrough_path(task_id: str, root: Path | None = None) -> Path:
    """The implementation/review retrospective: …/<task>/plan/walkthrough.md."""
    return plan_dir(task_id, root) / "walkthrough.md"


def read_walkthrough(task_id: str, root: Path | None = None) -> str:
    path = walkthrough_path(task_id, root)
    if not path.is_file():
        return ""
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


# ---------------------------------------------------------------------------
# Implementation stage: per-task state and pi session dir
# ---------------------------------------------------------------------------


def implementation_state(task_id: str, root: Path | None = None) -> JsonState:
    return JsonState(task_dir(task_id, root) / IMPLEMENTATION_FILENAME)


def implementation_sessions_dir(task_id: str, root: Path | None = None) -> Path:
    """Pi session dir for implementation agent hops: …/<task>/implementation/sessions/."""
    return task_dir(task_id, root) / "implementation" / "sessions"


# ---------------------------------------------------------------------------
# Implementation Review stage: per-task state and pi session dir
# ---------------------------------------------------------------------------


def impl_review_state(task_id: str, root: Path | None = None) -> JsonState:
    return JsonState(task_dir(task_id, root) / IMPL_REVIEW_FILENAME)


def impl_review_sessions_dir(task_id: str, root: Path | None = None) -> Path:
    """Pi session dir for the review agent: …/<task>/implementation/review_sessions/."""
    return task_dir(task_id, root) / "implementation" / "review_sessions"


# ---------------------------------------------------------------------------
# Finished stage: per-task chat state (resumes the review session)
# ---------------------------------------------------------------------------


def finished_state(task_id: str, root: Path | None = None) -> JsonState:
    return JsonState(task_dir(task_id, root) / FINISHED_FILENAME)


# ---------------------------------------------------------------------------
# Unscripted chats: free-form pi sessions outside the task pipeline
# ---------------------------------------------------------------------------


def unscripted_chats_dir(root: Path | None = None) -> Path:
    return (root or project_root()) / ".pi" / "crack" / "unscripted_chats"


def chat_dir(chat_id: str, root: Path | None = None) -> Path:
    if not CHAT_ID_RE.fullmatch(chat_id):
        raise ValueError("invalid chat_id")
    return unscripted_chats_dir(root) / chat_id


def list_chat_ids(root: Path | None = None) -> list[str]:
    """Chat ids sorted newest first (ids are ms-epoch prefixed, so name sort = time sort)."""
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
    """Pi session dir for the chat agent: …/unscripted_chats/<chat>/sessions/."""
    return chat_dir(chat_id, root) / "sessions"


def create_chat(chat_id: str, model: str, root: Path | None = None) -> dict:
    """Create a new chat directory with info.json + chat.json; returns the info dict."""
    directory = chat_dir(chat_id, root)
    directory.mkdir(parents=True, exist_ok=False)
    info = {"id": chat_id, "title": "", "model": model, "created_at": time.time()}
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
# Sub-agents: persona definitions and per-run working dirs
# ---------------------------------------------------------------------------


def sub_agents_dir(root: Path | None = None) -> Path:
    """Checked-in persona definitions: .pi/crack/sub_agents/."""
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
    """Run id format: <ms_epoch>_<uuid8>."""
    import uuid

    return f"{int(time.time() * 1000)}_{uuid.uuid4().hex[:8]}"


def run_dir(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    if not RUN_ID_RE.fullmatch(run_id):
        raise ValueError("invalid run_id")
    return chat_sub_agent_runs_dir(chat_id, root) / run_id


def find_run_dir(run_id: str, root: Path | None = None) -> Path:
    """Locate a run directory by id; raises if zero or multiple matches."""
    if not RUN_ID_RE.fullmatch(run_id):
        raise ValueError("invalid run_id")
    base = unscripted_chats_dir(root)
    matches = sorted(base.glob(f"*/sub_agent_runs/{run_id}"))
    if len(matches) != 1:
        raise FileNotFoundError(f"expected exactly one run dir for {run_id!r}, found {len(matches)}")
    return matches[0]


def list_run_ids(chat_id: str, root: Path | None = None) -> list[str]:
    """Run ids under a chat, sorted newest first."""
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


def run_pid_file(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    return run_dir(chat_id, run_id, root) / "agent.pid"


def run_report_path(chat_id: str, run_id: str, root: Path | None = None) -> Path:
    return run_dir(chat_id, run_id, root) / "report.md"
