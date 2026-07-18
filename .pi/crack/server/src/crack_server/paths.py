"""Resolve project paths and list prompt markdown files."""

from __future__ import annotations

import json
import os
import re
import time
from pathlib import Path

TASK_ID_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$")
PROMPT_NAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*\.md$")
STAGE_SLUG_RE = re.compile(r"^[a-z0-9_]+$")
PLAN_ARTEFACT_NAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*\.(md|json|txt)$")
INFO_FILENAME = "info.json"
TITLE_REGEN_FILENAME = "title_regen.json"
EXPLORE_FILENAME = "explore.json"
PLAN_FILENAME = "plan.json"
PLAN_REVIEW_FILENAME = "plan_review.json"


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


def _atomic_write_json(path: Path, data: dict) -> None:
    directory = path.parent
    directory.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
    os.replace(tmp, path)


def title_regen_path(task_id: str, root: Path | None = None) -> Path:
    return task_dir(task_id, root) / TITLE_REGEN_FILENAME


def read_title_regen_state(task_id: str, root: Path | None = None) -> dict:
    path = title_regen_path(task_id, root)
    if not path.is_file():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def write_title_regen_state(task_id: str, state: dict, root: Path | None = None) -> None:
    _atomic_write_json(title_regen_path(task_id, root), state)


def explore_path(task_id: str, root: Path | None = None) -> Path:
    return task_dir(task_id, root) / EXPLORE_FILENAME


def read_explore_state(task_id: str, root: Path | None = None) -> dict:
    path = explore_path(task_id, root)
    if not path.is_file():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def write_explore_state(task_id: str, state: dict, root: Path | None = None) -> None:
    _atomic_write_json(explore_path(task_id, root), state)


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


# ---------------------------------------------------------------------------
# Harness: models cache, per-stage config, stage prompt templates
# ---------------------------------------------------------------------------


def templates_dir() -> Path:
    """Prompt templates root, inside the server package repo (prompt_templates/)."""
    return Path(__file__).resolve().parent.parent.parent / "prompt_templates"


def harness_dir(root: Path | None = None) -> Path:
    """Harness-wide state dir: .pi/crack/harness/ (models cache, stage configs)."""
    return (root or project_root()) / ".pi" / "crack" / "harness"


def models_cache_path(root: Path | None = None) -> Path:
    return harness_dir(root) / "models_list.json"


def read_models_cache(root: Path | None = None) -> dict:
    path = models_cache_path(root)
    if not path.is_file():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def write_models_cache(data: dict, root: Path | None = None) -> None:
    _atomic_write_json(models_cache_path(root), data)


def _validate_stage_slug(slug: str) -> str:
    if not STAGE_SLUG_RE.fullmatch(slug):
        raise ValueError("invalid stage slug")
    return slug


def stage_config_path(slug: str, root: Path | None = None) -> Path:
    return harness_dir(root) / f"{_validate_stage_slug(slug)}.json"


def read_stage_config(slug: str, root: Path | None = None) -> dict:
    path = stage_config_path(slug, root)
    if not path.is_file():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def write_stage_config(slug: str, config: dict, root: Path | None = None) -> None:
    _atomic_write_json(stage_config_path(slug, root), config)


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


def plan_path(task_id: str, root: Path | None = None) -> Path:
    return task_dir(task_id, root) / PLAN_FILENAME


def read_plan_state(task_id: str, root: Path | None = None) -> dict:
    path = plan_path(task_id, root)
    if not path.is_file():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def write_plan_state(task_id: str, state: dict, root: Path | None = None) -> None:
    _atomic_write_json(plan_path(task_id, root), state)


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


def plan_review_path(task_id: str, root: Path | None = None) -> Path:
    return task_dir(task_id, root) / PLAN_REVIEW_FILENAME


def read_plan_review_state(task_id: str, root: Path | None = None) -> dict:
    path = plan_review_path(task_id, root)
    if not path.is_file():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def write_plan_review_state(task_id: str, state: dict, root: Path | None = None) -> None:
    _atomic_write_json(plan_review_path(task_id, root), state)


def plan_review_sessions_dir(task_id: str, root: Path | None = None) -> Path:
    """Pi session dir for plan-review critic hops: …/<task>/plan/review_sessions/."""
    return plan_dir(task_id, root) / "review_sessions"
