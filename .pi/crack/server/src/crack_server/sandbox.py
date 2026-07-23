"""Per-conversation podman sandbox lifecycle (create / exec / kill / destroy).

crack-dev drives the host podman socket (see ``run.sh``) to run one long-lived
sandbox container per chat or sub-agent run (cheap init via ``_sandbox_start.sh``,
no eager MCP HTTP bridges). Agent hops are executed
via ``podman exec`` into that container (Plan 3 wires pi_proc to this module).
"""

from __future__ import annotations

import asyncio
import logging
import os
import subprocess
import time
from collections.abc import Mapping
from pathlib import Path

from crack_server.paths import RUN_ID_RE, chat_dir, find_run_dir, project_root

logger = logging.getLogger("uvicorn.error")

CRACK_NET = "crack-docker-net"
HARNESS_VOLUME = "crack-harness-data"
TARGET_VOLUME = "crack-dev-target-dir"
SANDBOX_IMAGE = "localhost/crack-dev:latest"
_DEFAULT_EXEC_TIMEOUT = 60
_KILL_GRACE_SECONDS = 2.0


def sandbox_name(conv_id: str) -> str:
    return f"crack-sbx-{conv_id}"


def sandbox_enabled() -> bool:
    """True when agent hops should run inside per-conversation podman sandboxes.

    Off in tests (``CRACK_PI_PROJECT_ROOT`` not ``/workspace``) unless forced.
    On in crack-dev when ``CRACK_HARNESS_DATA_DIR`` is set. Override with
    ``CRACK_SANDBOX_ENABLED=0|1``."""
    raw = os.environ.get("CRACK_SANDBOX_ENABLED")
    if raw is not None:
        return raw.strip().lower() not in ("0", "false", "no", "off")
    if not os.environ.get("CRACK_HARNESS_DATA_DIR"):
        return False
    try:
        return project_root().resolve() == Path("/workspace").resolve()
    except OSError:
        return False


def _host_repo() -> str:
    try:
        return os.environ["CRACK_HOST_REPO_ROOT"]
    except KeyError as e:
        raise RuntimeError("CRACK_HOST_REPO_ROOT is not set") from e


def _harness_data_dir() -> Path:
    raw = os.environ.get("CRACK_HARNESS_DATA_DIR", "/crack-harness-data")
    return Path(raw)


def _overlay_root(conv_id: str) -> Path:
    if RUN_ID_RE.fullmatch(conv_id):
        return find_run_dir(conv_id) / "overlays"
    return chat_dir(conv_id) / "overlays"


def _overlay_dirs(conv_id: str) -> tuple[Path, Path]:
    base = _overlay_root(conv_id)
    return base / "upper", base / "work"


def _target_overlay_dirs(conv_id: str) -> tuple[Path, Path]:
    base = _overlay_root(conv_id)
    return base / "target-upper", base / "target-work"


def _host_overlay_root(vol: str, conv_id: str) -> str:
    rel = _overlay_root(conv_id).relative_to(_harness_data_dir())
    return f"{vol}/{rel}"


def overlay_base_dir(conv_id: str) -> Path:
    """Frozen tracked-tree materialisation used as the sandbox ``:O`` lower."""
    return _overlay_root(conv_id) / "base"


def overlay_upper_dir(conv_id: str) -> Path:
    """Writable overlay upper — where files the sandbox creates under
    ``/workspace`` land, readable from the host while the overlay exists.

    Used by the trajectory media hook to read images the agent produced inside
    its sandbox (e.g. screenshots) whose paths do not resolve in the server's own
    filesystem."""
    return _overlay_dirs(conv_id)[0]


def overlay_tree_path(conv_id: str) -> Path:
    """File holding the frozen ``git write-tree`` id for this sandbox."""
    return _overlay_root(conv_id) / "tree"


def snapshot_host_tree(root: Path | None = None) -> str:
    """``git write-tree`` on the host checkout (clean gate ⇒ equals HEAD^{tree})."""
    repo = root or project_root()
    try:
        proc = subprocess.run(
            ["git", "-C", str(repo), "write-tree"],
            capture_output=True, text=True, check=False, timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        raise RuntimeError(f"git write-tree failed: {e}") from e
    if proc.returncode != 0:
        raise RuntimeError(f"git write-tree failed: {(proc.stderr or proc.stdout).strip()}")
    tree = proc.stdout.strip()
    if not tree:
        raise RuntimeError("git write-tree returned empty tree id")
    return tree


def materialise_frozen_base(tree: str, dest: Path, *, repo: Path | None = None) -> None:
    """Materialise tracked files from ``tree`` into ``dest`` (no gitignored junk).

    Also seeds a minimal ``.git`` whose object alternates point at the host
    object store bind-mounted into sandboxes at ``/crack-host-git-objects``,
    so in-sandbox ``git write-tree`` / ``git diff`` can resolve the frozen
    base tree while new blobs land in the writable overlay upper.
    """
    repo = repo or project_root()
    if (dest / ".git" / "objects" / "info" / "alternates").is_file() and any(dest.iterdir()):
        # Already materialised (idempotent re-ensure).
        return
    dest.mkdir(parents=True, exist_ok=True)
    arch = subprocess.Popen(
        ["git", "-C", str(repo), "archive", tree],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert arch.stdout is not None
    tar = subprocess.run(
        ["tar", "-x", "-C", str(dest)],
        stdin=arch.stdout, capture_output=True, check=False,
    )
    arch_stderr = arch.communicate()[1]
    if arch.returncode != 0 or tar.returncode != 0:
        raise RuntimeError(
            f"git archive|tar failed for {tree[:12]}: "
            f"{(arch_stderr or b'').decode('utf-8', 'replace')[:200]} "
            f"{(tar.stderr or b'').decode('utf-8', 'replace')[:200]}"
        )
    init = subprocess.run(
        ["git", "init", str(dest)],
        capture_output=True, text=True, check=False,
    )
    if init.returncode != 0:
        raise RuntimeError(f"git init in frozen base failed: {init.stderr or init.stdout}")
    alt = dest / ".git" / "objects" / "info" / "alternates"
    alt.parent.mkdir(parents=True, exist_ok=True)
    # Sandbox-visible path of the host object store (mounted in ensure_sandbox).
    alt.write_text("/crack-host-git-objects\n", encoding="utf-8")
    # Record the frozen tree id next to the base for callers.
    (dest.parent / "tree").write_text(tree + "\n", encoding="utf-8")
    logger.info("materialised frozen base %s → %s", tree[:12], dest)


def frozen_tree_for(conv_id: str) -> str | None:
    """Return the recorded frozen tree id for ``conv_id``, or None."""
    try:
        path = overlay_tree_path(conv_id)
    except ValueError:
        return None
    if not path.is_file():
        return None
    return path.read_text(encoding="utf-8").strip() or None


def _podman_sync(*args: str, timeout: float = _DEFAULT_EXEC_TIMEOUT) -> tuple[int, str, str]:
    """Sync podman for stop handlers and startup recovery (no event loop)."""
    cmd = ("podman", *args)
    logger.debug("podman %s", " ".join(args))
    try:
        proc = subprocess.run(
            cmd, capture_output=True, timeout=timeout, check=False,
        )
    except subprocess.TimeoutExpired:
        logger.error("podman timed out after %.0fs: %s", timeout, " ".join(args))
        raise
    out = (proc.stdout or b"").decode("utf-8", "replace")
    err = (proc.stderr or b"").decode("utf-8", "replace")
    return proc.returncode if proc.returncode is not None else -1, out, err


async def _podman(*args: str, timeout: float = _DEFAULT_EXEC_TIMEOUT) -> tuple[int, str, str]:
    """Run one podman command against the host socket; return (rc, stdout, stderr)."""
    cmd = ("podman", *args)
    logger.debug("podman %s", " ".join(args))
    proc = await asyncio.create_subprocess_exec(
        *cmd,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    try:
        stdout_b, stderr_b = await asyncio.wait_for(proc.communicate(), timeout=timeout)
    except asyncio.TimeoutError:
        proc.kill()
        await proc.communicate()
        logger.error("podman timed out after %.0fs: %s", timeout, " ".join(args))
        raise
    out = (stdout_b or b"").decode("utf-8", "replace")
    err = (stderr_b or b"").decode("utf-8", "replace")
    rc = proc.returncode if proc.returncode is not None else -1
    if rc != 0:
        logger.debug("podman rc=%d stderr=%r stdout=%r", rc, err, out)
    return rc, out, err


async def harness_volume_host_path() -> str:
    """Host mountpoint for ``crack-harness-data`` (overlay upper/work paths)."""
    rc, out, err = await _podman(
        "volume", "inspect", HARNESS_VOLUME, "--format", "{{.Mountpoint}}",
    )
    if rc != 0:
        raise RuntimeError(f"podman volume inspect {HARNESS_VOLUME} failed: {err or out}")
    path = out.strip()
    if not path:
        raise RuntimeError(f"podman volume inspect {HARNESS_VOLUME} returned empty mountpoint")
    return path


async def target_volume_host_path() -> str:
    """Host mountpoint for ``crack-dev-target-dir`` (shared target / RAG base)."""
    rc, out, err = await _podman(
        "volume", "inspect", TARGET_VOLUME, "--format", "{{.Mountpoint}}",
    )
    if rc != 0:
        raise RuntimeError(f"podman volume inspect {TARGET_VOLUME} failed: {err or out}")
    path = out.strip()
    if not path:
        raise RuntimeError(f"podman volume inspect {TARGET_VOLUME} returned empty mountpoint")
    return path


async def ensure_network() -> None:
    rc, *_ = await _podman("network", "exists", CRACK_NET)
    if rc != 0:
        rc, out, err = await _podman("network", "create", CRACK_NET)
        if rc != 0:
            raise RuntimeError(f"podman network create {CRACK_NET} failed: {err or out}")
        logger.info("created podman network %s", CRACK_NET)


async def ensure_sandbox(conv_id: str, *, parent_conv: str | None = None) -> str:
    """Idempotently create+start the sandbox; return its name. Safe to call every hop.

    The ``:O`` lower is a **frozen git-tree snapshot** (tracked files only), not
    the live host tree — so concurrent chats and hand-edits cannot mutate each
    other's lower. Sub-agents pass ``parent_conv`` to reuse the parent's frozen
    base (same tree id) instead of re-snapshotting the host.
    """
    name = sandbox_name(conv_id)
    rc, *_ = await _podman("container", "exists", name)
    if rc == 0:
        await _podman("start", name)
        return name

    await ensure_network()
    vol = await harness_volume_host_path()
    target_vol = await target_volume_host_path()
    ovl = _host_overlay_root(vol, conv_id)
    upper, work = _overlay_dirs(conv_id)
    upper.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)
    t_upper, t_work = _target_overlay_dirs(conv_id)
    t_upper.mkdir(parents=True, exist_ok=True)
    t_work.mkdir(parents=True, exist_ok=True)

    if parent_conv and overlay_base_dir(parent_conv).is_dir():
        # Share the parent's immutable lower (overlay lower is read-only).
        lower_host = f"{_host_overlay_root(vol, parent_conv)}/base"
        tree = frozen_tree_for(parent_conv)
        if tree:
            _overlay_root(conv_id).mkdir(parents=True, exist_ok=True)
            overlay_tree_path(conv_id).write_text(tree + "\n", encoding="utf-8")
    else:
        tree = snapshot_host_tree()
        base = overlay_base_dir(conv_id)
        materialise_frozen_base(tree, base)
        lower_host = f"{ovl}/base"

    # Every sandbox (top-level or sub-agent) overlays /workspace/target on the
    # SAME shared base — the ``crack-dev-target-dir`` volume that holds the RAG
    # base index. A child must NOT chain its lower to the parent's *live*
    # target-upper: overlayfs forbids mutating a lowerdir under an active mount
    # (ESTALE / cache incoherence), and the parent keeps writing there. So a
    # sub-agent sees the frozen base index, not the parent's mid-run re-index
    # (accepted out-of-scope; see plan §8). Per-conversation upper/work still
    # isolate each sandbox's own re-index into its own overlay upper.
    target_lower = target_vol

    host_git_objects = f"{_host_repo()}/.git/objects"
    rc, out, err = await _podman(
        "run", "-d", "--name", name, "--network", CRACK_NET,
        "-v", f"{lower_host}:/workspace:O,upperdir={ovl}/upper,workdir={ovl}/work",
        "-v", f"{host_git_objects}:/crack-host-git-objects:ro",
        "-v", (
            f"{target_lower}:/workspace/target:O,"
            f"upperdir={ovl}/target-upper,workdir={ovl}/target-work"
        ),
        "-v", "crack-dev-root-dir:/root:O",
        "-v", f"{HARNESS_VOLUME}:/crack-harness-data",
        "-e", "CRACK_HARNESS_DATA_DIR=/crack-harness-data",
        "-e", "CRACK_PI_PROJECT_ROOT=/workspace",
        "-e", "CRACK_PI_HOST=crack-dev",
        SANDBOX_IMAGE,
        "bash", "/workspace/_docker/_sandbox_start.sh",
        timeout=120,
    )
    if rc != 0:
        raise RuntimeError(f"podman run {name} failed: {err or out}")
    logger.info(
        "started sandbox %s for conv %s (frozen tree %s)",
        name, conv_id, (tree or "?")[:12],
    )
    return name


async def exec_in(
    name: str,
    argv: list[str],
    *,
    env: Mapping[str, str] | None = None,
    cwd: str = "/workspace",
    detached: bool = False,
    stdout: int | None = None,
    stderr: int | None = None,
    interactive: bool = False,
    stdin: int | None = None,
    limit: int | None = None,
) -> asyncio.subprocess.Process:
    """Build and launch ``podman exec``; return the asyncio subprocess.

  Plan 3 tails stdout/stderr or a shared hop output file from the returned
  process. Pass ``interactive=True`` and ``stdin=PIPE`` for RPC-mode pi
  (stdin/stdout JSONL protocol)."""
    cmd: list[str] = ["podman", "exec"]
    if detached:
        cmd.append("-d")
    if interactive:
        cmd.append("-i")
    if cwd:
        cmd.extend(["-w", cwd])
    if env:
        for key, value in env.items():
            cmd.extend(["-e", f"{key}={value}"])
    cmd.append(name)
    cmd.extend(argv)

    kwargs: dict = {}
    if stdin is not None:
        kwargs["stdin"] = stdin
    if stdout is not None:
        kwargs["stdout"] = stdout
    if stderr is not None:
        kwargs["stderr"] = stderr
    if limit is not None:
        # StreamReader buffer cap for stdout/stderr readline(). Without this the
        # asyncio default (64 KiB) trips "Separator is not found, and chunk
        # exceed the limit" on a single large JSONL line (e.g. an RPC event
        # carrying a base64 browser screenshot).
        kwargs["limit"] = limit

    logger.debug("exec_in %s: %s", name, " ".join(argv))
    return await asyncio.create_subprocess_exec(*cmd, **kwargs)


async def _pkill_in_sandbox(name: str, signal_name: str, session_id: str) -> int:
    rc, out, err = await _podman(
        "exec", name, "pkill", f"-{signal_name}", "-f", session_id,
    )
    # pkill returns 1 when no processes matched — not an error for us.
    if rc not in (0, 1):
        logger.warning(
            "pkill -%s -f %s in %s failed (rc=%d): %s",
            signal_name, session_id, name, rc, err or out,
        )
    return rc


async def _session_alive(name: str, session_id: str) -> bool:
    rc, out, _ = await _podman(
        "exec", name, "pgrep", "-f", session_id,
    )
    return rc == 0 and bool(out.strip())


async def kill_session(name: str, session_id: str) -> None:
    """Mid-run kill: signal only the pi for one session (SIGTERM then SIGKILL)."""
    await _pkill_in_sandbox(name, "TERM", session_id)
    deadline = time.monotonic() + _KILL_GRACE_SECONDS
    while time.monotonic() < deadline:
        if not await _session_alive(name, session_id):
            return
        await asyncio.sleep(0.1)
    await _pkill_in_sandbox(name, "KILL", session_id)


def session_alive_sync(name: str, session_id: str) -> bool:
    rc, out, _ = _podman_sync("exec", name, "pgrep", "-f", session_id)
    return rc == 0 and bool(out.strip())


def kill_session_sync(name: str, session_id: str) -> None:
    """Sync wrapper for stop routes and ``kill_pid_file``."""
    rc, _, _ = _podman_sync("exec", name, "pkill", "-TERM", "-f", session_id)
    if rc not in (0, 1):
        return
    deadline = time.monotonic() + _KILL_GRACE_SECONDS
    while time.monotonic() < deadline:
        if not session_alive_sync(name, session_id):
            return
        time.sleep(0.1)
    _podman_sync("exec", name, "pkill", "-KILL", "-f", session_id)


def destroy_sandbox_sync(conv_id: str) -> None:
    """Sync wrapper for terminal handoffs from sync callers."""
    name = sandbox_name(conv_id)
    rc, _, _ = _podman_sync("container", "exists", name)
    if rc != 0:
        return
    _podman_sync("kill", name)
    _podman_sync("rm", "-f", name)


async def destroy_sandbox(conv_id: str) -> None:
    """Stop and remove the sandbox container for a conversation."""
    name = sandbox_name(conv_id)
    rc, out, err = await _podman("container", "exists", name)
    if rc != 0:
        return
    await _podman("kill", name)
    rc, out, err = await _podman("rm", "-f", name)
    if rc != 0:
        logger.warning("podman rm -f %s failed (rc=%d): %s", name, rc, err or out)
    else:
        logger.info("destroyed sandbox %s", name)


def container_exists_sync(name: str) -> bool:
    rc, _, _ = _podman_sync("container", "exists", name)
    return rc == 0


async def path_is_file(name: str, path: str) -> bool:
    """True when ``path`` is a regular file inside the sandbox."""
    rc, _, _ = await _podman("exec", name, "test", "-f", path)
    return rc == 0


async def identify_ok(name: str, path: str) -> bool:
    """True when imagemagick ``identify`` accepts ``path`` inside the sandbox."""
    rc, _, _ = await _podman("exec", name, "identify", path, timeout=20)
    return rc == 0
