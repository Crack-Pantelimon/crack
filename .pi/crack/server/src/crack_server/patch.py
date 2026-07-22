"""Baseline-diff patch extraction, size guard, and auto-apply (Plan 4).

Each sandboxed conversation snapshots ``git write-tree`` at session start and
diffs against it at end so the patch captures only that agent's delta (not
pre-existing host dirt). Patches auto-apply to the parent overlay (sub-agents)
or the crack-dev host tree (top-level chats).
"""

from __future__ import annotations

import asyncio
import logging
import subprocess
from dataclasses import dataclass
from pathlib import Path

from crack_server import paths, queue, sandbox

logger = logging.getLogger("uvicorn.error")

# 95.0 MB in decimal (10^6 bytes), per plans-23 spec.
MAX_FILE_BYTES = 95 * 1_000_000
MAX_GUARD_ATTEMPTS = 5
WORKSPACE = "/workspace"
_GIT_TIMEOUT = 300.0


@dataclass(frozen=True)
class ExtractResult:
    patch_path: Path | None
    empty: bool
    needs_nag: bool
    big_files: tuple[tuple[str, int], ...]
    nag_attempt: int

    @property
    def has_content(self) -> bool:
        return not self.empty and self.patch_path is not None


def base_tree_path(artifact_dir: Path) -> Path:
    return artifact_dir / "base_tree"


def patch_diff_path(artifact_dir: Path) -> Path:
    return artifact_dir / "patch.diff"


def format_big_file_nag(big_files: tuple[tuple[str, int], ...]) -> str:
    lines = [
        "The harness detected file(s) larger than 95 MB staged for the patch. "
        "They cannot be included. Please add them to `.gitignore` or delete them, "
        "then stop.",
        "",
    ]
    for path, size in big_files:
        lines.append(f"- {path} ({size} bytes)")
    return "\n".join(lines)


def format_apply_failure(stderr: str, patch_path: Path) -> str:
    resolved = patch_path.resolve()
    return (
      "Patch application failed.\n\n"
      f"git apply stderr:\n{stderr.strip() or '(empty)'}\n\n"
      f"The full patch is at: {resolved}\n\n"
      "Resolve the conflict directly in the working tree, finish applying the "
      f"patch, then continue your task. The full patch is at {resolved} for reference."
  )


def _git_host(*args: str, timeout: float = _GIT_TIMEOUT) -> tuple[int, str, str]:
    cmd = ("git", "-C", WORKSPACE, *args)
    logger.debug("host git %s", " ".join(args))
    try:
        proc = subprocess.run(cmd, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        raise RuntimeError(f"git timed out: {' '.join(args)}") from None
    out = (proc.stdout or b"").decode("utf-8", "replace")
    err = (proc.stderr or b"").decode("utf-8", "replace")
    return proc.returncode if proc.returncode is not None else -1, out, err


async def _git_in_sandbox(
    sandbox_name: str, *args: str, timeout: float = _GIT_TIMEOUT,
) -> tuple[int, str, str]:
    return await sandbox._podman(
        "exec", sandbox_name, "git", "-C", WORKSPACE, *args, timeout=timeout,
    )


def _git_in_sandbox_sync(
    sandbox_name: str, *args: str, timeout: float = _GIT_TIMEOUT,
) -> tuple[int, str, str]:
    return sandbox._podman_sync(
        "exec", sandbox_name, "git", "-C", WORKSPACE, *args, timeout=timeout,
    )


async def _staged_file_sizes(sandbox_name: str) -> list[tuple[str, int]]:
    """Return ``(repo-relative path, byte size)`` for each staged file."""
    script = (
        "git -C /workspace add -A && "
        "git -C /workspace diff --cached --name-only -z | "
        "while IFS= read -r -d '' f; do "
        'if [ -f "$f" ]; then printf "%s\\t%s\\n" "$f" "$(wc -c < "$f" | tr -d " \\n")"; fi; '
        "done"
    )
    rc, out, err = await sandbox._podman(
        "exec", sandbox_name, "bash", "-exc", script, timeout=_GIT_TIMEOUT,
    )
    if rc != 0:
        raise RuntimeError(f"staged file size listing failed: {err or out}")
    sizes: list[tuple[str, int]] = []
    for line in out.splitlines():
        if not line.strip() or "\t" not in line:
            continue
        rel, raw = line.split("\t", 1)
        try:
            sizes.append((rel, int(raw)))
        except ValueError:
            continue
    return sizes


async def _write_tree(sandbox_name: str) -> str:
    rc, out, err = await _git_in_sandbox(sandbox_name, "write-tree")
    if rc != 0:
        raise RuntimeError(f"git write-tree failed: {err or out}")
    tree = out.strip()
    if not tree:
        raise RuntimeError("git write-tree returned empty tree id")
    return tree


def _write_tree_sync(sandbox_name: str) -> str:
    rc, out, err = _git_in_sandbox_sync(sandbox_name, "write-tree")
    if rc != 0:
        raise RuntimeError(f"git write-tree failed: {err or out}")
    tree = out.strip()
    if not tree:
        raise RuntimeError("git write-tree returned empty tree id")
    return tree


async def capture_baseline(sandbox_name: str, artifact_dir: Path) -> str:
    """``git add -A`` + ``write-tree``; persist tree id to ``base_tree``."""
    artifact_dir.mkdir(parents=True, exist_ok=True)
    rc, _, err = await _git_in_sandbox(sandbox_name, "add", "-A")
    if rc != 0:
        raise RuntimeError(f"git add -A failed at baseline: {err}")
    tree = await _write_tree(sandbox_name)
    base_tree_path(artifact_dir).write_text(tree + "\n", encoding="utf-8")
    logger.info("patch: baseline %s for %s", tree[:12], artifact_dir.name)
    return tree


async def ensure_baseline(sandbox_name: str, artifact_dir: Path) -> str:
    """Capture baseline only when ``base_tree`` is missing (one run session)."""
    path = base_tree_path(artifact_dir)
    if path.is_file():
        return path.read_text(encoding="utf-8").strip()
    return await capture_baseline(sandbox_name, artifact_dir)


def capture_baseline_sync(sandbox_name: str, artifact_dir: Path) -> str:
    return asyncio.run(capture_baseline(sandbox_name, artifact_dir))


def ensure_baseline_sync(sandbox_name: str, artifact_dir: Path) -> str:
    path = base_tree_path(artifact_dir)
    if path.is_file():
        return path.read_text(encoding="utf-8").strip()
    return capture_baseline_sync(sandbox_name, artifact_dir)


async def _stage_for_patch(
    sandbox_name: str,
    *,
    exclude: tuple[str, ...] = (),
) -> None:
    rc, _, err = await _git_in_sandbox(sandbox_name, "add", "-A")
    if rc != 0:
        raise RuntimeError(f"git add -A failed: {err}")
    if exclude:
        rc, _, err = await _git_in_sandbox(sandbox_name, "reset", "--", *exclude)
        if rc != 0:
            raise RuntimeError(f"git reset failed: {err}")


def _stage_for_patch_sync(
    sandbox_name: str,
    *,
    exclude: tuple[str, ...] = (),
) -> None:
    rc, _, err = _git_in_sandbox_sync(sandbox_name, "add", "-A")
    if rc != 0:
        raise RuntimeError(f"git add -A failed: {err}")
    if exclude:
        rc, _, err = _git_in_sandbox_sync(sandbox_name, "reset", "--", *exclude)
        if rc != 0:
            raise RuntimeError(f"git reset failed: {err}")


async def _produce_diff(
    sandbox_name: str,
    base_tree: str,
    patch_path: Path,
    *,
    exclude: tuple[str, ...] = (),
) -> bool:
    await _stage_for_patch(sandbox_name, exclude=exclude)
    end_tree = await _write_tree(sandbox_name)
    patch_path.parent.mkdir(parents=True, exist_ok=True)
    rc, out, err = await _git_in_sandbox(
        sandbox_name, "diff", base_tree, end_tree,
    )
    if rc != 0:
        raise RuntimeError(f"git diff failed: {err or out}")
    patch_path.write_text(out, encoding="utf-8")
    return bool(out.strip())


def _produce_diff_sync(
    sandbox_name: str,
    base_tree: str,
    patch_path: Path,
    *,
    exclude: tuple[str, ...] = (),
) -> bool:
    _stage_for_patch_sync(sandbox_name, exclude=exclude)
    end_tree = _write_tree_sync(sandbox_name)
    patch_path.parent.mkdir(parents=True, exist_ok=True)
    rc, out, err = _git_in_sandbox_sync(sandbox_name, "diff", base_tree, end_tree)
    if rc != 0:
        raise RuntimeError(f"git diff failed: {err or out}")
    patch_path.write_text(out, encoding="utf-8")
    return bool(out.strip())


def _oversized(files: list[tuple[str, int]]) -> tuple[list[str], tuple[tuple[str, int], ...]]:
    """Return repo-relative big paths and display tuples with full paths."""
    rel = [p for p, sz in files if sz > MAX_FILE_BYTES]
    display = tuple(
        (f"/workspace/{p}" if not p.startswith("/") else p, sz)
        for p, sz in files
        if sz > MAX_FILE_BYTES
    )
    return rel, display


async def extract_patch(
    sandbox_name: str,
    artifact_dir: Path,
    *,
    forceful: bool = False,
    nag_attempt: int = 0,
) -> ExtractResult:
    """Extract this session's delta into ``patch.diff``."""
    base_path = base_tree_path(artifact_dir)
    if not base_path.is_file():
        await capture_baseline(sandbox_name, artifact_dir)
    base_tree = base_path.read_text(encoding="utf-8").strip()
    patch_path = patch_diff_path(artifact_dir)

    if forceful:
        sizes = await _staged_file_sizes(sandbox_name)
        exclude_rel, big_display = _oversized(sizes)
        has = await _produce_diff(
            sandbox_name, base_tree, patch_path, exclude=tuple(exclude_rel),
        )
        return ExtractResult(
            patch_path=patch_path if has else None,
            empty=not has,
            needs_nag=False,
            big_files=big_display,
            nag_attempt=nag_attempt,
        )

    sizes = await _staged_file_sizes(sandbox_name)
    exclude_rel, big_display = _oversized(sizes)
    if big_display:
        rc, _, err = await _git_in_sandbox(sandbox_name, "reset")
        if rc != 0:
            raise RuntimeError(f"git reset failed: {err}")
        if nag_attempt < MAX_GUARD_ATTEMPTS - 1:
            return ExtractResult(
                patch_path=None,
                empty=True,
                needs_nag=True,
                big_files=big_display,
                nag_attempt=nag_attempt + 1,
            )
        has = await _produce_diff(
            sandbox_name, base_tree, patch_path, exclude=tuple(exclude_rel),
        )
        return ExtractResult(
            patch_path=patch_path if has else None,
            empty=not has,
            needs_nag=False,
            big_files=big_display,
            nag_attempt=nag_attempt + 1,
        )

    has = await _produce_diff(sandbox_name, base_tree, patch_path)
    return ExtractResult(
        patch_path=patch_path if has else None,
        empty=not has,
        needs_nag=False,
        big_files=(),
        nag_attempt=nag_attempt,
    )


def extract_patch_sync(
    sandbox_name: str,
    artifact_dir: Path,
    *,
    forceful: bool = False,
    nag_attempt: int = 0,
) -> ExtractResult:
    return asyncio.run(
        extract_patch(
            sandbox_name, artifact_dir, forceful=forceful, nag_attempt=nag_attempt,
        )
    )


async def _apply_git(
    target_sandbox: str | None, patch_path: Path,
) -> tuple[bool, str]:
    """Apply ``patch_path``. ``target_sandbox=None`` applies on crack-dev host."""
    resolved = str(patch_path.resolve())
    for extra in (["--3way"], ["--reject"]):
        if target_sandbox is None:
            rc, out, err = _git_host("apply", *extra, resolved)
        else:
            rc, out, err = await sandbox._podman(
                "exec", target_sandbox,
                "git", "-C", WORKSPACE, "apply", *extra, resolved,
                timeout=_GIT_TIMEOUT,
            )
        if rc == 0:
            return True, ""
        combined = (err or out).strip()
        logger.warning(
            "patch apply %s failed (rc=%d): %s", extra, rc, combined,
        )
        last_err = combined
    return False, last_err or "git apply failed"


def apply_patch_to_host(patch_path: Path) -> tuple[bool, str]:
    return asyncio.run(_apply_git(None, patch_path))


async def apply_patch_on_host(patch_path: Path) -> tuple[bool, str]:
    return await _apply_git(None, patch_path)


async def apply_patch_to_sandbox(sandbox_name: str, patch_path: Path) -> tuple[bool, str]:
    return await _apply_git(sandbox_name, patch_path)


def apply_patch_to_sandbox_sync(sandbox_name: str, patch_path: Path) -> tuple[bool, str]:
    return asyncio.run(apply_patch_to_sandbox(sandbox_name, patch_path))


def enqueue_chat_system_message(chat_id: str, message: str, *, source: str = "system") -> None:
    from crack_server import chats

    def _enqueue(state: dict) -> dict:
        pending = list(state.get("pending") or [])
        pending.append({"user": message, "source": source})
        state["pending"] = pending
        state["phase"] = "chatting"
        state["stop_requested"] = False
        return state

    paths.chat_state(chat_id).update(_enqueue)
    queue.enqueue_exclusive(chat_id, chats.CHAT_JOB_SLUG, "run")


def enqueue_chat_patch_nag(chat_id: str, big_files: tuple[tuple[str, int], ...]) -> None:
    enqueue_chat_system_message(chat_id, format_big_file_nag(big_files), source="patch_guard")


def enqueue_chat_apply_failure(chat_id: str, stderr: str, patch_path: Path) -> None:
    enqueue_chat_system_message(
        chat_id, format_apply_failure(stderr, patch_path), source="patch_apply",
    )


def enqueue_subagent_patch_nag(run_id: str, big_files: tuple[tuple[str, int], ...]) -> None:
    from crack_server.sub_agents import registry

    state = paths.run_state_by_id(run_id).read()
    persona = registry.get(state.get("persona", ""))
    if persona is None:
        logger.warning("patch nag: unknown persona for run %s", run_id)
        return
    persona.enqueue_step(
        run_id,
        "run",
        {
            "run_id": run_id,
            "started_token": state.get("started_token"),
            "patch_nag": format_big_file_nag(big_files),
        },
    )


def notify_parent_apply_failure(
    parent_kind: str,
    parent_id: str,
    chat_id: str,
    stderr: str,
    patch_path: Path,
) -> None:
    message = format_apply_failure(stderr, patch_path)
    if parent_kind == "chat":
        enqueue_chat_system_message(chat_id, message, source="patch_apply")
        return
    if parent_kind == "run":
        from crack_server.sub_agents import registry

        parent_state = paths.run_state_by_id(parent_id).read()
        persona = registry.get(parent_state.get("persona", ""))
        if persona is None:
            logger.warning("patch apply failure: unknown parent persona for %s", parent_id)
            return
        persona.enqueue_step(
            parent_id,
            "run",
            {
                "run_id": parent_id,
                "started_token": parent_state.get("started_token"),
                "patch_conflict": message,
            },
        )


async def finalize_chat_sandbox(
    chat_id: str,
    sandbox_name: str,
    *,
    forceful: bool = False,
) -> bool:
    """Extract/apply/destroy for a top-level chat. Returns True if nag re-queued work."""
    artifact_dir = paths.chat_dir(chat_id)
    chat = paths.chat_state(chat_id)
    nag_attempt = int(chat.read().get("patch_guard_attempts", 0))
    result = await extract_patch(
        sandbox_name, artifact_dir, forceful=forceful, nag_attempt=nag_attempt,
    )
    if result.needs_nag and nag_attempt < MAX_GUARD_ATTEMPTS:
        def _bump(s: dict) -> dict:
            s["patch_guard_attempts"] = nag_attempt + 1
            s["phase"] = "chatting"
            s["stop_requested"] = False
            return s

        chat.update(_bump)
        enqueue_chat_patch_nag(chat_id, result.big_files)
        return True

    if result.has_content and result.patch_path is not None:
        ok, err = await apply_patch_on_host(result.patch_path)
        if not ok:
            enqueue_chat_apply_failure(chat_id, err, result.patch_path)

    def _reset(s: dict) -> dict:
        s["patch_guard_attempts"] = 0
        return s

    chat.update(_reset)
    base_tree_path(artifact_dir).unlink(missing_ok=True)
    await sandbox.destroy_sandbox(chat_id)
    return False


def finalize_run_sandbox(
    run_id: str,
    *,
    forceful: bool = False,
    apply_to_parent: bool = True,
) -> ExtractResult | None:
    """Extract patch for a sub-agent run; optionally apply to parent and destroy."""
    if not sandbox.sandbox_enabled():
        return None
    state = paths.run_state_by_id(run_id).read()
    chat_id = state.get("chat_id", "")
    artifact_dir = paths.run_dir(chat_id, run_id)
    sandbox_name = sandbox.sandbox_name(run_id)
    nag_attempt = int(state.get("patch_guard_attempts", 0))
    result = extract_patch_sync(
        sandbox_name, artifact_dir, forceful=forceful, nag_attempt=nag_attempt,
    )
    if result.needs_nag and nag_attempt < MAX_GUARD_ATTEMPTS:
        paths.run_state_by_id(run_id).update(
            lambda s: {
                **s,
                "patch_guard_attempts": nag_attempt + 1,
                "phase": "running",
            }
        )
        enqueue_subagent_patch_nag(run_id, result.big_files)
        return result

    if apply_to_parent and result.has_content and result.patch_path is not None:
        parent_kind = state.get("parent_kind")
        parent_id = state.get("parent_id")
        parent_conv = parent_id if parent_kind == "run" else chat_id
        ok, err = apply_patch_to_sandbox_sync(
            sandbox.sandbox_name(parent_conv), result.patch_path,
        )
        if not ok:
            notify_parent_apply_failure(
                parent_kind, parent_id, chat_id, err, result.patch_path,
            )

    base_tree_path(artifact_dir).unlink(missing_ok=True)
    sandbox.destroy_sandbox_sync(run_id)
    return result
