"""Best-effort git commits at pipeline checkpoints, plus host-tree helpers
for the clean-git gate / frozen sandbox base.

Every failure in :func:`commit` is logged and swallowed: a checkpoint commit
must never break the pipeline. All commit messages are prefixed
``slopmaster3000:``.
"""

from __future__ import annotations

import logging
import subprocess
from html import escape
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


def _host_repo_root() -> Path:
    """In-container path to the repo tree for the clean-git gate.

    The gate helpers run *inside* the crack-dev container. ``CRACK_HOST_REPO_ROOT``
    is a **host-only** path (e.g. ``/home/p/VIDOEGAME/crack``) that exists only on
    the docker host — it is meant for constructing sibling-container overlay mount
    specs, and does not resolve inside the container (``git -C`` there fails with
    ``cannot change to '…': No such file or directory``). The identical tree is
    bind-mounted at ``/workspace`` (== :func:`paths.project_root`), so git status
    there reflects the true host-tree dirtiness. Always use the in-container path.
    """
    return paths.project_root()


def host_worktree_dirty(root: Path | None = None) -> bool:
    """True when the host worktree has staged, unstaged, or untracked changes."""
    repo = root or _host_repo_root()
    try:
        result = subprocess.run(
            ["git", "-C", str(repo), "status", "--porcelain"],
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        logger.error("git_utils: host_worktree_dirty failed: %s", e)
        return True  # fail closed: refuse to fork a dirty/unknown tree
    if result.returncode != 0:
        logger.error(
            "git_utils: git status failed: %s",
            (result.stderr or result.stdout).strip()[:200],
        )
        return True
    return bool(result.stdout.strip())


def host_status_colored(limit: int = 10, root: Path | None = None) -> str:
    """First ``limit`` lines of colourised ``git status`` for the host tree."""
    repo = root or _host_repo_root()
    try:
        result = subprocess.run(
            ["git", "-C", str(repo), "-c", "color.status=always", "status"],
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        return f"(git status failed: {e})"
    text = result.stdout or result.stderr or ""
    lines = text.splitlines()
    return "\n".join(lines[:limit])


def ansi_to_html(text: str) -> str:
    """Minimal SGR→HTML span converter (dependency-free). Escapes HTML first."""
    out: list[str] = []
    i = 0
    open_span = False
    s = text
    while i < len(s):
        if s[i] == "\x1b" and i + 1 < len(s) and s[i + 1] == "[":
            j = i + 2
            while j < len(s) and s[j] != "m":
                j += 1
            codes = s[i + 2:j].split(";") if j < len(s) else []
            i = j + 1 if j < len(s) else len(s)
            if open_span:
                out.append("</span>")
                open_span = False
            if not codes or codes == ["0"] or codes == [""]:
                continue
            style_parts: list[str] = []
            for c in codes:
                if c == "1":
                    style_parts.append("font-weight:bold")
                elif c == "31":
                    style_parts.append("color:#c22")
                elif c == "32":
                    style_parts.append("color:#2a2")
                elif c == "33":
                    style_parts.append("color:#a80")
                elif c == "34":
                    style_parts.append("color:#26c")
                elif c == "35":
                    style_parts.append("color:#a2a")
                elif c == "36":
                    style_parts.append("color:#2aa")
                elif c == "91":
                    style_parts.append("color:#f55")
                elif c == "92":
                    style_parts.append("color:#5c5")
                elif c == "93":
                    style_parts.append("color:#da0")
            if style_parts:
                out.append(f'<span style="{";".join(style_parts)}">')
                open_span = True
            continue
        j = i
        while j < len(s) and s[j] != "\x1b":
            j += 1
        out.append(escape(s[i:j]))
        i = j
    if open_span:
        out.append("</span>")
    return "".join(out)


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
