"""Best-effort git commits at pipeline checkpoints.

Every failure is logged and swallowed: a checkpoint commit must never break the
pipeline (cross-process git races, detached HEAD, nothing-to-commit, no git repo,
etc. are all non-fatal). All commit messages are prefixed ``slopmaster3000:``.
"""

from __future__ import annotations

import logging
import subprocess
from pathlib import Path

from crack_server import paths

logger = logging.getLogger("uvicorn.error")

COMMIT_PREFIX = "slopmaster3000: "

# Identity passed inline so commits work even in a bare container with no
# global/repo git config (e.g. root user, no user.email set).
_IDENTITY = [
    "-c", "user.name=slopmaster3000",
    "-c", "user.email=slopmaster3000@crack.local",
]


def commit(add: str | Path | list[str | Path], message: str) -> None:
    """`git add <paths…>` then `git commit -m "slopmaster3000: <message>"`.

    ``add`` is a single path or a list of paths (relative to, or under, the
    project root). Any git failure is logged and swallowed."""
    root = paths.project_root()
    if isinstance(add, (str, Path)):
        add_paths = [str(add)]
    else:
        add_paths = [str(p) for p in add]

    try:
        subprocess.run(
            ["git", "-C", str(root), "add", "--", *add_paths],
            check=True,
            capture_output=True,
            text=True,
        )
        # Scope the commit to these paths so a checkpoint never sweeps in other
        # changes that happen to be staged in the shared working tree.
        result = subprocess.run(
            ["git", "-C", str(root), *_IDENTITY,
             "commit", "-m", f"{COMMIT_PREFIX}{message}", "--", *add_paths],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            # Most commonly "nothing to commit" — informational, not an error.
            logger.info(
                "git_utils: commit skipped (%r): %s",
                message,
                (result.stdout or result.stderr).strip()[:200],
            )
        else:
            logger.info("git_utils: committed %r", message)
    except Exception as e:  # noqa: BLE001 — checkpoint commits must never raise
        logger.error("git_utils: commit failed for %r: %s", message, e)
