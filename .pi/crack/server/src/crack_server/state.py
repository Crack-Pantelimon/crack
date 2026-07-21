"""Generic JSON state-file store.

Every chat and sub-agent run persists state as one JSON dict per file
(``chat.json``, ``run.json``, …). This module centralizes the three operations
those files need:

- :meth:`JsonState.read` — tolerant read: ``{}`` on a missing or corrupt file.
- :meth:`JsonState.write` — atomic whole-file write (tmp + ``os.replace``).
- :meth:`JsonState.update` — read-modify-write under a per-path ``flock``.
"""

from __future__ import annotations

import fcntl
import json
import logging
import os
from pathlib import Path
from typing import Callable

logger = logging.getLogger("uvicorn.error")

INFO_FILENAME = "info.json"
CHAT_STATE_FILENAME = "chat.json"
RUN_STATE_FILENAME = "run.json"


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

        Skip (and log) if the parent dir is gone — a deleted chat must not be
        resurrected by a straggler worker write.
        """
        if not self.path.parent.is_dir():
            logger.warning(
                "state: refusing to write %s — parent dir is gone (deleted chat?)",
                self.path,
            )
            return
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        os.replace(tmp, self.path)

    def update(self, fn: Callable[[dict], dict]) -> dict:
        """Read-modify-write under an exclusive per-path flock."""
        if not self.path.parent.is_dir():
            logger.warning(
                "state: refusing to update %s — parent dir is gone (deleted chat?)",
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


def chat_state_mtime(chat_id: str, root: Path | None = None) -> float:
    """Max mtime of the chat state file and any sub-agent run.json files."""
    from crack_server import paths  # lazy: paths imports this module

    latest = 0.0
    try:
        latest = max(latest, paths.chat_state_path(chat_id, root).stat().st_mtime)
    except OSError:
        pass
    try:
        runs_dir = paths.chat_sub_agent_runs_dir(chat_id, root)
    except (ValueError, OSError):
        return latest
    if not runs_dir.is_dir():
        return latest
    for run_json in runs_dir.glob("*/run.json"):
        try:
            latest = max(latest, run_json.stat().st_mtime)
        except OSError:
            continue
    return latest
