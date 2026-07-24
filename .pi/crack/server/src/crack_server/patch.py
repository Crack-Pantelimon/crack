"""Baseline-diff patch extraction, size guard, and merge-based integration.

Each sandboxed conversation snapshots ``git write-tree`` at session start and
diffs against it at end so the patch captures only that agent's delta (not
pre-existing host dirt). Top-level chats publish a *pending* patch for human
review (3-way ``merge-tree`` on Commit); sub-agent patches auto-merge into the
parent overlay.
"""

from __future__ import annotations

import asyncio
import fcntl
import json
import logging
import subprocess
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator

from crack_server import paths, queue, sandbox

logger = logging.getLogger("uvicorn.error")

# 95.0 MB in decimal (10^6 bytes), per plans-23 spec.
MAX_FILE_BYTES = 95 * 1_000_000
MAX_GUARD_ATTEMPTS = 5
WORKSPACE = "/workspace"
_GIT_TIMEOUT = 300.0

# Retry ladder for Commit-time real conflicts (D7). Reject-with-comments does
# not consume MERGE_AGENT_ATTEMPTS — only auto-bounces do.
MERGE_AUTO_ATTEMPTS = 1
MERGE_AGENT_ATTEMPTS = 1

# Top-level patches touching these trees can brick crack-dev when applied to the
# host (uvicorn reloads the server package; the extension is re-read per pi run).
# Such patches are gated: tested in the sandbox first, then health-checked with a
# reverse-apply rollback watcher (Plan 7 Part B).
_SELF_MOD_PREFIXES = (".pi/crack/server/", ".pi/extensions/crack/")

_INCOMING_REF = "refs/crack/incoming"
_COMMIT_IDENTITY = [
    "-c", "user.name=slopmaster3000",
    "-c", "user.email=slopmaster3000@crack.local",
]


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


@dataclass(frozen=True)
class MergeResult:
    ok: bool
    conflicted_paths: tuple[str, ...] = ()
    changed_paths: tuple[str, ...] = ()
    err: str = ""
    used_fallback: bool = False
    merged_tree: str = ""
    dest_tree: str = ""
    messages: str = ""


# ---------------------------------------------------------------------------
# Trajectory traces: surface patch build/apply as UI-only notes so the user can
# see a diff being constructed and merged (sub-agent → parent, or chat → host).
# ---------------------------------------------------------------------------


def _human_bytes(n: int) -> str:
    if n < 1000:
        return f"{n} B"
    if n < 1_000_000:
        return f"{n / 1000:.2f} KB"
    return f"{n / 1_000_000:.2f} MB"


def diff_stats(patch_text: str) -> dict:
    """Count files changed / added / deleted and total byte size of a git diff."""
    files = added = deleted = 0
    for line in patch_text.splitlines():
        if line.startswith("diff --git "):
            files += 1
        elif line.startswith("new file mode "):
            added += 1
        elif line.startswith("deleted file mode "):
            deleted += 1
    return {
        "files": files,
        "added": added,
        "deleted": deleted,
        "modified": max(0, files - added - deleted),
        "bytes": len(patch_text.encode("utf-8")),
    }


def _stats_from_path(patch_path: Path) -> dict | None:
    try:
        return diff_stats(patch_path.read_text(encoding="utf-8", errors="replace"))
    except OSError:
        return None


def format_patch_summary(stats: dict) -> str:
    return (
        f"{stats['files']} files changed, {stats['added']} added, "
        f"{stats['deleted']} deleted · {_human_bytes(stats['bytes'])}"
    )


def _note_parent(
    parent_kind: str,
    parent_id: str,
    chat_id: str,
    note_type: str,
    text: str,
    **kw: str,
) -> None:
    """Append a trajectory note to a patch's *destination* conversation (the
    parent overlay for sub-agent patches, the chat for host patches)."""
    try:
        obj = (
            paths.chat_state(chat_id)
            if parent_kind == "chat"
            else paths.run_state_by_id(parent_id)
        )
        paths.append_traj_note(obj, note_type, text, **kw)
    except (ValueError, FileNotFoundError):
        pass
    except Exception:  # a UI marker must never break patch apply
        logger.exception("patch: failed to record %s note", note_type)


def base_tree_path(artifact_dir: Path) -> Path:
    return artifact_dir / "base_tree"


def patch_diff_path(artifact_dir: Path) -> Path:
    return artifact_dir / "patch.diff"


def delta_bundle_path(artifact_dir: Path) -> Path:
    return artifact_dir / "delta.bundle"


def delta_json_path(artifact_dir: Path) -> Path:
    return artifact_dir / "delta.json"


def _conv_id_from_sandbox(sandbox_name: str) -> str:
    return (
        sandbox_name.removeprefix("crack-sbx-")
        if sandbox_name.startswith("crack-sbx-")
        else ""
    )


@contextmanager
def host_apply_lock() -> Iterator[None]:
    """Cross-process flock for host merge/commit (worker jobs run concurrently)."""
    lock_dir = paths.harness_dir() / "locks"
    lock_dir.mkdir(parents=True, exist_ok=True)
    lock_path = lock_dir / "host-apply.lock"
    with open(lock_path, "a+", encoding="utf-8") as fh:
        fcntl.flock(fh.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(fh.fileno(), fcntl.LOCK_UN)


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


async def _produce_diff(
    sandbox_name: str,
    base_tree: str,
    patch_path: Path,
    *,
    exclude: tuple[str, ...] = (),
    base_commit: str | None = None,
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
    # `--binary` emits the full `index <sha>..<sha>` line + literal binary hunk so
    # host `git apply` can reconstruct binary blobs (e.g. a screenshot PNG). Without
    # it git writes only "Binary files ... differ", which apply rejects with
    # "cannot apply binary patch ... without full index line".
    rc, out, err = await _git_in_sandbox(
        sandbox_name, "diff", "--binary", base_tree, end_tree,
    )
    if rc != 0:
        raise RuntimeError(f"git diff failed: {err or out}")
    patch_path.write_text(out, encoding="utf-8")
    has = bool(out.strip())
    if has:
        await _write_delta_bundle(
            sandbox_name,
            patch_path.parent,
            base_tree=base_tree,
            end_tree=end_tree,
            base_commit=base_commit,
        )
    return has


async def _write_delta_bundle(
    sandbox_name: str,
    artifact_dir: Path,
    *,
    base_tree: str,
    end_tree: str,
    base_commit: str | None,
) -> None:
    """Emit ``delta.bundle`` + ``delta.json`` for merge-tree consumers.

    Skips (logged) when ``base_commit`` is missing — old sandboxes / tests fall
    back to textual ``git apply`` on ``patch.diff``.
    """
    bundle = delta_bundle_path(artifact_dir)
    meta = delta_json_path(artifact_dir)
    for p in (bundle, meta):
        p.unlink(missing_ok=True)
    if not base_commit:
        logger.info(
            "patch: no frozen HEAD for %s — skipping delta.bundle (apply fallback)",
            artifact_dir.name,
        )
        return
    # commit-tree needs author/committer env; identity flags alone are enough when
    # passed before the subcommand via `git -c … commit-tree`.
    rc, end_commit, err = await _git_in_sandbox(
        sandbox_name,
        *_COMMIT_IDENTITY,
        "commit-tree", end_tree, "-p", base_commit, "-m", "crack-delta",
    )
    end_commit = end_commit.strip()
    if rc != 0 or not end_commit:
        logger.warning(
            "patch: commit-tree failed for %s: %s — skipping bundle",
            artifact_dir.name, (err or end_commit).strip(),
        )
        return
    bundle_abs = str(bundle.resolve())
    # git bundle create rejects bare-SHA ranges as "empty" on 2.47; named refs work.
    rc, _, err = await _git_in_sandbox(
        sandbox_name, "update-ref", "refs/crack/delta-base", base_commit,
    )
    if rc != 0:
        logger.warning("patch: delta-base ref failed: %s", err)
        return
    rc, _, err = await _git_in_sandbox(
        sandbox_name, "update-ref", "refs/crack/delta-end", end_commit,
    )
    if rc != 0:
        logger.warning("patch: delta-end ref failed: %s", err)
        return
    try:
        rc, _, err = await _git_in_sandbox(
            sandbox_name,
            "bundle", "create", bundle_abs,
            "refs/crack/delta-base..refs/crack/delta-end",
        )
    finally:
        await _git_in_sandbox(
            sandbox_name, "update-ref", "-d", "refs/crack/delta-base",
        )
        await _git_in_sandbox(
            sandbox_name, "update-ref", "-d", "refs/crack/delta-end",
        )
    if rc != 0 or not bundle.is_file():
        logger.warning(
            "patch: bundle create failed for %s: %s",
            artifact_dir.name, (err or "").strip(),
        )
        bundle.unlink(missing_ok=True)
        return
    meta.write_text(
        json.dumps({
            "base_commit": base_commit,
            "base_tree": base_tree,
            "end_commit": end_commit,
            "end_tree": end_tree,
        }, indent=2)
        + "\n",
        encoding="utf-8",
    )
    logger.info(
        "patch: wrote delta.bundle %s..%s for %s",
        base_commit[:12], end_commit[:12], artifact_dir.name,
    )


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
    """Extract this session's delta into ``patch.diff`` (+ optional delta.bundle)."""
    base_path = base_tree_path(artifact_dir)
    if not base_path.is_file():
        await capture_baseline(sandbox_name, artifact_dir)
    base_tree = base_path.read_text(encoding="utf-8").strip()
    patch_path = patch_diff_path(artifact_dir)
    conv_id = _conv_id_from_sandbox(sandbox_name)
    base_commit = sandbox.frozen_head_for(conv_id) if conv_id else None

    async def _produce(*, exclude: tuple[str, ...] = ()) -> bool:
        return await _produce_diff(
            sandbox_name, base_tree, patch_path,
            exclude=exclude, base_commit=base_commit,
        )

    if forceful:
        sizes = await _staged_file_sizes(sandbox_name)
        exclude_rel, big_display = _oversized(sizes)
        has = await _produce(exclude=tuple(exclude_rel))
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
        has = await _produce(exclude=tuple(exclude_rel))
        return ExtractResult(
            patch_path=patch_path if has else None,
            empty=not has,
            needs_nag=False,
            big_files=big_display,
            nag_attempt=nag_attempt + 1,
        )

    has = await _produce()
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


async def apply_patch_on_host(patch_path: Path) -> tuple[bool, str]:
    return await _apply_git(None, patch_path)


async def apply_patch_to_sandbox(sandbox_name: str, patch_path: Path) -> tuple[bool, str]:
    return await _apply_git(sandbox_name, patch_path)


def apply_patch_to_sandbox_sync(sandbox_name: str, patch_path: Path) -> tuple[bool, str]:
    return asyncio.run(apply_patch_to_sandbox(sandbox_name, patch_path))


def _load_delta_meta(artifact_dir: Path) -> dict | None:
    meta_path = delta_json_path(artifact_dir)
    if not meta_path.is_file():
        return None
    try:
        data = json.loads(meta_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as e:
        logger.warning("patch: bad delta.json in %s: %s", artifact_dir, e)
        return None
    if not isinstance(data, dict):
        return None
    return data


def _parse_merge_tree_output(stdout: str) -> tuple[str, tuple[str, ...], str]:
    """Return ``(merged_tree, conflicted_paths, messages)`` from merge-tree stdout."""
    lines = stdout.splitlines()
    merged_tree = ""
    conflicts: list[str] = []
    msgs: list[str] = []
    for i, line in enumerate(lines):
        if i == 0 and len(line.strip()) >= 40 and " " not in line.strip() and "\t" not in line:
            merged_tree = line.strip()
            continue
        if "CONFLICT" in line:
            msgs.append(line)
            # "CONFLICT (content): Merge conflict in path" / add/add / …
            if " in " in line:
                conflicts.append(line.rsplit(" in ", 1)[-1].strip())
        elif line.startswith("Auto-merging "):
            msgs.append(line)
    # de-dupe preserving order
    seen: set[str] = set()
    uniq: list[str] = []
    for p in conflicts:
        if p and p not in seen:
            seen.add(p)
            uniq.append(p)
    return merged_tree, tuple(uniq), "\n".join(msgs)


async def _git_dest(
    target_sandbox: str | None, *args: str, timeout: float = _GIT_TIMEOUT,
) -> tuple[int, str, str]:
    if target_sandbox is None:
        return _git_host(*args, timeout=timeout)
    return await _git_in_sandbox(target_sandbox, *args, timeout=timeout)


async def _capture_dest_tree(target_sandbox: str | None) -> str:
    """Live worktree tree (HEAD + uncommitted dirt) via a throwaway index."""
    script = (
        'export GIT_INDEX_FILE="$(mktemp)"; '
        'trap \'rm -f "$GIT_INDEX_FILE"\' EXIT; '
        f"git -C {WORKSPACE} read-tree HEAD && "
        f"git -C {WORKSPACE} add -A && "
        f"git -C {WORKSPACE} write-tree"
    )
    if target_sandbox is None:
        try:
            proc = subprocess.run(
                ["bash", "-ec", script],
                capture_output=True, text=True, check=False, timeout=_GIT_TIMEOUT,
            )
        except subprocess.TimeoutExpired:
            raise RuntimeError("capture dest_tree timed out") from None
        if proc.returncode != 0:
            raise RuntimeError(
                f"capture dest_tree failed: {(proc.stderr or proc.stdout).strip()}"
            )
        tree = (proc.stdout or "").strip().splitlines()[-1].strip()
    else:
        rc, out, err = await sandbox._podman(
            "exec", target_sandbox, "bash", "-ec", script, timeout=_GIT_TIMEOUT,
        )
        if rc != 0:
            raise RuntimeError(f"capture dest_tree failed: {err or out}")
        tree = out.strip().splitlines()[-1].strip()
    if not tree:
        raise RuntimeError("capture dest_tree returned empty tree id")
    return tree


async def _delete_incoming_ref(target_sandbox: str | None) -> None:
    await _git_dest(target_sandbox, "update-ref", "-d", _INCOMING_REF)


async def merge_apply(
    target_sandbox: str | None,
    artifact_dir: Path,
) -> MergeResult:
    """3-way merge a producer delta into host (``target_sandbox=None``) or a sandbox.

    Host callers must hold :func:`host_apply_lock`. Falls back to textual
    ``git apply`` when ``delta.bundle`` is missing.
    """
    meta = _load_delta_meta(artifact_dir)
    bundle = delta_bundle_path(artifact_dir)
    patch_path = patch_diff_path(artifact_dir)

    if meta is None or not bundle.is_file():
        if not patch_path.is_file():
            return MergeResult(ok=False, err="no delta.bundle and no patch.diff")
        logger.warning(
            "patch: merge_apply falling back to git apply for %s", artifact_dir.name,
        )
        ok, err = await _apply_git(target_sandbox, patch_path)
        changed: tuple[str, ...] = ()
        if ok:
            # Best-effort path list from the unified diff.
            try:
                text = patch_path.read_text(encoding="utf-8", errors="replace")
                paths_set: list[str] = []
                for line in text.splitlines():
                    if line.startswith("diff --git "):
                        parts = line.split()
                        if len(parts) >= 4:
                            paths_set.append(parts[3].removeprefix("b/"))
                changed = tuple(dict.fromkeys(paths_set))
            except OSError:
                pass
        return MergeResult(
            ok=ok, err=err, used_fallback=True, changed_paths=changed,
        )

    base_tree = str(meta.get("base_tree") or "")
    end_tree = str(meta.get("end_tree") or "")
    end_commit = str(meta.get("end_commit") or "")
    if not (base_tree and end_tree and end_commit):
        return MergeResult(ok=False, err="delta.json missing required fields")

    bundle_abs = str(bundle.resolve())
    try:
        rc, out, err = await _git_dest(
            target_sandbox,
            "fetch", "--no-tags", bundle_abs, f"+{end_commit}:{_INCOMING_REF}",
        )
        if rc != 0:
            return MergeResult(
                ok=False,
                err=f"git fetch bundle failed: {(err or out).strip()}",
            )

        try:
            dest_tree = await _capture_dest_tree(target_sandbox)
        except RuntimeError as e:
            return MergeResult(ok=False, err=str(e))

        rc, mout, merr = await _git_dest(
            target_sandbox,
            "merge-tree", "--write-tree", f"--merge-base={base_tree}",
            dest_tree, end_tree,
        )
        merged_tree, conflicted, messages = _parse_merge_tree_output(mout)
        if not merged_tree:
            return MergeResult(
                ok=False,
                err=f"merge-tree produced no tree: {(merr or mout).strip()}",
                dest_tree=dest_tree,
                messages=messages,
            )

        if conflicted or "CONFLICT" in mout:
            return MergeResult(
                ok=False,
                conflicted_paths=conflicted,
                err=messages or "merge conflict",
                merged_tree=merged_tree,
                dest_tree=dest_tree,
                messages=messages,
            )
        if rc != 0:
            return MergeResult(
                ok=False,
                err=f"merge-tree failed (rc={rc}): {(merr or mout).strip()}",
                merged_tree=merged_tree,
                dest_tree=dest_tree,
                messages=messages,
            )

        # Clean merge: apply the net worktree diff (dest_tree → merged_tree).
        rc, diff_out, diff_err = await _git_dest(
            target_sandbox, "diff", "--binary", dest_tree, merged_tree,
        )
        if rc != 0:
            return MergeResult(
                ok=False,
                err=f"git diff merged failed: {(diff_err or diff_out).strip()}",
                merged_tree=merged_tree,
                dest_tree=dest_tree,
            )
        if not diff_out.strip():
            return MergeResult(
                ok=True,
                changed_paths=(),
                merged_tree=merged_tree,
                dest_tree=dest_tree,
            )

        rc, name_out, _ = await _git_dest(
            target_sandbox, "diff", "--name-only", dest_tree, merged_tree,
        )
        changed_paths = tuple(
            p for p in name_out.splitlines() if p.strip()
        ) if rc == 0 else ()

        # Pipe diff into apply on the same dest.
        if target_sandbox is None:
            try:
                proc = subprocess.run(
                    ["git", "-C", WORKSPACE, "apply", "--whitespace=nowarn"],
                    input=diff_out.encode("utf-8"),
                    capture_output=True, check=False, timeout=_GIT_TIMEOUT,
                )
            except subprocess.TimeoutExpired:
                return MergeResult(
                    ok=False, err="git apply of merged diff timed out",
                    merged_tree=merged_tree, dest_tree=dest_tree,
                )
            if proc.returncode != 0:
                err_s = (proc.stderr or proc.stdout or b"").decode("utf-8", "replace")
                return MergeResult(
                    ok=False, err=f"git apply merged diff failed: {err_s.strip()}",
                    merged_tree=merged_tree, dest_tree=dest_tree,
                    changed_paths=changed_paths,
                )
        else:
            # Write a temp patch on the shared volume so podman exec can read it.
            tmp = artifact_dir / ".merged.apply.diff"
            try:
                tmp.write_text(diff_out, encoding="utf-8")
                rc, aout, aerr = await sandbox._podman(
                    "exec", target_sandbox,
                    "git", "-C", WORKSPACE, "apply", "--whitespace=nowarn",
                    str(tmp.resolve()),
                    timeout=_GIT_TIMEOUT,
                )
            finally:
                tmp.unlink(missing_ok=True)
            if rc != 0:
                return MergeResult(
                    ok=False,
                    err=f"git apply merged diff failed: {(aerr or aout).strip()}",
                    merged_tree=merged_tree,
                    dest_tree=dest_tree,
                    changed_paths=changed_paths,
                )

        return MergeResult(
            ok=True,
            changed_paths=changed_paths,
            merged_tree=merged_tree,
            dest_tree=dest_tree,
        )
    finally:
        await _delete_incoming_ref(target_sandbox)


def merge_apply_sync(target_sandbox: str | None, artifact_dir: Path) -> MergeResult:
    return asyncio.run(merge_apply(target_sandbox, artifact_dir))


async def commit_conflict_markers_into_overlay(
    target_sandbox: str,
    merged_tree: str,
    conflicted_paths: tuple[str, ...],
) -> None:
    """Materialise conflict-marker blobs from ``merged_tree`` into the overlay."""
    for rel in conflicted_paths:
        if not rel or rel.startswith("/") or ".." in rel.split("/"):
            continue
        rc, _, err = await _git_in_sandbox(
            target_sandbox, "checkout", merged_tree, "--", rel,
        )
        if rc != 0:
            logger.warning("checkout conflict path %s failed: %s", rel, err)


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


async def publish_pending_patch(
    chat_id: str,
    sandbox_name: str,
    *,
    forceful: bool = False,
) -> bool:
    """Extract a top-level chat delta, stop the container, enter ``review`` phase.

    Does **not** touch the host worktree — Commit is a separate user action.
    Returns True if a size-guard nag was re-queued (sandbox kept running).
    """
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

    def _reset_guard(s: dict) -> dict:
        s["patch_guard_attempts"] = 0
        return s

    chat.update(_reset_guard)

    if result.has_content and result.patch_path is not None:
        stats = _stats_from_path(result.patch_path) or {}
        summary = f" — {format_patch_summary(stats)}" if stats else ""
        diff_text = ""
        try:
            diff_text = result.patch_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            pass
        info = paths.chat_info_state(chat_id).read()
        title = str(info.get("title") or f"Chat {chat_id}")

        def _pending(s: dict) -> dict:
            s["phase"] = "review"
            s["pending_patch"] = {
                "patch_path": str(result.patch_path),
                "stats": stats,
                "title": title,
                "ignored": False,
            }
            s["review_comments"] = []
            s["apply_attempt"] = 0
            s.pop("review_confirm_needed", None)
            s.pop("review_refreshed_diff", None)
            return s

        chat.update(_pending)
        paths.append_traj_note(
            chat, "patch",
            f"Ready for review{summary}",
            icon="🔎", status="ok",
            review="1",
            chat_id=chat_id,
            diff_bytes=str(stats.get("bytes", len(diff_text.encode("utf-8")))),
        )
        # Keep base_tree + bundle on disk for Commit / Reject rounds.
        await sandbox.stop_sandbox(chat_id)
        return False

    # Empty delta — nothing to review; tear down.
    base_tree_path(artifact_dir).unlink(missing_ok=True)
    delta_bundle_path(artifact_dir).unlink(missing_ok=True)
    delta_json_path(artifact_dir).unlink(missing_ok=True)
    patch_diff_path(artifact_dir).unlink(missing_ok=True)
    await sandbox.destroy_sandbox(chat_id)
    return False


async def finalize_chat_sandbox(
    chat_id: str,
    sandbox_name: str,
    *,
    forceful: bool = False,
) -> bool:
    """Backward-compat alias → :func:`publish_pending_patch` (no auto host apply)."""
    return await publish_pending_patch(
        chat_id, sandbox_name, forceful=forceful,
    )


def extract_run_patch(
    run_id: str,
    *,
    forceful: bool = False,
    mark_pending: bool = True,
) -> ExtractResult | None:
    """Extract a sub-agent's delta into ``patch.diff`` (+ bundle) and tear down.

    Does NOT apply to the parent — that is deferred to :func:`drain_parent_patches`
    so sibling children can't race concurrent merges into the parent overlay, and
    so patches land in dispatch order. When ``mark_pending`` and the patch has
    content, flags the run ``patch_pending`` for the drain to pick up. A
    size-guard nag short-circuits (no teardown).
    """
    if not sandbox.sandbox_enabled():
        return None
    state = paths.run_state_by_id(run_id).read()
    chat_id = state.get("chat_id", "")
    artifact_dir = paths.run_dir(chat_id, run_id)
    sbx = sandbox.sandbox_name(run_id)
    nag_attempt = int(state.get("patch_guard_attempts", 0))
    result = extract_patch_sync(
        sbx, artifact_dir, forceful=forceful, nag_attempt=nag_attempt,
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

    if result.has_content and result.patch_path is not None:
        stats = _stats_from_path(result.patch_path)
        if stats is not None:
            paths.append_traj_note(
                paths.run_state_by_id(run_id), "patch",
                f"Creating patch — {format_patch_summary(stats)}",
                icon="📦", status="ok",
            )

    # Bundle is on the shared harness volume; safe to destroy the child container.
    # Keep base_tree / delta.* for drain; only the live overlay goes away.
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
    """``(run_id, artifact_dir)`` for a parent's finished children whose patch is
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
        out.append((run_id, paths.run_dir(chat_id, run_id)))
    return out


def _clear_pending(run_id: str) -> None:
    paths.run_state_by_id(run_id).update(lambda s: {**s, "patch_pending": False})


def _provenance_commit_in_sandbox(parent_sandbox: str, child_id: str) -> None:
    """Best-effort commit of the just-merged child delta into the parent overlay."""
    short = child_id.split("_", 1)[-1][:8]
    msg = f"crack: integrate sub-agent {short}"
    script = (
        'export GIT_INDEX_FILE="$(mktemp)"; '
        'trap \'rm -f "$GIT_INDEX_FILE"\' EXIT; '
        f"git -C {WORKSPACE} read-tree HEAD && "
        f"git -C {WORKSPACE} add -A && "
        'tree=$(git -C /workspace write-tree) && '
        'parent=$(git -C /workspace rev-parse HEAD) && '
        f'git -C /workspace {" ".join(_COMMIT_IDENTITY)} '
        'commit-tree "$tree" -p "$parent" '
        f'-m {json.dumps(msg)} > /tmp/crack-prov-commit && '
        'git -C /workspace update-ref HEAD "$(cat /tmp/crack-prov-commit)"'
    )
    rc, out, err = sandbox._podman_sync(
        "exec", parent_sandbox, "bash", "-ec", script, timeout=_GIT_TIMEOUT,
    )
    if rc != 0:
        logger.warning(
            "drain: provenance commit for %s failed: %s",
            child_id, (err or out).strip()[:300],
        )


def drain_parent_patches(chat_id: str, parent_kind: str, parent_id: str) -> None:
    """Merge every pending child delta into the parent overlay, in dispatch order.

    Serialized so two finishing siblings never merge concurrently. Each successful
    merge gets a provenance commit in the parent overlay and an informational
    note (no human review gate — D5). Conflicts are handed to the managing agent.
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
            for child_id, artifact_dir in pending:
                patch_path = patch_diff_path(artifact_dir)
                if not patch_path.is_file() and not delta_bundle_path(artifact_dir).is_file():
                    _clear_pending(child_id)
                    progressed = True
                    continue
                short = child_id.split("_", 1)[-1][:8]
                stats = _stats_from_path(patch_path) if patch_path.is_file() else None
                summary = f" — {format_patch_summary(stats)}" if stats else ""
                _note_parent(
                    parent_kind, parent_id, chat_id, "patch",
                    f"Merging sub-agent {short} patch{summary}",
                    icon="🔀",
                )
                try:
                    result = merge_apply_sync(parent_sandbox, artifact_dir)
                except Exception:
                    logger.exception(
                        "drain: merge raised for %s; leaving pending for retry", child_id,
                    )
                    continue
                _clear_pending(child_id)
                progressed = True
                if result.ok:
                    _provenance_commit_in_sandbox(parent_sandbox, child_id)
                    _note_parent(
                        parent_kind, parent_id, chat_id, "patch",
                        f"✓ Merged sub-agent {short} patch{summary}",
                        icon="✅", status="ok",
                    )
                else:
                    _note_parent(
                        parent_kind, parent_id, chat_id, "patch",
                        f"✗ Sub-agent {short} patch failed to merge — conflict handed "
                        "to the managing agent",
                        icon="⚠", status="err",
                        detail=(result.err or "").strip()[-2000:],
                    )
                    notify_parent_apply_failure(
                        parent_kind, parent_id, chat_id,
                        result.err or "merge conflict",
                        patch_path if patch_path.is_file() else artifact_dir,
                    )
        finally:
            parent_state.update(lambda s: {**s, "patch_draining": False})
        if not progressed:
            return


# ---------------------------------------------------------------------------
# Human review gate actions (Commit / Reject / Ignore / per-line comments)
# ---------------------------------------------------------------------------


def _serialize_review_comments(comments: list) -> str:
    lines: list[str] = []
    for c in comments or []:
        if not isinstance(c, dict):
            continue
        path = str(c.get("file") or "")
        line = c.get("line", "")
        body = str(c.get("body") or "").strip()
        if not path or not body:
            continue
        lines.append(f"{path}:{line}: {body}")
    return "\n".join(lines)


def _clear_pending_patch_state(state: dict) -> dict:
    state.pop("pending_patch", None)
    state.pop("review_comments", None)
    state.pop("apply_attempt", None)
    state.pop("review_confirm_needed", None)
    state.pop("review_refreshed_diff", None)
    if state.get("phase") == "review":
        state["phase"] = "idle"
    return state


def format_merge_conflict_bounce(
    *,
    conflicted_paths: tuple[str, ...],
    upstream_diff: str,
    messages: str,
) -> str:
    paths_list = "\n".join(f"- {p}" for p in conflicted_paths) or "- (unknown)"
    return (
        "Your previous patch conflicted with the current host tree during Commit.\n"
        "Please resolve the conflicts and stop again so a fresh patch can be reviewed.\n\n"
        f"Conflicted paths:\n{paths_list}\n\n"
        f"merge-tree messages:\n{(messages or '').strip() or '(none)'}\n\n"
        "Upstream changes since your base (git diff base→host):\n"
        f"{(upstream_diff or '').strip() or '(empty)'}\n"
    )


async def _upstream_diff_text(base_tree: str) -> str:
    rc, out, err = _git_host("diff", "--binary", base_tree, "HEAD")
    if rc != 0:
        return f"(failed to diff upstream: {err or out})"
    return out


def handle_patch_comment(
    chat_id: str,
    *,
    file: str,
    side: str,
    line: int,
    body: str,
) -> str:
    """Append a per-line review comment; return updated comments JSON for the UI."""
    body = (body or "").strip()
    if not body or not file:
        return "[]"

    def _add(s: dict) -> dict:
        comments = list(s.get("review_comments") or [])
        comments.append({
            "file": file,
            "side": side if side in ("old", "new") else "new",
            "line": int(line),
            "body": body,
        })
        s["review_comments"] = comments
        return s

    st = paths.chat_state(chat_id).update(_add)
    return json.dumps(st.get("review_comments") or [])


async def handle_patch_commit(
    chat_id: str,
    *,
    message: str = "",
    confirm: bool = False,
) -> str:
    """Merge pending patch onto host under lock, path-scoped commit, teardown.

    Returns an HTML fragment (chat content is re-rendered by the route).
    On a materially-refreshed merge, sets ``review_confirm_needed`` and returns
    without committing until ``confirm=True``.
    """
    from crack_server import git_utils

    chat = paths.chat_state(chat_id)
    state = chat.read()
    pending = state.get("pending_patch") or {}
    if not pending or pending.get("ignored"):
        return "no pending patch"
    artifact_dir = paths.chat_dir(chat_id)
    patch_path = patch_diff_path(artifact_dir)
    info = paths.chat_info_state(chat_id).read()
    commit_msg = (message or "").strip() or str(
        pending.get("title") or info.get("title") or f"Chat {chat_id}"
    )

    # Self-mod gate at Commit time (tests → merge → commit → health-watch).
    if patch_path.is_file() and patch_touches_self_mod(patch_path):
        sbx_name = sandbox.sandbox_name(chat_id)
        if sandbox.container_exists_sync(sbx_name):
            await sandbox.ensure_sandbox(chat_id)
            passed, output = await run_sandbox_tests(sbx_name)
            if not passed:
                paths.append_traj_note(
                    chat, "patch",
                    "✗ Test gate failed — host repo untouched",
                    icon="⚠", status="err", detail=(output or "")[-2000:],
                )
                enqueue_chat_test_failure(chat_id, output, patch_path)
                return "test gate failed"

    displayed = ""
    if patch_path.is_file():
        try:
            displayed = patch_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            pass

    with host_apply_lock():
        result = await merge_apply(None, artifact_dir)
        if result.ok and result.merged_tree and result.dest_tree:
            rc, net_diff, _ = _git_host(
                "diff", "--binary", result.dest_tree, result.merged_tree,
            )
            if rc == 0 and net_diff.strip() and displayed.strip():
                # Stale-review reconfirm: if the net merge differs from what was
                # shown, require a second click (unless already confirming).
                if not confirm and net_diff.strip() != displayed.strip():
                    def _need(s: dict) -> dict:
                        s["review_confirm_needed"] = True
                        s["review_refreshed_diff"] = True
                        return s

                    chat.update(_need)
                    # Refresh patch.diff so the panel shows the live merge result.
                    try:
                        patch_path.write_text(net_diff, encoding="utf-8")
                    except OSError:
                        pass
                    paths.append_traj_note(
                        chat, "patch",
                        "Review refreshed — host moved since this patch was built. "
                        "Confirm Commit again to integrate the updated merge.",
                        icon="🔄", status="ok", review="1",
                    )
                    return "review refreshed"

        if not result.ok:
            attempt = int(state.get("apply_attempt") or 0) + 1
            budget = MERGE_AUTO_ATTEMPTS + MERGE_AGENT_ATTEMPTS
            chat.update(lambda s: {**s, "apply_attempt": attempt})
            if attempt <= budget and result.conflicted_paths:
                # Agent-assisted bounce: restart container, write markers, enqueue.
                meta = _load_delta_meta(artifact_dir) or {}
                base_tree = str(meta.get("base_tree") or "")
                upstream = await _upstream_diff_text(base_tree) if base_tree else ""
                await sandbox.ensure_sandbox(chat_id)
                if result.merged_tree:
                    await commit_conflict_markers_into_overlay(
                        sandbox.sandbox_name(chat_id),
                        result.merged_tree,
                        result.conflicted_paths,
                    )
                bounce = format_merge_conflict_bounce(
                    conflicted_paths=result.conflicted_paths,
                    upstream_diff=upstream,
                    messages=result.messages or result.err,
                )
                paths.append_traj_note(
                    chat, "patch",
                    "Commit conflict — bouncing to agent to regenerate",
                    icon="⚠", status="err",
                    detail=result.err[-2000:] if result.err else "",
                )
                enqueue_chat_system_message(
                    chat_id, bounce, source="patch_conflict",
                )
                return "conflict bounce"
            # Exhausted
            paths.append_traj_note(
                chat, "patch",
                "✗ Commit failed — merge conflict (retries exhausted)",
                icon="⚠", status="err",
                detail=(result.err or "")[-2000:],
            )
            record_chat_apply_failure(
                chat_id, result.err or "merge conflict",
                patch_path if patch_path.is_file() else artifact_dir,
            )
            chat.update(lambda s: {**_clear_pending_patch_state(s), "apply_attempt": 0})
            return "conflict terminal"

        # Success — path-scoped commit.
        changed = list(result.changed_paths) or []
        if not changed and patch_path.is_file():
            # Fallback path list from patch.diff
            for line in displayed.splitlines():
                if line.startswith("diff --git "):
                    parts = line.split()
                    if len(parts) >= 4:
                        changed.append(parts[3].removeprefix("b/"))
            changed = list(dict.fromkeys(changed))

        short = ""
        if changed:
            short = git_utils.commit(changed, commit_msg) or ""
        else:
            logger.info("patch commit: no changed paths for chat %s", chat_id)

        hash_note = f" ({short})" if short else ""
        paths.append_traj_note(
            chat, "patch",
            f"✓ Committed{hash_note}: {commit_msg}",
            icon="✅", status="ok",
        )
        if patch_path.is_file() and patch_touches_self_mod(patch_path):
            launch_health_watcher(chat_id, patch_path)

        chat.update(_clear_pending_patch_state)
        await sandbox.destroy_sandbox(chat_id)
        return "committed"


async def handle_patch_reject(chat_id: str, *, message: str = "") -> str:
    """Reject with optional message + serialized per-line comments → new agent turn."""
    chat = paths.chat_state(chat_id)
    state = chat.read()
    comments = list(state.get("review_comments") or [])
    serialized = _serialize_review_comments(comments)
    prompt = (message or "").strip()
    if serialized:
        prompt = f"{prompt}\n\n{serialized}".strip() if prompt else serialized
    if not prompt:
        prompt = (
            "The human rejected your pending patch. Please revise and stop again "
            "so a new patch can be reviewed."
        )

    def _clear_comments(s: dict) -> dict:
        s["review_comments"] = []
        s["phase"] = "chatting"
        # Keep pending_patch until the new turn publishes a replacement.
        return s

    chat.update(_clear_comments)
    await sandbox.ensure_sandbox(chat_id)
    enqueue_chat_system_message(chat_id, prompt, source="patch_reject")
    paths.append_traj_note(
        chat, "patch", "Rejected — sent feedback to agent", icon="↩", status="ok",
    )
    return "rejected"


def handle_patch_ignore(chat_id: str) -> str:
    """Leave the pending patch on disk; mark ignored (no teardown)."""
    chat = paths.chat_state(chat_id)

    def _ignore(s: dict) -> dict:
        pending = dict(s.get("pending_patch") or {})
        pending["ignored"] = True
        s["pending_patch"] = pending
        s["phase"] = "review"
        return s

    chat.update(_ignore)
    paths.append_traj_note(
        chat, "patch", "Ignored — patch left on disk", icon="⏸", status="ok",
    )
    return "ignored"
