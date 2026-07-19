"""Generic JSON state-file store.

Every pipeline stage, the title-regen job, the split job, and the unscripted chats persist
their state as one JSON dict per file (``explore.json``, ``plan.json``,
``chat.json``, …). This module centralizes the three operations those files
need, replacing the eleven near-identical read/write pairs that used to live
in ``paths.py``:

- :meth:`JsonState.read` — tolerant read: ``{}`` on a missing or corrupt file.
- :meth:`JsonState.write` — atomic whole-file write (tmp + ``os.replace``).
- :meth:`JsonState.update` — read-modify-write under a per-path ``flock``
  (``<path>.lock``), so the web process and the out-of-process worker can't
  silently revert each other's fields (B3). The lock is held only for the
  read-modify-write cycle, never during pi work.
"""

from __future__ import annotations

import fcntl
import json
import logging
import os
from pathlib import Path
from typing import Callable

logger = logging.getLogger("uvicorn.error")

# Canonical state filenames (paths.py builds its paths from these).
INFO_FILENAME = "info.json"
CHAT_STATE_FILENAME = "chat.json"
TITLE_REGEN_FILENAME = "title_regen.json"
SPLIT_FILENAME = "split.json"
EXPLORE_FILENAME = "explore.json"
PLAN_FILENAME = "plan.json"
PLAN_REVIEW_FILENAME = "plan_review.json"
IMPLEMENTATION_FILENAME = "implementation.json"
IMPL_REVIEW_FILENAME = "impl_review.json"
FINISHED_FILENAME = "finished.json"


class JsonState:
    """A single JSON-dict state file with atomic writes and locked updates."""

    def __init__(self, path: Path):
        self.path = Path(path)

    def read(self) -> dict:
        """Tolerant read: ``{}`` when the file is missing or unparseable."""
        if not self.path.is_file():
            return {}
        try:
            data = json.loads(self.path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {}
        return data if isinstance(data, dict) else {}

    def write(self, data: dict) -> None:
        """Atomic whole-file write (tmp file + ``os.replace``).

        B7 groundwork: the parent dir (the task/chat dir, or the harness root)
        is always created before any state file is written, so a missing parent
        means the task/chat was deleted mid-run — a straggler worker write must
        NOT ``mkdir(parents=True)`` the directory back into existence. Skip and
        log instead. (First writes under ``.pi/crack/harness/`` are covered by
        the ``paths`` accessors, which create that durable root explicitly.)
        """
        if not self.path.parent.is_dir():
            logger.warning(
                "state: refusing to write %s — parent dir is gone (deleted task/chat?)",
                self.path,
            )
            return
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        os.replace(tmp, self.path)

    def update(self, fn: Callable[[dict], dict]) -> dict:
        """Read-modify-write under an exclusive per-path flock.

        ``fn`` receives the current state dict and returns the new one, which
        is written atomically while the lock is still held. Works across the
        web process and the out-of-process worker (``fcntl.flock`` on
        ``<path>.lock``). Returns the dict produced by ``fn``.
        """
        if not self.path.parent.is_dir():
            # Deleted task/chat dir: nothing to lock (the lock file itself
            # couldn't be created) and nothing to write — same B7 guard as
            # write(): skip instead of resurrecting the dir.
            logger.warning(
                "state: refusing to update %s — parent dir is gone (deleted task/chat?)",
                self.path,
            )
            return fn(self.read())
        lock_path = self.path.with_name(self.path.name + ".lock")
        with open(lock_path, "a+b") as lock_file:
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
            try:
                data = fn(self.read())
                self.write(data)
            finally:
                fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)
        return data


# State files whose mtimes drive the long-poll wait endpoint (plan 4.2).
TASK_STATE_FILENAMES = (
    EXPLORE_FILENAME,
    PLAN_FILENAME,
    PLAN_REVIEW_FILENAME,
    IMPLEMENTATION_FILENAME,
    IMPL_REVIEW_FILENAME,
    FINISHED_FILENAME,
    TITLE_REGEN_FILENAME,
    SPLIT_FILENAME,
    INFO_FILENAME,
)


def task_state_mtimes(task_id: str, root: Path | None = None) -> float:
    """Max mtime across the task's known state JSON files (0.0 if none exist)."""
    from crack_server import paths  # lazy: paths imports this module

    directory = paths.task_dir(task_id, root)
    latest = 0.0
    for name in TASK_STATE_FILENAMES:
        try:
            latest = max(latest, directory.joinpath(name).stat().st_mtime)
        except OSError:
            continue
    return latest


def chat_state_mtime(chat_id: str, root: Path | None = None) -> float:
    """Mtime of the chat's state file (0.0 if missing)."""
    from crack_server import paths  # lazy: paths imports this module

    try:
        return paths.chat_state_path(chat_id, root).stat().st_mtime
    except OSError:
        return 0.0

