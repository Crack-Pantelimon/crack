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

# Top-level patches touching these trees can brick crack-dev when applied to the
# host (uvicorn reloads the server package; the extension is re-read per pi run).
# Such patches are gated: tested in the sandbox first, then health-checked with a
# reverse-apply rollback watcher (Plan 7 Part B).
_SELF_MOD_PREFIXES = (".pi/crack/server/", ".pi/extensions/crack/")


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
    """Persist the sandbox's frozen tree id as ``base_tree``.

    Prefers the tree recorded at sandbox creation (no in-sandbox git round-trip).
    Falls back to ``git add -A`` + ``write-tree`` inside the sandbox when no
    frozen tree is on record (legacy / tests).
    """
    artifact_dir.mkdir(parents=True, exist_ok=True)
    # Derive conv id from sandbox name ``crack-sbx-<conv>``.
    conv_id = sandbox_name.removeprefix("crack-sbx-") if sandbox_name.startswith("crack-sbx-") else ""
    frozen = sandbox.frozen_tree_for(conv_id) if conv_id else None
    if frozen:
        base_tree_path(artifact_dir).write_text(frozen + "\n", encoding="utf-8")
        logger.info("patch: baseline %s (frozen) for %s", frozen[:12], artifact_dir.name)
        return frozen
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
    # Seed the index from the frozen base tree so `git add -A` computes a true
    # delta. Without this the sandbox's git repo was `git init`'d with an empty
    # index, so `git add -A` skips tracked-but-gitignored files (e.g. _data/**/*.bytes)
    # and every diff spuriously "deletes" them — which host `git apply` cannot apply.
    rc, _, err = await _git_in_sandbox(sandbox_name, "read-tree", base_tree)
    if rc != 0:
        raise RuntimeError(f"git read-tree {base_tree[:12]} failed: {err}")
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
    # Seed the index from the frozen base tree so `git add -A` computes a true
    # delta. Without this the sandbox's git repo was `git init`'d with an empty
    # index, so `git add -A` skips tracked-but-gitignored files (e.g. _data/**/*.bytes)
    # and every diff spuriously "deletes" them — which host `git apply` cannot apply.
    rc, _, err = _git_in_sandbox_sync(sandbox_name, "read-tree", base_tree)
    if rc != 0:
        raise RuntimeError(f"git read-tree {base_tree[:12]} failed: {err}")
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


# ---------------------------------------------------------------------------
# Plan 7 Part A: chain-overlay nesting via git-replay
# ---------------------------------------------------------------------------
#
# Rootless podman here rejects a multi-lower `--mount type=overlay`, and an
# explicit `:O` upperdir cannot itself sit on the host's overlay root. So a
# child cannot mount the parent's persisted upper as a lower directly. Instead
# the child sandbox starts as a plain `:O` overlay over the pristine host repo
# (like a top-level chat) and we *replay* the parent's uncommitted delta into it
# with `git apply`. The child then captures its own baseline, so its finish-time
# diff is exactly the child's own delta on top of the parent's tree — which the
# drain applies back to the parent overlay in dispatch order.


async def seed_child_from_parent(child_sandbox: str, run_id: str, state: dict) -> None:
    """Replay the parent's uncommitted delta into a fresh child sandbox so the
    child starts from the parent's current tree (Plan 7 Part A).

    Best-effort: any failure leaves the child on the pristine host tree (logged);
    the run still proceeds. Called once, before the child's baseline is captured.
    """
    parent_kind = state.get("parent_kind")
    chat_id = state.get("chat_id", "")
    parent_id = state.get("parent_id", "")
    parent_conv = parent_id if parent_kind == "run" else chat_id
    if parent_kind == "run":
        parent_dir = paths.run_dir(chat_id, parent_id)
    else:
        parent_dir = paths.chat_dir(chat_id)
    base_path = base_tree_path(parent_dir)
    if not base_path.is_file():
        return  # parent never captured a baseline (no sandbox / no edits yet)
    parent_base = base_path.read_text(encoding="utf-8").strip()
    parent_sandbox = sandbox.sandbox_name(parent_conv)
    rc, *_ = await sandbox._podman("container", "exists", parent_sandbox)
    if rc != 0:
        return  # parent sandbox gone (already finalized) — nothing to inherit

    # Compute the parent's delta vs its baseline using a throwaway index so we
    # never lock the parent's real .git/index — sibling seeds run concurrently.
    script = (
        'export GIT_INDEX_FILE="$(mktemp -u)"; '
        f"git -C {WORKSPACE} read-tree {parent_base} && "
        f"git -C {WORKSPACE} add -A && "
        f'git -C {WORKSPACE} diff --binary {parent_base} "$(git -C {WORKSPACE} write-tree)"; '
        'rc=$?; rm -f "$GIT_INDEX_FILE"; exit $rc'
    )
    rc, out, err = await sandbox._podman(
        "exec", parent_sandbox, "bash", "-c", script, timeout=_GIT_TIMEOUT,
    )
    if rc != 0:
        logger.warning("seed: parent delta failed for %s: %s", run_id, (err or out).strip())
        return
    if not out.strip():
        return  # parent has no uncommitted work to inherit
    seed_path = paths.run_dir(chat_id, run_id) / "parent_seed.diff"
    seed_path.write_text(out, encoding="utf-8")
    # Plain `git apply` (not --3way/--reject): the child's tree matches the seed's
    # base context exactly, so it applies cleanly with no stray .rej files that
    # could otherwise pollute the child's own baseline.
    arc, aout, aerr = await _git_in_sandbox(
        child_sandbox, "apply", str(seed_path.resolve()),
    )
    if arc == 0:
        logger.info("seed: replayed parent delta (%d bytes) into %s", len(out), run_id)
    else:
        logger.warning(
            "seed: apply parent delta into %s failed: %s", run_id, (aerr or aout).strip()
        )


def enqueue_chat_system_message(chat_id: str, message: str, *, source: str = "system") -> None:
    from crack_server import chats

    def _enqueue(state: dict) -> dict:
        pending = list(state.get("pending") or [])
        pending.append({"user": message, "source": source})
        state["pending"] = pending
        state["phase"] = "chatting"
        return state

    paths.chat_state(chat_id).update(_enqueue)
    queue.enqueue_exclusive(chat_id, chats.CHAT_JOB_SLUG, "run")


def enqueue_chat_patch_nag(chat_id: str, big_files: tuple[tuple[str, int], ...]) -> None:
    enqueue_chat_system_message(chat_id, format_big_file_nag(big_files), source="patch_guard")


def record_chat_apply_failure(chat_id: str, stderr: str, patch_path: Path) -> None:
    """Surface a host ``git apply`` failure as a durable, visible error and go idle.

    Deliberately does NOT enqueue a new agent turn — a host apply failure is
    environmental and must not restart the chat (that was the patch-apply loop).
    """
    resolved = str(patch_path.resolve())
    short = (stderr or "").strip()
    detail = short[-3000:]

    def _err(state: dict) -> dict:
        state["phase"] = "idle"
        state["error"] = "Your changes could not be applied to the host repo (git apply failed)."
        state["error_detail"] = f"{detail}\n\nThe full patch is at: {resolved}"
        return state

    paths.chat_state(chat_id).update(_err)
    logger.warning(
        "patch: host apply failed for chat %s; recorded error, not re-enqueuing", chat_id,
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
    if parent_kind == "chat":
        record_chat_apply_failure(chat_id, stderr, patch_path)
        return
    message = format_apply_failure(stderr, patch_path)
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


# ---------------------------------------------------------------------------
# Plan 7 Part B: self-modification apply guard
# ---------------------------------------------------------------------------


def patch_touches_self_mod(patch_path: Path) -> bool:
    """True when the patch changes crack-server or the crack extension — applying
    it to the host reloads/rebuilds the live harness, so it must be gated."""
    try:
        text = patch_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return False
    for line in text.splitlines():
        if not line.startswith("diff --git "):
            continue
        for token in line.split()[2:]:
            rel = token[2:] if token[:2] in ("a/", "b/") else token
            if rel.startswith(_SELF_MOD_PREFIXES):
                return True
    return False


def format_test_failure(output: str, patch_path: Path) -> str:
    resolved = patch_path.resolve()
    tail = output.strip()[-3000:] or "(no output)"
    return (
        "Your changes touch crack-server / the crack extension, so the harness ran "
        "the server test suite against your sandbox BEFORE applying to the live "
        "crack-dev host. The tests FAILED, so nothing was applied — the live server "
        "is untouched.\n\n"
        f"pytest output (tail):\n{tail}\n\n"
        f"The full patch is at: {resolved}\n\n"
        "Fix the failing tests, then stop; the harness will re-run this gate."
    )


def enqueue_chat_test_failure(chat_id: str, output: str, patch_path: Path) -> None:
    enqueue_chat_system_message(
        chat_id, format_test_failure(output, patch_path), source="patch_tests",
    )


async def run_sandbox_tests(sandbox_name: str) -> tuple[bool, str]:
    """Run the crack-server test suite inside the sandbox overlay (Plan 7B step 1).

    The sandbox inherits crack-dev's Poetry venv through the `:O` overlay on the
    target volume (``POETRY_VIRTUALENVS_PATH=/workspace/target/python-venvs``), so
    ``poetry run`` executes in it without installing. Returns
    ``(passed, combined_output)``.
    """
    script = (
        "cd /workspace/.pi/crack/server && "
        "PYTHONPATH=tests:. poetry run pytest -q "
        "--ignore=tests/test_vision_media.py"
    )
    rc, out, err = await sandbox._podman(
        "exec", sandbox_name, "bash", "-lc", script, timeout=600.0,
    )
    return rc == 0, (out + err)


def launch_health_watcher(chat_id: str, patch_path: Path) -> None:
    """Detached watcher: poll crack-dev health after a self-mod apply and, if it
    never comes healthy, reverse-apply the patch so the reloader recovers to a
    good tree (Plan 7B steps 2-4). Survives the server reload because it is a new
    session, independent of the worker/uvicorn process."""
    script = paths.project_root() / "_docker" / "_apply_healthcheck.sh"
    if not script.is_file():
        logger.warning("health watcher script missing: %s", script)
        return
    try:
        subprocess.Popen(  # noqa: S603 — fixed argv, no shell
            ["bash", str(script), str(patch_path.resolve()), chat_id],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
            cwd=str(paths.project_root()),
        )
        logger.info("launched apply health watcher for chat %s", chat_id)
    except OSError as e:
        logger.warning("could not launch health watcher for %s: %s", chat_id, e)


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
            return s

        chat.update(_bump)
        enqueue_chat_patch_nag(chat_id, result.big_files)
        return True

    if result.has_content and result.patch_path is not None:
        self_mod = patch_touches_self_mod(result.patch_path)
        gated = False
        if self_mod:
            passed, output = await run_sandbox_tests(sandbox_name)
            if not passed:
                # Do NOT apply a self-mod patch whose tests fail — the live
                # crack-dev host stays untouched; hand the failure to the chat.
                enqueue_chat_test_failure(chat_id, output, result.patch_path)
                gated = True
        if not gated:
            ok, err = await apply_patch_on_host(result.patch_path)
            if not ok:
                record_chat_apply_failure(chat_id, err, result.patch_path)
            elif self_mod:
                # Applied code reloads crack-dev; watch health, roll back on failure.
                launch_health_watcher(chat_id, result.patch_path)

    def _reset(s: dict) -> dict:
        s["patch_guard_attempts"] = 0
        return s

    chat.update(_reset)
    base_tree_path(artifact_dir).unlink(missing_ok=True)
    await sandbox.destroy_sandbox(chat_id)
    return False


def extract_run_patch(
    run_id: str,
    *,
    forceful: bool = False,
    mark_pending: bool = True,
) -> ExtractResult | None:
    """Extract a sub-agent's delta into ``patch.diff`` and tear down its sandbox.

    Does NOT apply to the parent — that is deferred to :func:`drain_parent_patches`
    so sibling children can't race concurrent ``git apply``s into the parent
    overlay, and so patches land in dispatch order (Plan 7 / parallel-patch guard).
    When ``mark_pending`` and the patch has content, flags the run ``patch_pending``
    for the drain to pick up. A size-guard nag short-circuits (no teardown).
    """
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

    if mark_pending and result.has_content:
        paths.run_state_by_id(run_id).update(lambda s: {**s, "patch_pending": True})

    base_tree_path(artifact_dir).unlink(missing_ok=True)
    sandbox.destroy_sandbox_sync(run_id)
    return result


def _dispatch_key(run_id: str) -> tuple[int, str]:
    """Sort key = spawn order. Run ids are ``<ms-epoch>_<hex>``; sort by the numeric
    epoch first so a stray width change can't reorder (lexicographic would)."""
    head = run_id.split("_", 1)[0]
    return (int(head) if head.isdigit() else 0, run_id)


def _pending_children_in_order(
    chat_id: str, parent_kind: str, parent_id: str
) -> list[tuple[str, Path]]:
    """``(run_id, patch_path)`` for a parent's finished children whose patch is
    still pending, oldest-spawned (dispatch order) first."""
    out: list[tuple[str, Path]] = []
    for run_id in sorted(paths.list_run_ids(chat_id), key=_dispatch_key):
        st = paths.run_state(chat_id, run_id).read()
        if st.get("parent_kind") != parent_kind:
            continue
        if parent_kind == "chat" and st.get("parent_id") != chat_id:
            continue
        if parent_kind == "run" and st.get("parent_id") != parent_id:
            continue
        if not st.get("patch_pending"):
            continue
        out.append((run_id, patch_diff_path(paths.run_dir(chat_id, run_id))))
    return out


def _clear_pending(run_id: str) -> None:
    paths.run_state_by_id(run_id).update(lambda s: {**s, "patch_pending": False})


def drain_parent_patches(chat_id: str, parent_kind: str, parent_id: str) -> None:
    """Apply every pending child patch into the parent overlay, in dispatch order,
    but only once the parent has zero running children — and serialized so two
    finishing siblings never ``git apply`` into the parent sandbox concurrently.

    A child that fails to apply hands the conflict to the managing agent (same as
    Plan 4) and is still cleared, so a later sibling isn't blocked behind it.
    """
    if not sandbox.sandbox_enabled():
        return
    from crack_server.sub_agents import runner

    parent_conv = parent_id if parent_kind == "run" else chat_id
    parent_state = (
        paths.chat_state(chat_id) if parent_kind == "chat"
        else paths.run_state_by_id(parent_id)
    )
    while True:
        if runner.active_child_count(chat_id, parent_kind, parent_id) > 0:
            return  # siblings still running; whoever finishes last drains
        claimed = {"v": False}

        def _claim(s: dict) -> dict:
            if s.get("patch_draining"):
                return s
            claimed["v"] = True
            s["patch_draining"] = True
            return s

        parent_state.update(_claim)
        if not claimed["v"]:
            return  # another finisher holds the drain; it will pick up our patch
        progressed = False
        try:
            pending = _pending_children_in_order(chat_id, parent_kind, parent_id)
            if not pending:
                return
            parent_sandbox = sandbox.sandbox_name(parent_conv)
            for child_id, patch_path in pending:
                if not patch_path.is_file():
                    _clear_pending(child_id)
                    progressed = True
                    continue
                try:
                    ok, err = apply_patch_to_sandbox_sync(parent_sandbox, patch_path)
                except Exception:
                    # A raised apply (e.g. podman timeout) must not strand the whole
                    # drain or silently drop the patch: leave patch_pending set so a
                    # later drain retries it, and move on to the next child.
                    logger.exception(
                        "drain: apply raised for %s; leaving pending for retry", child_id,
                    )
                    continue
                # Clear only after a definitive apply attempt (success or a conflict
                # handed to the managing agent) — never before, so an exception can't
                # lose the patch.
                _clear_pending(child_id)
                progressed = True
                if not ok:
                    notify_parent_apply_failure(
                        parent_kind, parent_id, chat_id, err, patch_path,
                    )
        finally:
            parent_state.update(lambda s: {**s, "patch_draining": False})
        # Re-loop only if we cleared at least one patch this pass — a sibling may
        # have finished (set patch_pending) during our apply. If a pass made NO
        # progress (every apply raised), stop rather than spin; the still-pending
        # patches are retried by the next child's finish()-drain.
        if not progressed:
            return
