"""Resolve project paths and list prompt markdown files."""

from __future__ import annotations

import json
import os
import re
import time
from pathlib import Path

TASK_ID_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$")
PROMPT_NAME_RE = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*\.md$")
INFO_FILENAME = "info.json"


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
