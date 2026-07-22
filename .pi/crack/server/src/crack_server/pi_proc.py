"""`pi` subprocess runners: single-shot text calls and the RPC agent hop.

Split out of pi_runner.py (A6). Rate limiting and retry scheduling live in
ratelimit.py; turn accumulation lives in transcript.py.

The implementation is fully async (``arun_*``): subprocesses are awaited via
``asyncio.create_subprocess_exec`` so a waiting hop costs a coroutine, not a
thread. Thin sync wrappers (``run_pi_text`` / ``run_agent_hop``) preserve the
old API for callers that still run on threads (stage jobs dispatched via
``asyncio.to_thread``, tests) — they must not be called from inside a running
event loop. Everything here logs through the uvicorn logger.

Agent hops always use pi's ``--mode rpc`` (see :mod:`crack_server.pi_rpc`); the
old json-mode agent-hop path has been removed. ``CRACK_PI_JSON=1`` is now a hard
error rather than a fallback. One-off ``arun_pi_text`` calls stay on the simple
``--print`` path.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import os
import shlex
import signal
import subprocess
import time
from collections.abc import Callable
from pathlib import Path

from crack_server.paths import project_root
from crack_server.ratelimit import (
    PI_RETRY_ATTEMPTS,
    PI_TIMEOUT_SECONDS,
    _async_retry_backoff_sleep,
    _async_transient_backoff_sleep,
    async_wait_for_rate_limit,
    is_transient,
)
from crack_server.transcript import apply_event_to_turn

logger = logging.getLogger("uvicorn.error")

OUTPUT_TAIL_LINES = 10
STDERR_TAIL_LINES = 10
STREAM_LINE_LIMIT = 16 * 1024 * 1024

CRACK_EXT = project_root() / ".pi" / "extensions" / "crack" / "index.ts"
CRACK_SYSTEM_MD = project_root() / ".pi" / "SYSTEM.md"

# After a hop's RPC stream ends, wait this long for the pi process to exit
# before SIGKILL (MCP teardown linger).
EXIT_GRACE_SECONDS = 8


class PiError(RuntimeError):
    """A pi subprocess failure carrying a short message plus a ``detail`` blob
    for inline UI display. ``over_budget`` marks a durable error-budget spend."""

    def __init__(self, message: str, detail: str = "", over_budget: bool = False) -> None:
        super().__init__(message)
        self.detail = detail
        self.over_budget = over_budget


class PiStopped(RuntimeError):
    """The pi subprocess died because of an intentional external STOP."""


def _tail_text(text: str, n: int = OUTPUT_TAIL_LINES) -> str:
    lines = [ln for ln in (text or "").splitlines() if ln.strip()]
    return "\n".join(lines[-n:])


def _scrub_nuls(text: str) -> str:
    return text.replace("\x00", "") if text else text


def _compose_detail(output_tail: str, stderr_tail: str) -> str:
    stderr_tail = _scrub_nuls(stderr_tail)
    output_tail = _scrub_nuls(output_tail)
    if stderr_tail.strip():
        return "last stderr:\n" + stderr_tail
    if output_tail.strip():
        return "last output:\n" + output_tail
    return ""


def _record_attempt_error(record_error, entry: dict, log_prefix: str) -> int:
    if record_error is None:
        return 0
    try:
        return int(record_error(entry))
    except Exception:
        logger.exception("%s: record_error raised", log_prefix)
        return 0


def _agent_meta_path(pid_file: Path) -> Path:
    return pid_file.with_name(pid_file.stem + ".meta.json")


async def arun_pi_text(
    prompt: str,
    log_prefix: str,
    model: str,
    max_input_chars: int | None = None,
    record_prompt=None,
    pid_file: Path | None = None,
    stop_check: Callable[[], bool] | None = None,
    image_paths: list[Path] | None = None,
    record_error=None,
    sandbox: str | None = None,
) -> tuple[str, float]:
    """Run `pi` non-interactively with a single text prompt (async).

    When ``sandbox`` is set, the process runs inside that container (so image
    paths resolve against the sandbox's ``/workspace``), matching how agent hops
    already execute.
    """
    if max_input_chars is not None and len(prompt) > max_input_chars:
        logger.info("%s: truncating prompt from %d to %d chars", log_prefix, len(prompt), max_input_chars)
        prompt = prompt[:max_input_chars]

    cmd = ["pi", "--model", model, "--print", "--no-session", "--no-tools"]
    cmd += [f"@{p}" for p in (image_paths or [])]
    cmd += [prompt]

    logger.info("%s: full prompt:\n%s", log_prefix, prompt)
    logger.info("%s: timeout=%ss", log_prefix, PI_TIMEOUT_SECONDS)
    if sandbox:
        logger.info("+ podman exec %s %s", sandbox, shlex.join(cmd))
    else:
        logger.info("+ %s", shlex.join(cmd))

    if record_prompt is not None:
        try:
            record_prompt({"kind": "user_prompt", "compiled": prompt, "at": time.time()})
        except Exception:
            logger.exception("%s: record_prompt raised", log_prefix)

    first_attempt_at = time.monotonic()
    last_error = "pi command failed"
    last_detail = ""
    last_transient = False
    transient_reattempts = 0

    for attempt in range(PI_RETRY_ATTEMPTS):
        if attempt > 0:
            if last_transient:
                await _async_transient_backoff_sleep(transient_reattempts)
                transient_reattempts += 1
            else:
                await _async_retry_backoff_sleep(attempt, first_attempt_at)

        await async_wait_for_rate_limit(model)
        start = time.monotonic()
        try:
            result = await _arun_text_attempt(cmd, log_prefix, pid_file, sandbox=sandbox)
        except subprocess.TimeoutExpired as e:
            elapsed = time.monotonic() - start
            logger.error("%s: pi timed out after %.2fs (attempt %d)", log_prefix, elapsed, attempt + 1)
            last_error = "pi command timed out"
            last_detail = _compose_detail(_tail_text(e.stdout or ""), _tail_text(e.stderr or ""))
            last_transient = is_transient(last_detail)
            _record_attempt_error(record_error, {
                "message": last_error, "detail": last_detail, "rc": None,
                "attempt": attempt + 1, "phase": log_prefix,
            }, log_prefix)
            continue
        except FileNotFoundError:
            elapsed = time.monotonic() - start
            logger.error("%s: pi command not found on PATH (after %.2fs)", log_prefix, elapsed)
            last_error = "pi command not found"
            last_detail = ""
            last_transient = False
            _record_attempt_error(record_error, {
                "message": last_error, "detail": "", "rc": None,
                "attempt": attempt + 1, "phase": log_prefix,
            }, log_prefix)
            continue

        elapsed = time.monotonic() - start
        logger.info("%s: pi exited %d in %.2fs (attempt %d/%d)",
                    log_prefix, result.returncode, elapsed, attempt + 1, PI_RETRY_ATTEMPTS)

        if result.returncode == 0:
            text = result.stdout.strip()
            logger.info("%s: output summary: %r", log_prefix, text[:200])
            return text, elapsed

        if stop_check is not None and stop_check():
            raise PiStopped(f"pi run stopped by user (rc={result.returncode})")

        detail = _compose_detail(_tail_text(result.stdout or ""), _tail_text(result.stderr or ""))
        logger.error("%s: pi exited %d; last output:\n%s", log_prefix, result.returncode, detail)
        last_error = f"pi exited {result.returncode}"
        last_detail = detail
        last_transient = is_transient(detail)
        _record_attempt_error(record_error, {
            "message": last_error, "detail": detail, "rc": result.returncode,
            "attempt": attempt + 1, "phase": log_prefix,
        }, log_prefix)

    raise PiError(f"{last_error} after {PI_RETRY_ATTEMPTS} attempts", detail=last_detail)


def run_pi_text(*args, **kwargs) -> tuple[str, float]:
    """Sync wrapper over :func:`arun_pi_text` for thread-based callers."""
    return asyncio.run(arun_pi_text(*args, **kwargs))


async def _arun_text_attempt(
    cmd: list[str],
    log_prefix: str,
    pid_file: Path | None,
    *,
    sandbox: str | None = None,
) -> subprocess.CompletedProcess:
    if sandbox:
        from crack_server import sandbox as sandbox_mod

        proc = await sandbox_mod.exec_in(
            sandbox,
            cmd,
            cwd="/workspace",
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            limit=STREAM_LINE_LIMIT,
        )
    else:
        proc = await asyncio.create_subprocess_exec(
            *cmd, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
            start_new_session=True, cwd=str(project_root()),
            limit=STREAM_LINE_LIMIT,
        )
    if pid_file is not None:
        try:
            pid_file.parent.mkdir(parents=True, exist_ok=True)
            pid_file.write_text(str(proc.pid), encoding="utf-8")
        except OSError as e:
            logger.warning("%s: could not write pid_file %s: %s", log_prefix, pid_file, e)
    try:
        try:
            stdout_b, stderr_b = await asyncio.wait_for(
                proc.communicate(), timeout=PI_TIMEOUT_SECONDS
            )
        except asyncio.TimeoutError:
            _kill_process_group(proc)
            stdout_b, stderr_b = await proc.communicate()
            raise subprocess.TimeoutExpired(
                cmd, PI_TIMEOUT_SECONDS,
                output=(stdout_b or b"").decode("utf-8", "replace"),
                stderr=(stderr_b or b"").decode("utf-8", "replace"),
            )
    finally:
        if pid_file is not None:
            with contextlib.suppress(OSError):
                pid_file.unlink()
    return subprocess.CompletedProcess(
        cmd, proc.returncode,
        (stdout_b or b"").decode("utf-8", "replace"),
        (stderr_b or b"").decode("utf-8", "replace"),
    )


def _kill_process_group(proc: asyncio.subprocess.Process) -> None:
    try:
        os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
    except (ProcessLookupError, PermissionError, OSError):
        with contextlib.suppress(ProcessLookupError, OSError):
            proc.kill()


def kill_pid_file(pid_file: Path) -> bool:
    """Kill the agent hop named in ``pid_file``.

    For sandbox hops, reads a small ``.meta.json`` sidecar written alongside
  the pid file. For local hops, sends SIGTERM/SIGKILL to the process group.
    Returns True if a signal was delivered."""
    delivered = False
    meta_path = _agent_meta_path(pid_file)
    try:
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        meta = {}
    sbx = meta.get("sandbox")
    session_id = meta.get("session_id")
    if sbx and session_id:
        # Sandbox hop: the pid in ``pid_file`` is the host-side ``podman exec``
        # process, which — because ``sandbox.exec_in`` does NOT start a new
        # session — shares the in-process worker/uvicorn process group. A
        # host-side ``killpg`` here would SIGTERM the whole crack-server (the
        # worker runs in the uvicorn event loop), taking the UI down with it.
        # Kill only the pi *inside* the sandbox; the podman-exec process exits on
        # its own once its stdio peer dies. Do NOT fall through to the killpg.
        from crack_server import sandbox as sandbox_mod

        sandbox_mod.kill_session_sync(str(sbx), str(session_id))
        delivered = True
        with contextlib.suppress(OSError):
            meta_path.unlink()
        with contextlib.suppress(OSError):
            pid_file.unlink()
        return delivered

    try:
        pid = int(pid_file.read_text(encoding="utf-8").strip())
    except (OSError, ValueError):
        pid = None
    if pid is not None:
        try:
            pgid = os.getpgid(pid)
        except ProcessLookupError:
            pgid = None
        if pgid is not None:
            for sig in (signal.SIGTERM, signal.SIGKILL):
                try:
                    os.killpg(pgid, sig)
                    delivered = True
                except ProcessLookupError:
                    delivered = True
                    break
                if sig == signal.SIGTERM:
                    for _ in range(20):
                        try:
                            os.killpg(pgid, 0)
                        except ProcessLookupError:
                            break
                        time.sleep(0.1)
                    else:
                        continue
                    break
    with contextlib.suppress(OSError):
        pid_file.unlink()
    return delivered


class _TurnAccumulator:
    """In-progress turn dict plus monotonic timing for tool calls."""

    def __init__(self) -> None:
        self.current_turn: dict = {}
        self.turn_started_at: float | None = None
        self.tool_starts: dict = {}

    def apply(self, event: dict) -> None:
        apply_event_to_turn(event, self.current_turn)
        now = time.monotonic()
        etype = event.get("type")
        if etype == "turn_start":
            self.turn_started_at = now
        elif etype == "turn_end" and self.turn_started_at is not None:
            self.current_turn["elapsed"] = round(now - self.turn_started_at, 3)
        elif etype == "message_end":
            self._stamp_message_timing(event, now)

    def _stamp_message_timing(self, event: dict, now: float) -> None:
        msg = event.get("message")
        if not isinstance(msg, dict):
            return
        role = msg.get("role")
        if role == "toolResult":
            started = self.tool_starts.pop(msg.get("toolCallId"), None)
            if started is not None:
                for block in self.current_turn.get("tool_blocks", []):
                    if block.get("id") == msg.get("toolCallId"):
                        block["elapsed"] = round(now - started, 3)
                        break
            return
        if role == "user":
            return
        content = msg.get("content")
        if not isinstance(content, list):
            return
        for block in content:
            if (isinstance(block, dict) and block.get("type") == "toolCall"
                    and block.get("id") is not None):
                self.tool_starts[block["id"]] = now


async def arun_agent_hop(
    *,
    log_prefix: str,
    model: str,
    session_id: str,
    sessions_dir: Path,
    tools: str | None,
    message: str,
    start: float,
    sentinel: str | None,
    timeout_seconds: int,
    persist_turn,
    hop: int = 1,
    pid_file: Path | None = None,
    stop_check=None,
    record_prompt=None,
    record_error=None,
    error_budget: Callable[[], int] | None = None,
    env_extra: dict[str, str] | None = None,
    waiting_check: Callable[[], bool] | None = None,
    append_system_prompt: str | None = None,
    swap_after_edit: bool = False,
    todo_already: bool = False,
    sandbox: str | None = None,
    resume_session: bool = False,
    on_first_byte: Callable[[float], None] | None = None,
) -> str:
    """Run one hop of a tool-using agent via pi RPC (async).

    Returns the stop reason or raises :class:`PiError` on genuine failure.
    LLM retries are owned by pi's auto-retry; Python only safety-retries
    infrastructure failures (RPC channel/process died before ``agent_settled``).
    """
    if os.environ.get("CRACK_PI_JSON"):
        raise RuntimeError(
            "CRACK_PI_JSON=1 is no longer supported — agent hops use RPC mode"
        )

    from crack_server import pi_rpc

    return await pi_rpc.arun_agent_hop_rpc(
        log_prefix=log_prefix,
        model=model,
        session_id=session_id,
        sessions_dir=sessions_dir,
        tools=tools,
        message=message,
        start=start,
        sentinel=sentinel,
        timeout_seconds=timeout_seconds,
        persist_turn=persist_turn,
        hop=hop,
        pid_file=pid_file,
        stop_check=stop_check,
        record_prompt=record_prompt,
        record_error=record_error,
        error_budget=error_budget,
        env_extra=env_extra,
        waiting_check=waiting_check,
        append_system_prompt=append_system_prompt,
        swap_after_edit=swap_after_edit,
        todo_already=todo_already,
        sandbox=sandbox,
        resume_session=resume_session,
        on_first_byte=on_first_byte,
    )


def run_agent_hop(**kwargs) -> str:
    """Sync wrapper over :func:`arun_agent_hop` for thread-based callers."""
    return asyncio.run(arun_agent_hop(**kwargs))
