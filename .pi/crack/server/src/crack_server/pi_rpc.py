"""Drive pi in ``--mode rpc``: authoritative completion via ``agent_settled``.

Default agent-hop control plane (see :func:`crack_server.pi_proc.arun_agent_hop`).
Set ``CRACK_PI_JSON=1`` to force the legacy json-mode path during transition.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import os
import shlex
import time
from collections import deque
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

from crack_server.paths import project_root
from crack_server.pi_proc import (
    CRACK_EXT,
    CRACK_SYSTEM_MD,
    EXIT_GRACE_SECONDS,
    PiError,
    STREAM_LINE_LIMIT,
    _TurnAccumulator,
    _agent_meta_path,
    _compose_detail,
    _record_attempt_error,
    _scrub_nuls,
)
from crack_server.ratelimit import RESUME_MESSAGE, async_wait_for_rate_limit
from crack_server.transcript import text_from_content, turn_has_content

logger = logging.getLogger("uvicorn.error")

# Infrastructure-only retries: RPC channel/process died before agent_settled.
RPC_SAFETY_MAX_ATTEMPTS = 3  # initial + 2 safety-net retries
RPC_SAFETY_BACKOFF_SECONDS = 0.5
STDERR_TAIL_LINES = 10


def _write_agent_meta(pid_file: Path, *, sandbox: str | None, session_id: str) -> None:
    if not sandbox:
        return
    try:
        pid_file.parent.mkdir(parents=True, exist_ok=True)
        _agent_meta_path(pid_file).write_text(
            json.dumps({"sandbox": sandbox, "session_id": session_id}),
            encoding="utf-8",
        )
    except OSError as e:
        logger.warning("could not write agent meta %s: %s", pid_file, e)


def _unlink_agent_meta(pid_file: Path | None) -> None:
    if pid_file is None:
        return
    with contextlib.suppress(OSError):
        _agent_meta_path(pid_file).unlink()


def _build_rpc_cmd(
    *,
    model: str,
    session_id: str,
    sessions_dir: Path,
    tools: str | None,
    append_system_prompt: str | None,
) -> list[str]:
    cmd = ["pi", "--mode", "rpc", "--model", model]
    if CRACK_EXT.exists():
        cmd += ["-e", str(CRACK_EXT)]
    if tools is not None:
        cmd += ["--tools", tools]
    if CRACK_SYSTEM_MD.exists():
        cmd += ["--append-system-prompt", str(CRACK_SYSTEM_MD)]
    if append_system_prompt:
        cmd += ["--append-system-prompt", append_system_prompt]
    cmd += ["--session-id", session_id, "--session-dir", str(sessions_dir)]
    return cmd


async def _launch_rpc_proc(
    argv: list[str],
    *,
    sandbox: str | None,
    env: dict[str, str] | None,
) -> asyncio.subprocess.Process:
    if sandbox:
        from crack_server import sandbox as sandbox_mod

        return await sandbox_mod.exec_in(
            sandbox,
            argv,
            env=env,
            cwd="/workspace",
            interactive=True,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    return await asyncio.create_subprocess_exec(
        *argv,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        cwd=str(project_root()),
        env={**os.environ, **(env or {})},
        limit=STREAM_LINE_LIMIT,
        start_new_session=True,
    )


async def _send_line(proc: asyncio.subprocess.Process, payload: dict) -> None:
    assert proc.stdin is not None
    proc.stdin.write((json.dumps(payload) + "\n").encode())
    await proc.stdin.drain()


async def _send_abort(proc: asyncio.subprocess.Process) -> None:
    await _send_line(proc, {"type": "abort"})


async def _drain_stderr(proc: asyncio.subprocess.Process, tail: deque[str]) -> None:
    if proc.stderr is None:
        return
    while True:
        line_b = await proc.stderr.readline()
        if not line_b:
            break
        line = line_b.decode("utf-8", errors="replace").rstrip("\r\n")
        if not line.strip():
            continue
        tail.append(line[:500])
        while len(tail) > STDERR_TAIL_LINES:
            tail.popleft()


async def _rpc_command(
    proc: asyncio.subprocess.Process,
    payload: dict,
    *,
    expect_id: str | None = None,
    expect_command: str | None = None,
    timeout: float = 30.0,
) -> dict | None:
    """Send one RPC command and wait for its matching ``response`` event."""
    await _send_line(proc, payload)
    cmd_id = expect_id or payload.get("id")
    cmd_name = expect_command or payload.get("type")
    deadline = time.monotonic() + timeout
    assert proc.stdout is not None
    while time.monotonic() < deadline:
        try:
            line = await asyncio.wait_for(proc.stdout.readline(), timeout=deadline - time.monotonic())
        except asyncio.TimeoutError:
            return None
        if not line:
            return None
        text = line.decode("utf-8", errors="replace").strip()
        if not text:
            continue
        try:
            ev = json.loads(text)
        except json.JSONDecodeError:
            continue
        if ev.get("type") != "response":
            continue
        if cmd_id is not None and ev.get("id") != cmd_id:
            continue
        if cmd_id is None and cmd_name and ev.get("command") != cmd_name:
            continue
        return ev
    return None


async def _shutdown(
    proc: asyncio.subprocess.Process,
    *,
    sandbox: str | None,
    session_id: str,
    log_prefix: str,
    hop: int,
) -> None:
    if proc.stdin is not None:
        proc.stdin.close()
        with contextlib.suppress(Exception):
            await proc.stdin.wait_closed()
    try:
        await asyncio.wait_for(proc.wait(), timeout=EXIT_GRACE_SECONDS)
    except asyncio.TimeoutError:
        if sandbox:
            from crack_server import sandbox as sandbox_mod

            if sandbox_mod.session_alive_sync(sandbox, session_id):
                logger.info(
                    "%s hop %d: rpc pi still alive after %.0fs; killing session %s",
                    log_prefix, hop, EXIT_GRACE_SECONDS, session_id,
                )
                sandbox_mod.kill_session_sync(sandbox, session_id)
        else:
            proc.kill()
            await proc.wait()


def _tick_wait_credit(
    waiting_check: Callable[[], bool] | None,
    *,
    wait_credit: float,
    waiting_since: float | None,
    log_prefix: str,
    hop: int,
) -> tuple[float, float | None]:
    waiting = False
    if waiting_check is not None:
        try:
            waiting = bool(waiting_check())
        except Exception:
            logger.exception("%s hop %d: waiting_check raised", log_prefix, hop)
    now = time.monotonic()
    if waiting and waiting_since is None:
        return wait_credit, now
    if not waiting and waiting_since is not None:
        return wait_credit + (now - waiting_since), None
    return wait_credit, waiting_since


def _message_update_error(ev: dict) -> str | None:
    ame = ev.get("assistantMessageEvent")
    if not isinstance(ame, dict) or ame.get("type") != "error":
        return None
    for key in ("error", "message", "text", "delta"):
        val = ame.get(key)
        if val:
            return _scrub_nuls(str(val))
    return "error"


@dataclass
class _RpcAttemptResult:
    reason: str
    settled: bool
    persisted: int
    pi_failure: PiError | None
    infrastructure_failure: bool
    stderr_tail: str


async def _run_single_rpc_attempt(
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
    hop: int,
    pid_file: Path | None,
    stop_check,
    waiting_check: Callable[[], bool] | None,
    append_system_prompt: str | None,
    swap_after_edit: bool,
    todo_already: bool,
    sandbox: str | None,
    env_extra: dict[str, str] | None,
) -> _RpcAttemptResult:
    sessions_dir.mkdir(parents=True, exist_ok=True)
    argv = _build_rpc_cmd(
        model=model,
        session_id=session_id,
        sessions_dir=sessions_dir,
        tools=tools,
        append_system_prompt=append_system_prompt,
    )
    if sandbox:
        logger.info("+ podman exec -i %s %s", sandbox, shlex.join(argv))
    else:
        logger.info("+ %s", shlex.join(argv))
    logger.info("%s hop %d: full prompt:\n%s", log_prefix, hop, message)

    await async_wait_for_rate_limit(model)
    proc = await _launch_rpc_proc(argv, sandbox=sandbox, env=env_extra)

    if pid_file is not None and proc.pid:
        try:
            pid_file.parent.mkdir(parents=True, exist_ok=True)
            pid_file.write_text(str(proc.pid), encoding="utf-8")
            _write_agent_meta(pid_file, sandbox=sandbox, session_id=session_id)
        except OSError as e:
            logger.warning("%s hop %d: could not write pid_file %s: %s",
                           log_prefix, hop, pid_file, e)

    assert proc.stdin is not None
    assert proc.stdout is not None

    stderr_tail: deque[str] = deque(maxlen=STDERR_TAIL_LINES)
    stderr_task = asyncio.create_task(_drain_stderr(proc, stderr_tail))

    prompt_message = message
    pi_failure: PiError | None = None
    infrastructure_failure = False

    try:
        retry_resp = await _rpc_command(
            proc,
            {"type": "set_auto_retry", "enabled": True},
            expect_command="set_auto_retry",
            timeout=15.0,
        )
        if retry_resp is not None and not retry_resp.get("success"):
            err = str(retry_resp.get("error") or retry_resp.get("message") or "set_auto_retry failed")
            pi_failure = PiError("pi rejected set_auto_retry", detail=err)

        if pi_failure is None:
            await _send_line(
                proc,
                {"id": "p1", "type": "prompt", "message": prompt_message},
            )

        acc = _TurnAccumulator()
        reason = "agent_end"
        persisted = 0
        settled = False
        abort_sent = False
        wait_credit = 0.0
        waiting_since: float | None = None
        todo_seen = todo_already
        prompt_answered = pi_failure is not None

        def active_elapsed() -> float:
            now = time.monotonic()
            credit = wait_credit
            if waiting_since is not None:
                credit += now - waiting_since
            return now - start - credit

        async def maybe_abort(new_reason: str) -> None:
            nonlocal reason, abort_sent
            if abort_sent:
                return
            reason = new_reason
            abort_sent = True
            await _send_abort(proc)

        if pi_failure is None:
            assert proc.stdout is not None
            while True:
                line_b = await proc.stdout.readline()
                if not line_b:
                    break
                line = line_b.decode("utf-8", errors="replace").strip()
                if not line:
                    continue
                try:
                    ev = json.loads(line)
                except json.JSONDecodeError:
                    logger.warning("%s hop %d: non-JSON rpc line: %s", log_prefix, hop, line[:200])
                    continue

                etype = ev.get("type")

                if etype == "response":
                    if not prompt_answered and ev.get("id") == "p1":
                        prompt_answered = True
                        if not ev.get("success"):
                            err = str(
                                ev.get("error")
                                or ev.get("message")
                                or "rpc prompt rejected"
                            )
                            pi_failure = PiError("pi rejected the prompt", detail=err)
                            break
                    continue

                if etype == "auto_retry_start":
                    logger.info(
                        "%s hop %d: pi auto-retry start (attempt %s/%s)",
                        log_prefix, hop, ev.get("attempt"), ev.get("maxAttempts"),
                    )
                    continue

                if etype == "auto_retry_end":
                    if ev.get("success"):
                        logger.info("%s hop %d: pi auto-retry recovered", log_prefix, hop)
                        continue
                    final_error = _scrub_nuls(str(ev.get("finalError") or "auto_retry exhausted"))
                    pi_failure = PiError(f"pi gave up: {final_error}", detail=final_error)
                    break

                if etype == "message_update":
                    err_text = _message_update_error(ev)
                    if err_text:
                        pi_failure = PiError(err_text, detail=err_text)
                        break

                if etype == "error":
                    err = _scrub_nuls(str(
                        ev.get("error") or ev.get("message") or ev.get("text") or "error event"
                    ))
                    pi_failure = PiError(err, detail=err)
                    break

                acc.apply(ev)

                if etype == "message_end":
                    msg = ev.get("message")
                    if isinstance(msg, dict) and msg.get("role") == "error":
                        err_text = text_from_content(msg.get("content")) or "error message"
                        err_text = _scrub_nuls(err_text)
                        pi_failure = PiError(err_text, detail=err_text)
                        break

                if sentinel is not None and etype == "message_end":
                    text_lines = acc.current_turn.get("text", "").splitlines()
                    if any(l.strip() == sentinel for l in text_lines):
                        if turn_has_content(acc.current_turn):
                            persist_turn(acc.current_turn, hop)
                            persisted += 1
                        await maybe_abort("sentinel")

                if etype == "turn_end":
                    turn = acc.current_turn
                    if turn_has_content(turn):
                        if swap_after_edit:
                            names = [str(b.get("name", "")) for b in turn.get("tool_blocks", [])]
                            if "todo" in names:
                                todo_seen = True
                            if todo_seen and any(n in ("edit", "write") for n in names):
                                logger.info(
                                    "%s hop %d: first edit after todo — prewalk swap (rpc)",
                                    log_prefix, hop,
                                )
                                persist_turn(turn, hop)
                                persisted += 1
                                await maybe_abort("swap")
                                continue
                        persist_turn(turn, hop)
                        persisted += 1
                        acc = _TurnAccumulator()
                    else:
                        logger.warning("%s hop %d: empty turn (rpc); skipped", log_prefix, hop)

                wait_credit, waiting_since = _tick_wait_credit(
                    waiting_check, wait_credit=wait_credit, waiting_since=waiting_since,
                    log_prefix=log_prefix, hop=hop,
                )

                if stop_check is not None:
                    try:
                        if stop_check():
                            await maybe_abort("stopped")
                    except Exception:
                        logger.exception("%s hop %d: stop_check raised", log_prefix, hop)

                if active_elapsed() > timeout_seconds and reason not in ("stopped", "sentinel", "swap"):
                    await maybe_abort("time_cap")

                if etype == "agent_end" and ev.get("willRetry"):
                    continue

                if etype == "agent_settled":
                    settled = True
                    break

        if pi_failure is None and not settled:
            stopped = reason == "stopped"
            if not stopped and stop_check is not None:
                try:
                    stopped = bool(stop_check())
                except Exception:
                    logger.exception("%s hop %d: stop_check raised", log_prefix, hop)
            if stopped:
                reason = "stopped"
                settled = True
            else:
                detail = _compose_detail("", "\n".join(stderr_tail))
                if not detail.strip():
                    detail = "rpc stream ended without agent_settled"
                pi_failure = PiError("pi rpc process exited unexpectedly", detail=detail)
                infrastructure_failure = True

        if reason == "agent_end" and persisted == 0 and pi_failure is None:
            reason = "empty"

        return _RpcAttemptResult(
            reason=reason,
            settled=settled,
            persisted=persisted,
            pi_failure=pi_failure,
            infrastructure_failure=infrastructure_failure and pi_failure is not None,
            stderr_tail="\n".join(stderr_tail),
        )
    finally:
        stderr_task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await stderr_task
        await _shutdown(
            proc, sandbox=sandbox, session_id=session_id,
            log_prefix=log_prefix, hop=hop,
        )
        if pid_file is not None:
            with contextlib.suppress(OSError):
                pid_file.unlink()
            _unlink_agent_meta(pid_file)


async def arun_agent_hop_rpc(
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
    **_ignored,
) -> str:
    """Run one agent hop over pi's RPC protocol (async).

    Same contract as :func:`crack_server.pi_proc.arun_agent_hop`: one prompt on
    the wire, turns persisted via ``persist_turn``, stop reason returned.
  Genuine pi failures raise :class:`PiError` with the exact provider error in
    ``detail``. Only infrastructure failures (RPC died before ``agent_settled``)
    are retried a few times.
    """
    from crack_server import ratelimit

    budget = error_budget if error_budget is not None else (lambda: ratelimit.MAX_TOTAL_ERRORS)
    attempt_message = message
    if resume_session and attempt_message != RESUME_MESSAGE:
        attempt_message = RESUME_MESSAGE

    if record_prompt is not None:
        try:
            record_prompt({"kind": "user_prompt", "compiled": message, "hop": hop, "at": time.time()})
        except Exception:
            logger.exception("%s hop %d: record_prompt raised", log_prefix, hop)

    last_failure: PiError | None = None
    total_attempts = 0

    for safety_attempt in range(RPC_SAFETY_MAX_ATTEMPTS):
        if safety_attempt > 0:
            await asyncio.sleep(RPC_SAFETY_BACKOFF_SECONDS * safety_attempt)
            attempt_message = RESUME_MESSAGE

        total_attempts += 1
        result = await _run_single_rpc_attempt(
            log_prefix=log_prefix,
            model=model,
            session_id=session_id,
            sessions_dir=sessions_dir,
            tools=tools,
            message=attempt_message,
            start=start,
            sentinel=sentinel,
            timeout_seconds=timeout_seconds,
            persist_turn=persist_turn,
            hop=hop,
            pid_file=pid_file,
            stop_check=stop_check,
            waiting_check=waiting_check,
            append_system_prompt=append_system_prompt,
            swap_after_edit=swap_after_edit,
            todo_already=todo_already,
            sandbox=sandbox,
            env_extra=env_extra,
        )

        if result.pi_failure is None:
            return result.reason

        last_failure = result.pi_failure
        if not result.infrastructure_failure:
            total_errors = _record_attempt_error(record_error, {
                "message": str(result.pi_failure),
                "detail": result.pi_failure.detail,
                "rc": None,
                "attempt": total_attempts,
                "phase": log_prefix,
            }, log_prefix)
            if total_errors >= budget():
                raise PiError(
                    f"{result.pi_failure} after {total_attempts} attempts "
                    f"({total_errors} recorded errors — budget spent)",
                    detail=result.pi_failure.detail,
                    over_budget=True,
                )
            raise result.pi_failure

        total_errors = _record_attempt_error(record_error, {
            "message": str(result.pi_failure),
            "detail": result.pi_failure.detail,
            "rc": None,
            "attempt": total_attempts,
            "phase": log_prefix,
        }, log_prefix)
        if total_errors >= budget():
            raise PiError(
                f"{result.pi_failure} after {total_attempts} attempts "
                f"({total_errors} recorded errors — budget spent)",
                detail=result.pi_failure.detail,
                over_budget=True,
            )
        if safety_attempt + 1 >= RPC_SAFETY_MAX_ATTEMPTS:
            raise result.pi_failure
        logger.warning(
            "%s hop %d: rpc infrastructure failure (attempt %d); safety retry",
            log_prefix, hop, total_attempts,
        )

    assert last_failure is not None
    raise last_failure
