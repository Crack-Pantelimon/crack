"""`pi` subprocess runners: single-shot text calls and the streaming JSON-mode
agent hop, plus process-group kill support.

Split out of pi_runner.py (A6). Rate limiting and retry scheduling live in
ratelimit.py; turn accumulation lives in transcript.py.

The implementation is fully async (``arun_*``): subprocesses are awaited via
``asyncio.create_subprocess_exec`` so a waiting hop costs a coroutine, not a
thread. Thin sync wrappers (``run_pi_text`` / ``run_agent_hop``) preserve the
old API for callers that still run on threads (stage jobs dispatched via
``asyncio.to_thread``, tests) — they must not be called from inside a running
event loop. Everything here logs through the uvicorn logger.

Reload survival (detached hops): an agent hop's pi writes its JSON event
stream to an append-only hop.jsonl file (not a pipe) and publishes a hop.json
manifest (pid, session, consumed offset) next to the pid file. A server
reload never kills pi — the worker detaches and the restarted worker's next
hop call re-attaches (tailing from the stored offset) instead of spawning a
second pi for the same session. One-off ``arun_pi_text`` calls stay
pipe-based (short-lived, idempotent on re-pickup).
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
from typing import NamedTuple

from crack_server import ratelimit
from crack_server import paths as paths_mod
from crack_server.paths import project_root
from crack_server.ratelimit import (
    PI_RETRY_ATTEMPTS, PI_TIMEOUT_SECONDS, RESUME_MESSAGE,
    _async_hard_backoff_sleep, _async_retry_backoff_sleep,
    _async_transient_backoff_sleep,
    async_wait_for_rate_limit, is_transient,
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

# asyncio's StreamReader defaults to a 64 KiB line limit; pi JSON event lines
# (tool results embedded in message_end) can exceed that, and the old sync
# reader was unbounded. 16 MiB is a pragmatic ceiling.
STREAM_LINE_LIMIT = 16 * 1024 * 1024

# pi auto-discovers `.pi/extensions/` relative to its launch cwd, so we pass our
# extension explicitly with `-e` (existence-checked in _build_cmd, so tests and
# partial checkouts don't break) and pin the subprocess cwd to the project root
# (pi dedupes `-e` against auto-discovery — no double registration).
CRACK_EXT = project_root() / ".pi" / "extensions" / "crack" / "index.ts"
# Shared tool-usage guidance appended to the system prompt of every tool-bearing
# hop (unscripted chats + all sub-agents, plan and non-plan). It teaches weak
# models the exact JSON shapes for the hash-anchored read/edit/write/grep tools.
# Only the tool hops load it — the title/vision one-off runs go through
# `arun_pi_text` with `--no-tools` and never touch `_build_cmd`, so they skip it.
CRACK_SYSTEM_MD = project_root() / ".pi" / "SYSTEM.md"


class PiError(RuntimeError):
    """A pi subprocess failure carrying a short message plus a ``detail`` blob
    (the last few lines of captured output — the raw stderr tail when there is
    one, else the JSON-event/stdout tail) for inline UI display. ``over_budget``
    marks a failure caused by the durable error budget (MAX_TOTAL_ERRORS) being
    spent, so stages can show the "something is likely wrong" banner."""

    def __init__(self, message: str, detail: str = "", over_budget: bool = False) -> None:
        super().__init__(message)
        self.detail = detail
        self.over_budget = over_budget


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


def _record_attempt_error(record_error, entry: dict, log_prefix: str) -> int:
    """Best-effort durable error-row record for one failed attempt.

    Returns the total error count reported by the recorder; a missing or
    broken recorder never wedges retries (0 = no budget signal)."""
    if record_error is None:
        return 0
    try:
        return int(record_error(entry))
    except Exception:
        logger.exception("%s: record_error raised", log_prefix)
        return 0


# ---------------------------------------------------------------------------
# Detached-hop machinery (reload survival): pi's stdout+stderr is redirected
# to an append-only hop.jsonl file and a hop.json manifest tracks the pid and
# the consumed byte offset, so a pi started before a server reload keeps
# running (tini reaps it) and the restarted worker re-attaches instead of
# spawning a second pi for the same session.
# ---------------------------------------------------------------------------

# Idle-poll cadence of the file-tailing stream loop.
TAIL_POLL_SECONDS = 0.2

# After a hop's event stream ends, wait this long for the pi *process* to exit
# before deciding what to do. A terminal event (agent_end / sentinel / time_cap)
# means pi is only lingering on MCP teardown — never SIGKILL it. A non-terminal
# hang still gets killed after this grace (see ``_attempt_once`` finally).
EXIT_GRACE_SECONDS = 8

# Grace-period detaches (terminal event but pi still alive) are tracked in the
# hop manifest under ``detached_pids``. The next attempt sweeps that ledger:
# SIGTERM once a detach is this old, then SIGKILL after a further wait. Generous
# enough that legitimate MCP teardown (~8–21s) is never cut short, but no
# process lingers forever across retries.
DETACHED_TERMINATE_AFTER_SECONDS = 60
DETACHED_KILL_AFTER_SECONDS = 30

# The manifest keeps the attempt message for debugging; compiled prompts can
# be huge, so it is truncated.
MANIFEST_MESSAGE_MAX_CHARS = 4000


def _hop_paths(p: "_HopParams") -> tuple[Path, Path]:
    """(manifest, output) paths for the detached-hop machinery: derived from
    the pid file when one is given (all production callers), else from the
    sessions dir (pid-less test callers)."""
    if p.pid_file is not None:
        return paths_mod.hop_manifest_path(p.pid_file), paths_mod.hop_output_path(p.pid_file)
    base = p.sessions_dir.parent / p.sessions_dir.name
    return Path(str(base) + ".hop.json"), Path(str(base) + ".hop.jsonl")


def _read_hop_manifest(path: Path) -> dict:
    """Tolerant manifest read: {} when missing or unparseable."""
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def _write_hop_manifest(path: Path, data: dict) -> None:
    """Atomic whole-manifest write (tmp + replace); failures only log."""
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_name(path.name + ".tmp")
        tmp.write_text(json.dumps(data, indent=2), encoding="utf-8")
        os.replace(tmp, path)
    except OSError as e:
        logger.warning("hop manifest write failed %s: %s", path, e)


def _pid_alive(pid: int, session_id: str | None = None) -> bool:
    """Liveness for a possibly-detached pi: ``os.kill(pid, 0)`` plus a
    /proc cmdline identity check — the cmdline contains the ``--session-id``
    argv, which guards against pid reuse and weeds out zombies (whose
    cmdline reads empty)."""
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    try:
        raw = Path(f"/proc/{pid}/cmdline").read_bytes()
    except FileNotFoundError:
        return False
    except OSError:
        return True  # unreadable here: fall back to the kill check alone
    cmdline = raw.replace(b"\x00", b" ").decode("utf-8", "replace")
    if not cmdline.strip():
        return False  # zombie
    return not session_id or session_id in cmdline


def _terminate_group(pid: int, sig: int) -> None:
    """Signal a detached pi's whole process group (spawned start_new_session,
    so pgid == pid), falling back to the bare pid."""
    try:
        os.killpg(os.getpgid(pid), sig)
    except (ProcessLookupError, PermissionError, OSError):
        with contextlib.suppress(ProcessLookupError, OSError):
            os.kill(pid, sig)


def _sweep_detached_pids(
    entries: list,
    session_id: str | None,
    log_prefix: str,
    hop: int,
) -> list[dict]:
    """Drop dead detached pids; SIGTERM / SIGKILL those past the age thresholds.

    Each entry is ``{"pid", "since", "sigterm_at"?}``. Survivors (still alive
    and not yet SIGKILL'd) are returned for the next manifest write. Logs one
    warning summary when any survive — the visibility hook for "are we rate
    limited by our own leaked processes?".
    """
    now = time.time()
    survivors: list[dict] = []
    for raw in entries or []:
        if not isinstance(raw, dict):
            continue
        pid = raw.get("pid")
        since = raw.get("since")
        if not isinstance(pid, int) or not isinstance(since, (int, float)):
            continue
        if not _pid_alive(pid, session_id):
            continue
        entry = dict(raw)
        sigterm_at = entry.get("sigterm_at")
        if isinstance(sigterm_at, (int, float)):
            if now - sigterm_at >= DETACHED_KILL_AFTER_SECONDS:
                _terminate_group(pid, signal.SIGKILL)
                continue
            survivors.append(entry)
            continue
        if now - since >= DETACHED_TERMINATE_AFTER_SECONDS:
            _terminate_group(pid, signal.SIGTERM)
            entry["sigterm_at"] = now
            survivors.append(entry)
            continue
        survivors.append(entry)

    if survivors:
        oldest = max(now - float(e["since"]) for e in survivors)
        logger.warning(
            "%s hop %d: %d detached pi still alive (pids=%s, oldest=%.0fs)",
            log_prefix, hop, len(survivors),
            [e["pid"] for e in survivors], oldest,
        )
    return survivors


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
) -> tuple[str, float]:
    """Run `pi` non-interactively with a single text prompt (async).

    Returns ``(text, elapsed_seconds)``. Logs the full prompt, exact command
    line, timeout, elapsed time, and an output summary so failures are
    diagnosable from server logs alone. Raises RuntimeError: callers run in
    worker tasks, where HTTPException has no request context to turn into.

    ``record_prompt`` (optional, called once per logical call, not per retry)
    receives ``{"kind": "user_prompt", "compiled": prompt, "at": ...}`` so
    callers can persist the exact compiled prompt into their trajectory.

    ``record_error`` (optional) is called once per *failed attempt* with
    ``{"message", "detail", "rc", "attempt", "phase"}`` so callers can persist
    durable error rows; the retry schedule itself is unchanged by it.

    ``pid_file`` / ``stop_check`` (optional, RC5): the subprocess runs in its
    own session with its group-leader pid published to ``pid_file`` so an
    external STOP can kill it, exactly like ``arun_agent_hop``. When a failed
    attempt coincides with ``stop_check()`` being truthy, :class:`PiStopped`
    is raised instead of retrying — callers treat it as a clean halt.

    ``image_paths`` (optional): each path is passed as a ``@<path>`` arg before
    the prompt (mirrors ``pi @img.png "prompt"``), attaching the image to the
    one-off call. Callers must validate the paths beforehand.
    """
    if max_input_chars is not None and len(prompt) > max_input_chars:
        logger.info("%s: truncating prompt from %d to %d chars", log_prefix, len(prompt), max_input_chars)
        prompt = prompt[:max_input_chars]

    cmd = ["pi", "--model", model, "--print", "--no-session", "--no-tools"]
    cmd += [f"@{p}" for p in (image_paths or [])]
    cmd += [prompt]

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
                await _async_transient_backoff_sleep(transient_reattempts)
                transient_reattempts += 1
            else:
                await _async_retry_backoff_sleep(attempt, first_attempt_at)

        await async_wait_for_rate_limit(model)
        start = time.monotonic()
        try:
            result = await _arun_text_attempt(cmd, log_prefix, pid_file)
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
        _record_attempt_error(record_error, {
            "message": last_error, "detail": detail, "rc": result.returncode,
            "attempt": attempt + 1, "phase": log_prefix,
        }, log_prefix)

    raise PiError(f"{last_error} after {PI_RETRY_ATTEMPTS} attempts", detail=last_detail)


def run_pi_text(*args, **kwargs) -> tuple[str, float]:
    """Sync wrapper over :func:`arun_pi_text` for thread-based callers.

    Must NOT be called from inside a running event loop (asyncio.run)."""
    return asyncio.run(arun_pi_text(*args, **kwargs))


async def _arun_text_attempt(
    cmd: list[str], log_prefix: str, pid_file: Path | None
) -> subprocess.CompletedProcess:
    """One arun_pi_text attempt so the group-leader pid can be published to
    ``pid_file`` for the whole call (kill_pid_file kills the group). Mirrors
    ``subprocess.run(capture_output=True, timeout=...)``."""
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
            try:
                pid_file.unlink()
            except OSError:
                pass
    return subprocess.CompletedProcess(
        cmd, proc.returncode,
        (stdout_b or b"").decode("utf-8", "replace"),
        (stderr_b or b"").decode("utf-8", "replace"),
    )


def _kill_process_group(proc: asyncio.subprocess.Process) -> None:
    """Best-effort SIGKILL of the subprocess's whole process group."""
    try:
        os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
    except (ProcessLookupError, PermissionError, OSError):
        with contextlib.suppress(ProcessLookupError, OSError):
            proc.kill()


def kill_pid_file(pid_file: Path) -> bool:
    """Kill the process group named in ``pid_file`` (written by arun_agent_hop).

    Sends SIGTERM to the whole group (pi + any children it spawned), then
    SIGKILL as a fallback. Also SIGKILL any still-alive ``detached_pids`` in
    the sibling hop manifest — an explicit STOP is the moment aggressive
    cleanup is safe (no MCP-teardown ambiguity). Returns True if a signal was
    delivered. Safe to call when the file is missing or the process is already
    gone."""
    delivered = False
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
                # Give SIGTERM a brief moment before escalating to SIGKILL.
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

    # Explicit STOP: also reap grace-period-detached pids from prior attempts.
    manifest = _read_hop_manifest(paths_mod.hop_manifest_path(pid_file))
    for entry in manifest.get("detached_pids") or []:
        if not isinstance(entry, dict):
            continue
        dpid = entry.get("pid")
        if not isinstance(dpid, int):
            continue
        if not _pid_alive(dpid):
            continue
        _terminate_group(dpid, signal.SIGKILL)
        delivered = True
    return delivered


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
    stop-reason bookkeeping the tail loop fills in.

    ``wait_credit`` / ``waiting_since`` track how long the hop's agent has been
    suspended in a server-side wait (``waiting_check`` true, e.g. a blocking
    wait_join): both the in-stream time cap and the hard watchdog bill *active*
    time only, so a token-free wait never times the hop out.

    ``offset`` is the byte position in the hop output file consumed so far;
    ``persist_offset`` is the offset just past the last persisted turn (always
    a turn boundary, so a re-attach replays at most one partial turn). When
    ``manifest_path`` is set, every persisted turn also flushes the offset to
    the on-disk hop manifest — that is what lets a restarted worker resume
    consuming exactly where the killed one stopped."""

    def __init__(self, p: _HopParams) -> None:
        self.p = p
        self.acc = _TurnAccumulator()
        self.output_tail: list[str] = []
        self.stderr_tail: list[str] = []
        self.reason = "agent_end"
        self.terminated_by_us = False
        self.terminal = False
        # Set when pi emits auto_retry_end with success=false: the attempt's
        # last turn died to an unrecoverable internal error (e.g. exhausted
        # 429 retries) even if earlier turns in the same attempt persisted.
        self.ended_in_error: str | None = None
        self.persisted = 0
        self.wait_credit = 0.0
        self.waiting_since: float | None = None
        self.offset = 0
        self.persist_offset = 0
        self.manifest_path: Path | None = None
        self.manifest: dict = {}
        # Prewalk swap watch: seeded from run state (a todo list may already
        # exist from an earlier hop), flipped true when this hop sees a todo.
        self.todo_seen = p.todo_already

    def persist(self, turn: dict) -> None:
        self.persisted += 1
        self.p.persist_turn(turn, self.p.hop)
        self.persist_offset = self.offset
        if self.manifest_path is not None:
            self.manifest.update({"offset": self.offset, "status": "running"})
            _write_hop_manifest(self.manifest_path, self.manifest)


def _tick_wait_credit(p: _HopParams, sink: _StreamSink) -> None:
    """Fold the current waiting_check() state into sink's wait-credit ledger."""
    waiting = False
    if p.waiting_check is not None:
        try:
            waiting = bool(p.waiting_check())
        except Exception:
            logger.exception("%s hop %d: waiting_check raised", p.log_prefix, p.hop)
    now = time.monotonic()
    if waiting and sink.waiting_since is None:
        sink.waiting_since = now
        logger.info("%s hop %d: agent is waiting (server-side); timeout clock suspended",
                    p.log_prefix, p.hop)
    elif not waiting and sink.waiting_since is not None:
        sink.wait_credit += now - sink.waiting_since
        sink.waiting_since = None
        logger.info("%s hop %d: wait ended; timeout clock resumed (credit %.1fs)",
                    p.log_prefix, p.hop, sink.wait_credit)


def _active_elapsed(p: _HopParams, sink: _StreamSink) -> float:
    """Monotonic seconds since hop start, minus time spent server-side waiting."""
    now = time.monotonic()
    credit = sink.wait_credit
    if sink.waiting_since is not None:
        credit += now - sink.waiting_since
    return now - p.start - credit


def _process_stream_line(sink: _StreamSink, line: str, terminate: Callable[[], None]) -> bool:
    """One hop-output line: tail buffers, event accumulation, turn persistence,
    and the terminal checks (sentinel / time cap / agent_end). Returns True
    when the hop is over."""
    p = sink.p
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
        return False

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
        sink.terminal = True
        terminate()
        return True

    if etype == "turn_end":
        if not turn_has_content(sink.acc.current_turn):
            # Content-less turns (empty model responses) are noise:
            # never persist them.
            logger.warning("%s hop %d: empty turn (no text/thinking/tool blocks); skipped",
                           p.log_prefix, p.hop)
            return False
        sink.persist(sink.acc.current_turn)
        logger.info("%s hop %d: completed turn (persisted %d this attempt)",
                    p.log_prefix, p.hop, sink.persisted)

    # Prewalk swap point: during a planning hop, end the moment a turn lands its
    # first edit/write *after* a todo list exists. The frontier model has shown
    # the pattern once, in place; the caller resumes the same session on the
    # cheap model. bash edits deliberately don't count (prompt-forbidden).
    if p.swap_after_edit and etype == "turn_end":
        names = [str(b.get("name", "")) for b in sink.acc.current_turn.get("tool_blocks", [])]
        if "todo" in names:
            sink.todo_seen = True
        if sink.todo_seen and any(n in ("edit", "write") for n in names):
            logger.info("%s hop %d: first edit after todo — prewalk swap", p.log_prefix, p.hop)
            sink.reason = "swap"
            sink.terminated_by_us = True
            sink.terminal = True
            terminate()
            return True

    _tick_wait_credit(p, sink)
    if _active_elapsed(p, sink) > p.timeout_seconds:
        if turn_has_content(sink.acc.current_turn) and etype != "turn_end":
            sink.persist(sink.acc.current_turn)
        sink.reason = "time_cap"
        sink.terminated_by_us = True
        sink.terminal = True
        terminate()
        return True

    # pi's own "I gave up retrying" signal (distinct from per-attempt
    # agent_end errorMessage). More precise than inferring from stopReason.
    if etype == "auto_retry_end" and not event.get("success"):
        sink.ended_in_error = event.get("finalError") or "auto_retry exhausted"

    if etype == "agent_end" and event.get("willRetry"):
        # pi is continuing its own internal agent loop (auto-retry,
        # multi-phase orchestration, etc.) — not a process-exit signal.
        # Keep tailing; only a willRetry:false agent_end or agent_settled
        # actually ends the hop.
        return False
    if etype in ("agent_end", "agent_settled"):
        sink.terminal = True
        return True
    return False


async def _tail_events(
    sink: _StreamSink,
    output_path: Path,
    *,
    proc: asyncio.subprocess.Process | None,
    pid: int,
) -> None:
    """Tail pi's stdout file from ``sink.offset`` until the hop ends (sentinel,
    time cap, agent_end) or the pid disappears. The same routine serves a
    freshly-spawned pi (``proc`` given) and a re-attached one (``proc=None``,
    liveness via the pid). The active-time watchdog is folded in: a pi that
    hangs without emitting output gets its process group killed well past the
    stage time cap."""
    p = sink.p

    def terminate() -> None:
        if proc is not None:
            with contextlib.suppress(ProcessLookupError):
                proc.terminate()
        else:
            _terminate_group(pid, signal.SIGTERM)

    def dead() -> bool:
        if proc is not None:
            return proc.returncode is not None
        return not _pid_alive(pid)

    sink.persist_offset = sink.offset
    buf = b""
    drained_after_death = False
    with open(output_path, "rb") as f:
        f.seek(sink.offset)
        while True:
            chunk = f.read()
            if chunk:
                buf += chunk
                if len(buf) > STREAM_LINE_LIMIT:
                    # A pathological single line would otherwise grow the
                    # buffer forever; flush it as one line.
                    complete = [buf]
                    buf = b""
                else:
                    *complete, buf = buf.split(b"\n")
                for raw in complete:
                    sink.offset += len(raw) + 1
                    line = raw.decode("utf-8", "replace").strip()
                    if not line:
                        continue
                    if _process_stream_line(sink, line, terminate):
                        return
            elif dead():
                if drained_after_death:
                    if buf.strip():
                        # A crash may leave a final line without a trailing
                        # newline — it often holds the error that killed pi.
                        sink.offset += len(buf)
                        line = buf.decode("utf-8", "replace").strip()
                        _process_stream_line(sink, line, terminate)
                    return
                # One final pass so lines written just before death are seen.
                drained_after_death = True
            else:
                _tick_wait_credit(p, sink)
                if _active_elapsed(p, sink) > p.timeout_seconds + 60:
                    logger.error("%s hop %d: watchdog kill after %.0fs active seconds",
                                 p.log_prefix, p.hop, _active_elapsed(p, sink))
                    if proc is not None:
                        with contextlib.suppress(ProcessLookupError):
                            proc.kill()
                    else:
                        _terminate_group(pid, signal.SIGKILL)
                    # Death is observed on a later pass (then drained).
                await asyncio.sleep(TAIL_POLL_SECONDS)


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
    waiting_check: Callable[[], bool] | None
    # Prewalk: a system-prompt append delivered as a launch flag (never a
    # session turn) so omitting it on a later resume prunes it from context.
    append_system_prompt: str | None = None
    # Prewalk swap watch: while true, the hop ends with reason "swap" the first
    # time a turn calls edit/write after a todo list has been created.
    swap_after_edit: bool = False
    # Seed for the swap watch when a todo list already exists from a prior hop.
    todo_already: bool = False


def _build_cmd(p: _HopParams, msg: str) -> list[str]:
    cmd = ["pi", "--mode", "json", "-p", "--model", p.model]
    if CRACK_EXT.exists():
        cmd += ["-e", str(CRACK_EXT)]
    if p.tools is not None:
        cmd += ["--tools", p.tools]
    # Load the shared tool-usage guidance for every tool-bearing hop. A path arg
    # makes pi read the file's contents; it stacks with the prewalk append below
    # (`--append-system-prompt` may repeat).
    if CRACK_SYSTEM_MD.exists():
        cmd += ["--append-system-prompt", str(CRACK_SYSTEM_MD)]
    if p.append_system_prompt:
        cmd += ["--append-system-prompt", p.append_system_prompt]
    cmd += ["--session-id", p.session_id, "--session-dir", str(p.sessions_dir), msg]
    return cmd


async def _attempt_once(p: _HopParams, attempt_idx: int, attempt_message: str) -> dict:
    """Run one pi subprocess: redirect its stdout+stderr to the durable hop
    output file, tail it to completion, and report how it ended. ``persisted``
    counts turns committed to disk this attempt so the retry loop can
    distinguish resuming from replaying."""
    cmd = _build_cmd(p, attempt_message)
    logger.info("%s hop %d: full prompt:\n%s", p.log_prefix, p.hop, attempt_message)
    logger.info("+ %s", shlex.join(cmd))

    await async_wait_for_rate_limit(p.model)
    manifest_path, output_path = _hop_paths(p)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    # Carry forward prior grace-period detaches and bound them before we spawn
    # another pi for this hop (each attempt used to overwrite the sole pid slot).
    prior = _read_hop_manifest(manifest_path)
    detached_pids = _sweep_detached_pids(
        prior.get("detached_pids") or [],
        p.session_id, p.log_prefix, p.hop,
    )
    # start_new_session=True puts pi in its own process group so an external
    # STOP can kill pi *and* any children it spawned (npx MCP servers) via
    # the group leader's pid, which we publish to pid_file. Output goes to a
    # file (not a pipe) so pi survives a server reload: the restarted worker
    # re-attaches via the manifest instead of respawning.
    with open(output_path, "wb") as out:
        proc = await asyncio.create_subprocess_exec(
            *cmd, stdout=out, stderr=asyncio.subprocess.STDOUT,
            start_new_session=True, cwd=str(project_root()),
            env={**os.environ, **(p.env_extra or {})},
        )
    if p.pid_file is not None:
        try:
            p.pid_file.parent.mkdir(parents=True, exist_ok=True)
            p.pid_file.write_text(str(proc.pid), encoding="utf-8")
        except OSError as e:
            logger.warning("%s hop %d: could not write pid_file %s: %s",
                           p.log_prefix, p.hop, p.pid_file, e)
    sink = _StreamSink(p)
    sink.manifest_path = manifest_path
    sink.manifest = {
        "pid": proc.pid,
        "started_at": time.time(),
        "output_path": str(output_path),
        "offset": 0,
        "session_id": p.session_id,
        "model": p.model,
        "tools": p.tools,
        "message": attempt_message[:MANIFEST_MESSAGE_MAX_CHARS],
        "hop": p.hop,
        "timeout": p.timeout_seconds,
        "status": "running",
        "detached_pids": detached_pids,
    }
    _write_hop_manifest(manifest_path, sink.manifest)

    detached = False
    try:
        await _tail_events(sink, output_path, proc=proc, pid=proc.pid)
    except asyncio.CancelledError:
        # Server reload / job abort: DON'T kill pi — it keeps writing to the
        # output file (tini reaps it later) and the restarted worker
        # re-attaches via the manifest. The pid_file must survive too, so a
        # user STOP can still find the pid. Flush the offset of the last
        # persisted turn (a turn boundary): the re-attaching worker replays
        # at most the one partial in-flight turn.
        detached = True
        sink.manifest.update({"offset": sink.persist_offset, "status": "running"})
        _write_hop_manifest(manifest_path, sink.manifest)
        logger.info("%s hop %d: detached pi pid %d at offset %d (reload?); leaving it running",
                    p.log_prefix, p.hop, proc.pid, sink.persist_offset)
        raise
    finally:
        if not detached:
            try:
                await asyncio.wait_for(proc.wait(), timeout=EXIT_GRACE_SECONDS)
            except asyncio.TimeoutError:
                if sink.terminal:
                    # Terminal event already fired: pi is lingering on MCP
                    # client teardown. Do not SIGKILL — leave it running
                    # (tini / PID 1 reaps it). returncode stays None (= clean).
                    # Record the pid so the next attempt's sweep can bound it.
                    sink.manifest.setdefault("detached_pids", []).append(
                        {"pid": proc.pid, "since": time.time()},
                    )
                    logger.info(
                        "%s hop %d: pi pid %d still alive %.0fs after terminal "
                        "event; detaching (no SIGKILL)",
                        p.log_prefix, p.hop, proc.pid, EXIT_GRACE_SECONDS,
                    )
                else:
                    proc.kill()
                    await proc.wait()
            if p.pid_file is not None:
                try:
                    p.pid_file.unlink()
                except OSError:
                    pass
            crashed = (
                not sink.terminated_by_us
                and not sink.terminal
                and proc.returncode not in (0, None)
            )
            sink.manifest.update({
                "offset": sink.offset,
                "status": "crashed" if crashed else "done",
            })
            _write_hop_manifest(manifest_path, sink.manifest)

    # An external STOP kills the subprocess out from under the tail loop,
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
        "terminal": sink.terminal,
        "returncode": proc.returncode,
        "persisted": sink.persisted,
        "ended_in_error": sink.ended_in_error,
        "detail": _compose_detail("\n".join(sink.output_tail), "\n".join(sink.stderr_tail)),
    }


async def _reattach_attempt(
    p: _HopParams, attempt_idx: int, manifest_path: Path, manifest: dict
) -> dict:
    """Re-attach to a pi that survived a server reload: tail its output file
    from the stored offset to completion, persisting new turns — no second pi
    is spawned for the session. A pi that already died is drained from the
    file: a terminal event in the backlog completes the hop normally,
    otherwise the attempt reports a crash (rc -1) and the retry loop resumes
    the session with a fresh pi."""
    pid = int(manifest["pid"])
    output_path = Path(manifest.get("output_path") or str(_hop_paths(p)[1]))
    logger.info("%s hop %d: re-attaching to detached pi pid %d at offset %d",
                p.log_prefix, p.hop, pid, int(manifest.get("offset") or 0))
    sink = _StreamSink(p)
    sink.offset = int(manifest.get("offset") or 0)
    sink.manifest_path = manifest_path
    sink.manifest = manifest
    await _tail_events(sink, output_path, proc=None, pid=pid)

    # A pi that emitted agent_end may still be flushing its session files;
    # give it a moment to exit before a later hop spawns against the same dir.
    deadline = time.monotonic() + 5
    while _pid_alive(pid, p.session_id) and time.monotonic() < deadline:
        await asyncio.sleep(0.1)

    if p.pid_file is not None:
        try:
            p.pid_file.unlink()
        except OSError:
            pass
    sink.manifest.update({
        "offset": sink.offset,
        "status": "done" if sink.terminal else "crashed",
    })
    _write_hop_manifest(manifest_path, sink.manifest)

    # Same STOP classification as a freshly-spawned attempt.
    if not sink.terminated_by_us and p.stop_check is not None:
        try:
            if p.stop_check():
                sink.reason = "stopped"
                sink.terminated_by_us = True
        except Exception:
            logger.exception("%s hop %d: stop_check raised", p.log_prefix, p.hop)

    elapsed = time.monotonic() - p.start
    logger.info("%s hop %d: re-attached attempt %d finished reason=%s persisted=%d elapsed=%.2fs",
                p.log_prefix, p.hop, attempt_idx + 1, sink.reason, sink.persisted, elapsed)
    return {
        "reason": sink.reason,
        "terminated_by_us": sink.terminated_by_us,
        "terminal": sink.terminal,
        # Unknown rc for a re-attached process: a terminal event is a clean
        # end (None); anything else is treated as a crash so it is retried.
        "returncode": None if sink.terminal else -1,
        "persisted": sink.persisted,
        "ended_in_error": sink.ended_in_error,
        "detail": _compose_detail("\n".join(sink.output_tail), "\n".join(sink.stderr_tail)),
    }


def _live_detached_manifest(p: _HopParams) -> tuple[Path, dict] | None:
    """The hop manifest worth re-attaching to: status "running", same
    session, and either a live pid or unconsumed output left to drain (a pi
    that finished or crashed during the restart window)."""
    manifest_path, _ = _hop_paths(p)
    manifest = _read_hop_manifest(manifest_path)
    if manifest.get("status") != "running":
        return None
    if manifest.get("session_id") != p.session_id:
        return None
    pid = manifest.get("pid")
    if not isinstance(pid, int):
        return None
    if _pid_alive(pid, p.session_id):
        return manifest_path, manifest
    try:
        size = Path(str(manifest.get("output_path") or "")).stat().st_size
    except OSError:
        return None
    if size > int(manifest.get("offset") or 0):
        return manifest_path, manifest
    return None


async def _run_hop_with_retries(
    p: _HopParams,
    message: str,
    record_error=None,
    error_budget: Callable[[], int] | None = None,
) -> str:
    """Drive _attempt_once until the hop stops cleanly, the no-progress streak
    is exhausted, or the durable error budget is spent; then either return the
    stop reason or raise PiError.

    Every error is retried: hard failures (SIGKILL/-9 included) and empty runs
    sleep on the HARD_RETRY_DELAYS schedule, transient upstream failures on
    TRANSIENT_RETRY_DELAYS. An attempt that persisted a turn counts as
    progress: it resets the no-progress streak and the next attempt resumes
    the preserved session with RESUME_MESSAGE. Two caps end the loop: the
    streak cap (len(HARD_RETRY_DELAYS) + 1 attempts without progress) and the
    error budget (``error_budget()`` total recorded errors, default
    MAX_TOTAL_ERRORS), the latter raising PiError(over_budget=True)."""
    budget = error_budget if error_budget is not None else (lambda: ratelimit.MAX_TOTAL_ERRORS)
    attempt_message = message
    last_message = "pi command failed"
    last_detail = ""
    last_rc: int | None = None
    consecutive_no_progress = 0  # failed attempts since the last persisted turn
    transient_reattempts = 0
    total_attempts = 0
    local_errors = 0  # fallback count when no recorder is wired (or it breaks)

    # Reload survival: a pi detached by a server restart (live pid, or a
    # backlog to drain) is re-attached as the first attempt instead of
    # spawning a second pi for the same session.
    detached = _live_detached_manifest(p)

    while True:
        if detached is not None:
            res = await _reattach_attempt(p, total_attempts, *detached)
            detached = None
        else:
            res = await _attempt_once(p, total_attempts, attempt_message)
        total_attempts += 1

        # A terminal stream (agent_end / sentinel / time_cap) is a clean hop
        # end even when the process linger-detaches (rc None) or exits nonzero
        # during teardown — never treat that as a crash to retry.
        failed = (
            not res["terminated_by_us"]
            and not res["terminal"]
            and res["returncode"] not in (0, None)
        )
        if not failed:
            # Unrecoverable pi-internal error on the last turn (e.g. exhausted
            # 429 auto-retries): retry even if earlier turns in this attempt
            # persisted — otherwise the hop looks "done" and the chat goes idle.
            if res["ended_in_error"]:
                last_message = f"pi gave up: {res['ended_in_error']}"
                last_detail = res["detail"]
                last_rc = res["returncode"]
            elif res["persisted"] > 0 or res["reason"] != "agent_end":
                return res["reason"]
            else:
                # Clean exit with no real turns: content-less responses only —
                # retried like a hard failure, surfaced as "empty" at streak end.
                last_message = "pi returned only empty turns"
                last_detail = res["detail"]
                last_rc = res["returncode"]
        else:
            last_message = f"pi exited {res['returncode']}"
            last_detail = res["detail"]
            last_rc = res["returncode"]

        local_errors += 1
        total_errors = local_errors
        if record_error is not None:
            try:
                total_errors = int(record_error({
                    "message": last_message,
                    "detail": last_detail,
                    "rc": last_rc,
                    "attempt": total_attempts,
                    "phase": p.log_prefix,
                }))
            except Exception:
                # A broken recorder must never wedge retries.
                logger.exception("%s hop %d: record_error raised", p.log_prefix, p.hop)
                total_errors = local_errors

        if res["persisted"] > 0:
            # Progress: turns are committed and the session dir kept them, so
            # the streak resets and every further attempt resumes the session,
            # never replays the original prompt.
            consecutive_no_progress = 0
            attempt_message = RESUME_MESSAGE
        else:
            consecutive_no_progress += 1

        if total_errors >= budget():
            logger.error("%s hop %d: error budget spent (%d errors); giving up",
                         p.log_prefix, p.hop, total_errors)
            raise PiError(
                f"{last_message} after {total_attempts} attempts "
                f"({total_errors} recorded errors — budget spent)",
                detail=last_detail, over_budget=True,
            )

        if consecutive_no_progress > len(ratelimit.HARD_RETRY_DELAYS):
            if not failed:
                logger.error("%s hop %d: pi returned only empty turns after %d attempts",
                             p.log_prefix, p.hop, total_attempts)
                return "empty"
            logger.error("%s hop %d: %d consecutive attempts without progress; giving up",
                         p.log_prefix, p.hop, consecutive_no_progress)
            raise PiError(
                f"{last_message} after {total_attempts} attempts "
                f"(no progress in {consecutive_no_progress} consecutive attempts)",
                detail=last_detail,
            )

        if failed and is_transient(last_detail):
            logger.warning("%s hop %d: transient failure (rc=%s); will resume (reattempt %d)",
                           p.log_prefix, p.hop, last_rc, transient_reattempts + 1)
            await _async_transient_backoff_sleep(transient_reattempts)
            transient_reattempts += 1
        else:
            logger.warning("%s hop %d: %s; retrying (streak %d)",
                           p.log_prefix, p.hop, last_message, consecutive_no_progress)
            await _async_hard_backoff_sleep(consecutive_no_progress)


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
) -> str:
    """Run one hop of a tool-using agent and stream its JSON events (async).

    Parameterized on model / session / tools so multiple stages can share it.
    ``tools=None`` omits ``--tools`` so pi runs with every tool it has. A hop
    has no turn cap: it ends only when the model stops on its own, on the
    sentinel (matched *on its own line* in assistant text), on the time cap, on
    the prewalk swap point (``swap_after_edit``: first edit/write after a todo
    exists), on an external stop, or on an unrecoverable error. The pi session
    is persisted under ``sessions_dir`` so the next hop/step resumes it via the
    same --session-id. ``persist_turn(current_turn, hop)`` is called for every
    completed non-empty turn. Returns the stop reason: "sentinel", "time_cap",
    "agent_end", "empty", "swap", or "stopped".

    Prewalk (``append_system_prompt`` / ``swap_after_edit`` / ``todo_already``):
    the append is a launch flag, never a session turn, so a later resume that
    omits it prunes the planning instruction from the cheap model's context.

    ``pid_file`` (optional): the pi subprocess is started in its own session and
    its PID written here so another process (e.g. a web STOP handler) can kill
    the whole process group. ``stop_check`` (optional, called with no args): when
    it returns truthy after the subprocess ends, the hop is classified as
    "stopped" (a clean, intentional halt) rather than a crash to retry.

    ``waiting_check`` (optional, called with no args): while it returns truthy
    the hop's agent is suspended in a server-side wait (a blocking ``wait_join``
    long-poll). That time is credited out of both the in-stream time cap and
    the hard watchdog — timeouts bill active time only.

    ``record_prompt`` (optional, called once per hop call, not per retry
    attempt) receives ``{"kind": "user_prompt", "compiled": message, "hop": hop,
    "at": ...}`` so callers can persist the exact compiled prompt into their
    trajectory alongside the turns it produced.

    ``record_error`` (optional) is called once per *failed attempt* (hard,
    transient, or empty) with ``{"message", "detail", "rc", "attempt",
    "phase"}`` and returns the new total error count; ``error_budget``
    (optional, default ``ratelimit.MAX_TOTAL_ERRORS``) supplies the total-error
    cap at which the hop raises PiError(over_budget=True).

    Retries: every failure is retried. *Transient* upstream failures (see
    ``is_transient``) follow their own longer backoff; hard failures
    (SIGKILL/-9 included) and empty runs follow HARD_RETRY_DELAYS. Whenever an
    attempt persisted turns, the preserved session is resumed with
    ``RESUME_MESSAGE`` instead of replaying, so no work is duplicated, and the
    no-progress streak resets. The hop gives up after
    ``len(HARD_RETRY_DELAYS) + 1`` consecutive attempts without progress, or
    when the recorded-error total reaches the budget (over_budget).

    Reload survival: pi's event stream goes to a durable hop.jsonl file with a
    hop.json manifest (pid, session, offset). If a previous, still-running
    (or just-finished) detached hop exists for this session, the first
    attempt re-attaches to it — tailing from the stored offset — instead of
    spawning a second pi that would corrupt the shared session dir.
    """
    p = _HopParams(log_prefix=log_prefix, model=model, session_id=session_id,
                   sessions_dir=sessions_dir, tools=tools, start=start, sentinel=sentinel,
                   timeout_seconds=timeout_seconds, persist_turn=persist_turn, hop=hop,
                   pid_file=pid_file, stop_check=stop_check, env_extra=env_extra,
                   waiting_check=waiting_check, append_system_prompt=append_system_prompt,
                   swap_after_edit=swap_after_edit, todo_already=todo_already)
    p.sessions_dir.mkdir(parents=True, exist_ok=True)

    if record_prompt is not None:
        try:
            record_prompt({"kind": "user_prompt", "compiled": message, "hop": hop, "at": time.time()})
        except Exception:
            logger.exception("%s hop %d: record_prompt raised", log_prefix, hop)

    return await _run_hop_with_retries(
        p, message, record_error=record_error, error_budget=error_budget
    )


def run_agent_hop(**kwargs) -> str:
    """Sync wrapper over :func:`arun_agent_hop` for thread-based callers.

    Must NOT be called from inside a running event loop (asyncio.run)."""
    return asyncio.run(arun_agent_hop(**kwargs))
