"""`pi` subprocess runners: single-shot text calls and the streaming JSON-mode
agent hop, plus process-group kill support.

Split out of pi_runner.py (A6). Rate limiting and retry scheduling live in
ratelimit.py; turn accumulation lives in transcript.py. Everything here logs
through the uvicorn logger and is only ever called from background threads.
"""

from __future__ import annotations

import json
import logging
import os
import shlex
import signal
import subprocess
import threading
import time
from collections.abc import Callable
from pathlib import Path
from typing import NamedTuple

from crack_server import ratelimit
from crack_server.paths import project_root
from crack_server.ratelimit import (
    PI_RETRY_ATTEMPTS, PI_TIMEOUT_SECONDS, RESUME_MESSAGE, _retry_backoff_sleep,
    _transient_backoff_sleep, is_transient, wait_for_rate_limit,
)
from crack_server.transcript import apply_event_to_turn, turn_has_content

logger = logging.getLogger("uvicorn.error")

# Rolling raw-output buffer size: the last N lines of pi's stdout+stderr are kept
# and surfaced in the error so a failure is diagnosable from the UI, not just logs.
OUTPUT_TAIL_LINES = 10

# Separate ring buffer for non-JSON (stderr-ish) lines in the streaming hop: the
# JSON-event output_tail usually holds only well-formed events, not the stderr
# that explains a crash, so PiError.detail prefers this tail when it's nonempty.
STDERR_TAIL_LINES = 10

# pi auto-discovers `.pi/extensions/` relative to its launch cwd, so we pass our
# extension explicitly with `-e` (existence-checked in _build_cmd, so tests and
# partial checkouts don't break) and pin the subprocess cwd to the project root
# (pi dedupes `-e` against auto-discovery — no double registration).
CRACK_EXT = project_root() / ".pi" / "extensions" / "crack" / "index.ts"


class PiError(RuntimeError):
    """A pi subprocess failure carrying a short message plus a ``detail`` blob
    (the last few lines of captured output — the raw stderr tail when there is
    one, else the JSON-event/stdout tail) for inline UI display."""

    def __init__(self, message: str, detail: str = "") -> None:
        super().__init__(message)
        self.detail = detail


class PiStopped(RuntimeError):
    """The pi subprocess died because of an intentional external STOP
    (``stop_check`` confirmed it) — a clean halt, never an error to record."""


def _tail_text(text: str, n: int = OUTPUT_TAIL_LINES) -> str:
    """Keep the last ``n`` non-empty lines of a captured output blob."""
    lines = [ln for ln in (text or "").splitlines() if ln.strip()]
    return "\n".join(lines[-n:])


def _compose_detail(output_tail: str, stderr_tail: str) -> str:
    """Build a PiError detail blob: prefer the raw stderr tail (what usually
    explains the crash), fall back to the JSON-event/stdout tail otherwise.
    Each blob is labeled so the UI shows which tail it is."""
    if stderr_tail.strip():
        return "last stderr:\n" + stderr_tail
    if output_tail.strip():
        return "last output:\n" + output_tail
    return ""


def run_pi_text(
    prompt: str,
    log_prefix: str,
    model: str,
    max_input_chars: int | None = None,
    record_prompt=None,
    pid_file: Path | None = None,
    stop_check: Callable[[], bool] | None = None,
) -> tuple[str, float]:
    """Run `pi` non-interactively with a single text prompt.

    Returns ``(text, elapsed_seconds)``. Logs the full prompt, exact command
    line, timeout, elapsed time, and an output summary so failures are
    diagnosable from server logs alone. Raises RuntimeError because this helper
    is only used from background threads, where HTTPException has no request
    context to turn into.

    ``record_prompt`` (optional, called once per logical call, not per retry)
    receives ``{"kind": "user_prompt", "compiled": prompt, "at": ...}`` so
    callers can persist the exact compiled prompt into their trajectory.

    ``pid_file`` / ``stop_check`` (optional, RC5): the subprocess runs in its
    own session with its group-leader pid published to ``pid_file`` so an
    external STOP can kill it, exactly like ``run_agent_hop``. When a failed
    attempt coincides with ``stop_check()`` being truthy, :class:`PiStopped`
    is raised instead of retrying — callers treat it as a clean halt.
    """
    if max_input_chars is not None and len(prompt) > max_input_chars:
        logger.info("%s: truncating prompt from %d to %d chars", log_prefix, len(prompt), max_input_chars)
        prompt = prompt[:max_input_chars]

    cmd = ["pi", "--model", model, "--print", "--no-session", "--no-tools", prompt]

    logger.info("%s: full prompt:\n%s", log_prefix, prompt)
    logger.info("%s: timeout=%ss", log_prefix, PI_TIMEOUT_SECONDS)
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
                _transient_backoff_sleep(transient_reattempts)
                transient_reattempts += 1
            else:
                _retry_backoff_sleep(attempt, first_attempt_at)

        wait_for_rate_limit(model)
        start = time.monotonic()
        try:
            result = _run_text_attempt(cmd, log_prefix, pid_file)
        except subprocess.TimeoutExpired as e:
            elapsed = time.monotonic() - start
            logger.error("%s: pi timed out after %.2fs (attempt %d)", log_prefix, elapsed, attempt + 1)
            last_error = "pi command timed out"
            last_detail = _compose_detail(_tail_text(e.stdout or ""), _tail_text(e.stderr or ""))
            last_transient = is_transient(last_detail)
            continue
        except FileNotFoundError:
            elapsed = time.monotonic() - start
            logger.error("%s: pi command not found on PATH (after %.2fs)", log_prefix, elapsed)
            last_error = "pi command not found"
            last_detail = ""
            last_transient = False
            continue

        elapsed = time.monotonic() - start
        logger.info("%s: pi exited %d in %.2fs (attempt %d/%d)",
                    log_prefix, result.returncode, elapsed, attempt + 1, PI_RETRY_ATTEMPTS)

        if result.returncode == 0:
            text = result.stdout.strip()
            logger.info("%s: output summary: %r", log_prefix, text[:200])
            return text, elapsed

        # A STOP kills the process group out from under us and looks like a
        # crash; when the caller confirms a stop was requested, halt cleanly
        # instead of retrying (RC5).
        if stop_check is not None and stop_check():
            raise PiStopped(f"pi run stopped by user (rc={result.returncode})")

        detail = _compose_detail(_tail_text(result.stdout or ""), _tail_text(result.stderr or ""))
        logger.error("%s: pi exited %d; last output:\n%s", log_prefix, result.returncode, detail)
        last_error = f"pi exited {result.returncode}"
        last_detail = detail
        last_transient = is_transient(detail)

    raise PiError(f"{last_error} after {PI_RETRY_ATTEMPTS} attempts", detail=last_detail)


def _run_text_attempt(
    cmd: list[str], log_prefix: str, pid_file: Path | None
) -> subprocess.CompletedProcess:
    """One run_pi_text attempt via Popen so the group-leader pid can be
    published to ``pid_file`` for the whole call (kill_pid_file kills the
    group). Mirrors ``subprocess.run(capture_output=True, timeout=...)``."""
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, start_new_session=True,
                            cwd=str(project_root()))
    if pid_file is not None:
        try:
            pid_file.parent.mkdir(parents=True, exist_ok=True)
            pid_file.write_text(str(proc.pid), encoding="utf-8")
        except OSError as e:
            logger.warning("%s: could not write pid_file %s: %s", log_prefix, pid_file, e)
    try:
        stdout, stderr = proc.communicate(timeout=PI_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout, stderr = proc.communicate()
        raise subprocess.TimeoutExpired(cmd, PI_TIMEOUT_SECONDS, output=stdout, stderr=stderr)
    finally:
        if pid_file is not None:
            try:
                pid_file.unlink()
            except OSError:
                pass
    return subprocess.CompletedProcess(cmd, proc.returncode, stdout, stderr)


def kill_pid_file(pid_file: Path) -> bool:
    """Kill the process group named in ``pid_file`` (written by run_agent_hop).

    Sends SIGTERM to the whole group (pi + any children it spawned), then
    SIGKILL as a fallback. Returns True if a signal was delivered. Safe to call
    when the file is missing or the process is already gone."""
    try:
        pid = int(pid_file.read_text(encoding="utf-8").strip())
    except (OSError, ValueError):
        return False
    try:
        pgid = os.getpgid(pid)
    except ProcessLookupError:
        return False
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            return True
        # Give SIGTERM a brief moment before escalating to SIGKILL.
        if sig == signal.SIGTERM:
            for _ in range(20):
                try:
                    os.killpg(pgid, 0)
                except ProcessLookupError:
                    return True
                time.sleep(0.1)
    return True


class _TurnAccumulator:
    """The in-progress turn dict plus the monotonic timing state the pure
    apply_event_to_turn does not track: start of the current turn and of each
    in-flight toolCall id, so turn_end / toolResult events can attach elapsed
    seconds to the turn dict."""

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


class _StreamSink:
    """Per-attempt stream state: the rolling raw-output tail, a separate ring
    buffer for non-JSON (stderr-ish) lines, the turn accumulator, and the
    stop-reason bookkeeping _stream_events fills in."""

    def __init__(self, p: _HopParams) -> None:
        self.p = p
        self.acc = _TurnAccumulator()
        self.output_tail: list[str] = []
        self.stderr_tail: list[str] = []
        self.reason = "agent_end"
        self.terminated_by_us = False
        self.persisted = 0

    def persist(self, turn: dict) -> None:
        self.persisted += 1
        self.p.persist_turn(turn, self.p.hop)


def _stream_events(proc: subprocess.Popen, sink: _StreamSink) -> None:
    """Consume pi's JSON event stream until the hop ends (sentinel, time cap,
    agent_end, or EOF), filling sink.reason / sink.terminated_by_us."""
    p = sink.p
    for line in proc.stdout or []:
        line = line.strip()
        if not line:
            continue
        # Keep a rolling tail of the raw output (JSON or not) so a crash is
        # diagnosable inline in the UI, not just from server logs.
        sink.output_tail.append(line[:500])
        del sink.output_tail[:-OUTPUT_TAIL_LINES]

        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            # Keep the full raw line in its own stderr tail: this is usually
            # what explains a crash, and it would otherwise survive only as a
            # truncated WARN log line.
            sink.stderr_tail.append(line[:500])
            del sink.stderr_tail[:-STDERR_TAIL_LINES]
            logger.warning("%s hop %d: non-JSON line: %s", p.log_prefix, p.hop, line[:200])
            continue

        sink.acc.apply(event)
        etype = event.get("type")

        text_lines = sink.acc.current_turn.get("text", "").splitlines()
        if (p.sentinel is not None and etype == "message_end"
                and any(l.strip() == p.sentinel for l in text_lines)):
            if turn_has_content(sink.acc.current_turn):
                sink.persist(sink.acc.current_turn)
            logger.info("%s hop %d: sentinel %s received", p.log_prefix, p.hop, p.sentinel)
            sink.reason = "sentinel"
            sink.terminated_by_us = True
            proc.terminate()
            break

        if etype == "turn_end":
            if not turn_has_content(sink.acc.current_turn):
                # Content-less turns (empty model responses) are noise:
                # never persist them.
                logger.warning("%s hop %d: empty turn (no text/thinking/tool blocks); skipped",
                               p.log_prefix, p.hop)
                continue
            sink.persist(sink.acc.current_turn)
            logger.info("%s hop %d: completed turn (persisted %d this attempt)",
                        p.log_prefix, p.hop, sink.persisted)

        if time.monotonic() - p.start > p.timeout_seconds:
            if turn_has_content(sink.acc.current_turn) and etype != "turn_end":
                sink.persist(sink.acc.current_turn)
            sink.reason = "time_cap"
            sink.terminated_by_us = True
            proc.terminate()
            break

        if etype in ("agent_end", "agent_settled"):
            break


class _HopParams(NamedTuple):
    log_prefix: str
    model: str
    session_id: str
    sessions_dir: Path
    tools: str | None
    start: float
    sentinel: str | None
    timeout_seconds: int
    persist_turn: Callable[[dict, int], None]
    hop: int
    pid_file: Path | None
    stop_check: Callable[[], bool] | None
    env_extra: dict[str, str] | None


def _build_cmd(p: _HopParams, msg: str) -> list[str]:
    cmd = ["pi", "--mode", "json", "-p", "--model", p.model]
    if CRACK_EXT.exists():
        cmd += ["-e", str(CRACK_EXT)]
    if p.tools is not None:
        cmd += ["--tools", p.tools]
    cmd += ["--session-id", p.session_id, "--session-dir", str(p.sessions_dir), msg]
    return cmd


def _attempt_once(p: _HopParams, attempt_idx: int, attempt_message: str) -> dict:
    """Run one pi subprocess: stream, persist completed turns, and report how it
    ended. ``persisted`` counts turns committed to disk this attempt so the retry
    loop can distinguish resuming from replaying."""
    cmd = _build_cmd(p, attempt_message)
    logger.info("%s hop %d: full prompt:\n%s", p.log_prefix, p.hop, attempt_message)
    logger.info("+ %s", shlex.join(cmd))

    wait_for_rate_limit(p.model)
    # start_new_session=True puts pi in its own process group so an external
    # STOP can kill pi *and* any children it spawned (npx MCP servers) via
    # the group leader's pid, which we publish to pid_file.
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, start_new_session=True,
                            cwd=str(project_root()),
                            env={**os.environ, **(p.env_extra or {})})
    # Hard watchdog: a pi that hangs without emitting output would wedge the
    # stream loop forever; kill it well past the stage time cap.
    watchdog = threading.Timer(p.timeout_seconds + 60, proc.kill)
    watchdog.daemon = True
    watchdog.start()
    if p.pid_file is not None:
        try:
            p.pid_file.parent.mkdir(parents=True, exist_ok=True)
            p.pid_file.write_text(str(proc.pid), encoding="utf-8")
        except OSError as e:
            logger.warning("%s hop %d: could not write pid_file %s: %s",
                           p.log_prefix, p.hop, p.pid_file, e)
    sink = _StreamSink(p)
    try:
        _stream_events(proc, sink)
    finally:
        watchdog.cancel()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        if p.pid_file is not None:
            try:
                p.pid_file.unlink()
            except OSError:
                pass

    # An external STOP kills the subprocess out from under the stream loop,
    # which looks like a crash (non-zero rc). If the caller confirms a stop
    # was requested, classify it as an intentional, clean halt.
    if not sink.terminated_by_us and p.stop_check is not None:
        try:
            if p.stop_check():
                sink.reason = "stopped"
                sink.terminated_by_us = True
        except Exception:
            logger.exception("%s hop %d: stop_check raised", p.log_prefix, p.hop)

    elapsed = time.monotonic() - p.start
    logger.info("%s hop %d: attempt %d finished reason=%s persisted=%d total_elapsed=%.2fs rc=%s",
                p.log_prefix, p.hop, attempt_idx + 1, sink.reason, sink.persisted, elapsed,
                proc.returncode)
    return {
        "reason": sink.reason,
        "terminated_by_us": sink.terminated_by_us,
        "returncode": proc.returncode,
        "persisted": sink.persisted,
        "detail": _compose_detail("\n".join(sink.output_tail), "\n".join(sink.stderr_tail)),
    }


def _run_hop_with_retries(p: _HopParams, message: str) -> str:
    """Drive _attempt_once until the hop stops cleanly or the retry schedules are
    exhausted, then either return the stop reason or raise PiError."""
    first_attempt_at = time.monotonic()
    attempt_message = message
    last_detail = ""
    last_rc: int | None = None
    hard_attempts = 0       # hard failures + empty runs, on the 4/61s schedule
    transient_reattempts = 0  # transient failures, on the TRANSIENT_RETRY_DELAYS schedule
    total_attempts = 0

    while True:
        res = _attempt_once(p, total_attempts, attempt_message)
        total_attempts += 1

        failed = not res["terminated_by_us"] and res["returncode"] not in (0, None)
        if not failed:
            # A clean exit that persisted no real turns means the model returned
            # only content-less responses: retry, then surface "empty" so callers
            # fail the stage instead of treating silence as success.
            if res["persisted"] > 0 or res["reason"] != "agent_end":
                return res["reason"]
            hard_attempts += 1
            if hard_attempts >= PI_RETRY_ATTEMPTS:
                logger.error("%s hop %d: pi returned only empty turns after %d attempts",
                             p.log_prefix, p.hop, total_attempts)
                return "empty"
            logger.warning("%s hop %d: pi returned only empty turns; retrying", p.log_prefix, p.hop)
            _retry_backoff_sleep(hard_attempts, first_attempt_at)
            continue

        last_detail = res["detail"]
        last_rc = res["returncode"]
        if res["persisted"] > 0:
            # Turns are already committed and the session dir kept them: every
            # further attempt must resume the session, never replay the prompt.
            attempt_message = RESUME_MESSAGE

        if is_transient(last_detail):
            if transient_reattempts < len(ratelimit.TRANSIENT_RETRY_DELAYS):
                logger.warning("%s hop %d: transient failure (rc=%s); will resume (reattempt %d/%d)",
                               p.log_prefix, p.hop, last_rc,
                               transient_reattempts + 1, len(ratelimit.TRANSIENT_RETRY_DELAYS))
                _transient_backoff_sleep(transient_reattempts)
                transient_reattempts += 1
                continue
            break

        # Hard failure. Partial progress (turns already committed to disk) is
        # real work: don't replay it — raise so the stage records the error and
        # a later retry/resume continues the session.
        if res["persisted"] > 0:
            logger.info("%s hop %d: pi exited %s after %d persisted turn(s); not retrying",
                        p.log_prefix, p.hop, last_rc, res["persisted"])
            break
        hard_attempts += 1
        if hard_attempts >= PI_RETRY_ATTEMPTS:
            break
        _retry_backoff_sleep(hard_attempts, first_attempt_at)

    raise PiError(f"pi exited {last_rc} after {total_attempts} attempts", detail=last_detail)


def run_agent_hop(
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
    env_extra: dict[str, str] | None = None,
) -> str:
    """Run one hop of a tool-using agent and stream its JSON events.

    Parameterized on model / session / tools so multiple stages can share it.
    ``tools=None`` omits ``--tools`` so pi runs with every tool it has. A hop
    has no turn cap: it ends only when the model stops on its own, on the
    sentinel (matched *on its own line* in assistant text), on the time cap, on
    an external stop, or on an unrecoverable error. The pi session is persisted
    under ``sessions_dir`` so the next hop/step resumes it via the same
    --session-id. ``persist_turn(current_turn, hop)`` is called for every
    completed non-empty turn. Returns the stop reason: "sentinel", "time_cap",
    "agent_end", "empty", or "stopped".

    ``pid_file`` (optional): the pi subprocess is started in its own session and
    its PID written here so another process (e.g. a web STOP handler) can kill
    the whole process group. ``stop_check`` (optional, called with no args): when
    it returns truthy after the subprocess ends, the hop is classified as
    "stopped" (a clean, intentional halt) rather than a crash to retry.

    ``record_prompt`` (optional, called once per hop call, not per retry
    attempt) receives ``{"kind": "user_prompt", "compiled": message, "hop": hop,
    "at": ...}`` so callers can persist the exact compiled prompt into their
    trajectory alongside the turns it produced.

    Retries: hard process failures with no persisted turns follow the standard
    4-attempt/61s schedule, replaying ``message``. *Transient* upstream
    failures (see ``is_transient``) follow their own longer backoff and retry
    even after turns were persisted — the preserved session is resumed with
    ``RESUME_MESSAGE`` instead of replaying, so no work is duplicated. A hard
    failure after persisted turns raises immediately.
    """
    p = _HopParams(log_prefix=log_prefix, model=model, session_id=session_id,
                   sessions_dir=sessions_dir, tools=tools, start=start, sentinel=sentinel,
                   timeout_seconds=timeout_seconds, persist_turn=persist_turn, hop=hop,
                   pid_file=pid_file, stop_check=stop_check, env_extra=env_extra)
    p.sessions_dir.mkdir(parents=True, exist_ok=True)

    if record_prompt is not None:
        try:
            record_prompt({"kind": "user_prompt", "compiled": message, "hop": hop, "at": time.time()})
        except Exception:
            logger.exception("%s hop %d: record_prompt raised", log_prefix, hop)

    return _run_hop_with_retries(p, message)
