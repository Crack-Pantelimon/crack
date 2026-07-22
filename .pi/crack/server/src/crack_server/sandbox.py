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

from crack_server.paths import project_root

logger = logging.getLogger("uvicorn.error")

CRACK_NET = "crack-net"
HARNESS_VOLUME = "crack-harness-data"
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


def _overlay_dirs(conv_id: str) -> tuple[Path, Path]:
    base = _harness_data_dir() / "overlays" / conv_id
    return base / "upper", base / "work"


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


async def ensure_network() -> None:
    rc, *_ = await _podman("network", "exists", CRACK_NET)
    if rc != 0:
        rc, out, err = await _podman("network", "create", CRACK_NET)
        if rc != 0:
            raise RuntimeError(f"podman network create {CRACK_NET} failed: {err or out}")
        logger.info("created podman network %s", CRACK_NET)


async def ensure_sandbox(conv_id: str) -> str:
    """Idempotently create+start the sandbox; return its name. Safe to call every hop."""
    name = sandbox_name(conv_id)
    rc, *_ = await _podman("container", "exists", name)
    if rc == 0:
        await _podman("start", name)
        return name

    await ensure_network()
    vol = await harness_volume_host_path()
    ovl = f"{vol}/overlays/{conv_id}"
    upper, work = _overlay_dirs(conv_id)
    upper.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)

    rc, out, err = await _podman(
        "run", "-d", "--name", name, "--network", CRACK_NET,
        "-v", f"{_host_repo()}:/workspace:O,upperdir={ovl}/upper,workdir={ovl}/work",
        "-v", "crack-dev-target-dir:/workspace/target:O",
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
    logger.info("started sandbox %s for conv %s", name, conv_id)
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
) -> asyncio.subprocess.Process:
    """Build and launch ``podman exec``; return the asyncio subprocess.

  Plan 3 tails stdout/stderr or a shared hop output file from the returned process."""
    cmd: list[str] = ["podman", "exec"]
    if detached:
        cmd.append("-d")
    if cwd:
        cmd.extend(["-w", cwd])
    if env:
        for key, value in env.items():
            cmd.extend(["-e", f"{key}={value}"])
    cmd.append(name)
    cmd.extend(argv)

    kwargs: dict = {}
    if stdout is not None:
        kwargs["stdout"] = stdout
    if stderr is not None:
        kwargs["stderr"] = stderr

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
