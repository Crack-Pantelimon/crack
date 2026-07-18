"""Shared `pi` subprocess machinery: rate limiting, single-shot text calls, the
streaming JSON-mode hop runner, transcript rendering, and path-ref extraction.

Extracted from app.py (move-with-minimal-edits) so both the Explore and Plan
stages can share it. Everything here logs through the uvicorn logger and is only
ever called from background threads.
"""

from __future__ import annotations

import json
import logging
import re
import shlex
import subprocess
import threading
import time
from pathlib import Path

from crack_server import paths

logger = logging.getLogger("uvicorn.error")

# The title model is hosted behind the nvidia provider, so it shares the
# nvidia-wide 40 calls/minute budget; it additionally has its own tighter
# 30 calls/minute budget and a ~4k-token (~10,000 char) input limit.
TITLE_MODEL = "nvidia/nemotron-3-nano-30b-a3b"

PI_TIMEOUT_SECONDS = 120

NVIDIA_CALLS_PER_MINUTE = 40
TITLE_CALLS_PER_MINUTE = 30
TITLE_MAX_INPUT_CHARS = 10_000

# Every pi invocation is retried on a transient process failure (nonzero exit,
# SIGKILL/-9 OOM, timeout, missing binary). We make at least PI_RETRY_ATTEMPTS
# attempts spaced by exponential backoff, anchored so the final attempt begins
# exactly PI_RETRY_WINDOW_SECONDS after the first attempt started.
PI_RETRY_ATTEMPTS = 4
PI_RETRY_WINDOW_SECONDS = 61.0

# Rolling raw-output buffer size: the last N lines of pi's stdout+stderr are kept
# and surfaced in the error so a failure is diagnosable from the UI, not just logs.
OUTPUT_TAIL_LINES = 10


class PiError(RuntimeError):
    """A pi subprocess failure carrying a short message plus a ``detail`` blob
    (the last few lines of captured stdout+stderr) for inline UI display."""

    def __init__(self, message: str, detail: str = "") -> None:
        super().__init__(message)
        self.detail = detail


def _retry_offsets(n: int, total: float) -> list[float]:
    """Offsets (seconds, from the first attempt) at which each attempt starts.

    Exponentially spaced (each gap doubles) and anchored so ``offsets[0] == 0``
    and ``offsets[-1] == total``. For n=4, total=61 → [0, 8.71, 26.14, 61.0]."""
    if n <= 1:
        return [0.0]
    denom = (2 ** (n - 1)) - 1
    return [total * ((2 ** k) - 1) / denom for k in range(n)]


def _retry_backoff_sleep(next_attempt: int, first_attempt_at: float) -> None:
    """Sleep until attempt ``next_attempt`` (0-based) is due per the schedule."""
    offsets = _retry_offsets(PI_RETRY_ATTEMPTS, PI_RETRY_WINDOW_SECONDS)
    idx = min(next_attempt, len(offsets) - 1)
    delay = (first_attempt_at + offsets[idx]) - time.monotonic()
    if delay > 0:
        logger.info(
            "pi-retry: sleeping %.1fs before attempt %d/%d",
            delay, next_attempt + 1, PI_RETRY_ATTEMPTS,
        )
        time.sleep(delay)

READ_MAX_LINES = 200
READ_MAX_CHARS = 10_000

_PATH_REF_RE = re.compile(
    r"`?([A-Za-z0-9_][A-Za-z0-9_./-]*\.[A-Za-z]{1,10})`?(?::(\d+)(?:-(\d+))?)?"
)


class RateLimiter:
    """Thread-safe minimum-interval limiter: converts a calls/minute budget into a
    minimum spacing between calls, and blocks the caller until that spacing has
    elapsed. Holding the lock across the sleep is intentional — it serializes callers
    so the configured spacing is always respected regardless of which thread arrives
    first, which is all a local dev tool needs."""

    def __init__(self, name: str, calls_per_minute: float) -> None:
        self._name = name
        self._min_interval = 60.0 / calls_per_minute
        self._lock = threading.Lock()
        self._last_call = 0.0

    def wait(self) -> None:
        with self._lock:
            now = time.monotonic()
            sleep_for = self._min_interval - (now - self._last_call)
            if sleep_for > 0:
                time.sleep(sleep_for)
                logger.info("rate-limit(%s): slept %.2fs", self._name, sleep_for)
            else:
                logger.info(
                    "rate-limit(%s): no wait needed (last call %.2fs ago, min interval %.2fs)",
                    self._name,
                    now - self._last_call,
                    self._min_interval,
                )
            self._last_call = time.monotonic()


# One limiter for the shared nvidia-provider budget (applies to every pi call below,
# since every model in use is nvidia-hosted), plus a tighter limiter keyed by model id
# for models with their own additional per-model budget.
_nvidia_limiter = RateLimiter("nvidia-provider", NVIDIA_CALLS_PER_MINUTE)
_model_limiters: dict[str, RateLimiter] = {
    TITLE_MODEL: RateLimiter(f"model:{TITLE_MODEL}", TITLE_CALLS_PER_MINUTE),
}


def wait_for_rate_limit(model: str) -> None:
    # Both limiters are deficit-only: each tracks its own last_call and sleeps
    # just the remainder of its own interval, so running them sequentially does
    # not double-count — time spent waiting on the shared nvidia budget also
    # counts toward the per-model interval.
    _nvidia_limiter.wait()
    limiter = _model_limiters.get(model)
    if limiter is not None:
        limiter.wait()


def run_pi_text(
    prompt: str, log_prefix: str, model: str, max_input_chars: int | None = None
) -> tuple[str, float]:
    """Run `pi` non-interactively with a single text prompt.

    Returns ``(text, elapsed_seconds)``. Logs the full prompt, exact command
    line, timeout, elapsed time, and an output summary so failures are
    diagnosable from server logs alone. Raises RuntimeError because this helper
    is only used from background threads, where HTTPException has no request
    context to turn into.
    """
    if max_input_chars is not None and len(prompt) > max_input_chars:
        logger.info(
            "%s: truncating prompt from %d to %d chars", log_prefix, len(prompt), max_input_chars
        )
        prompt = prompt[:max_input_chars]

    cmd = ["pi", "--model", model, "--print", "--no-session", "--no-tools", prompt]

    logger.info("%s: full prompt:\n%s", log_prefix, prompt)
    logger.info("%s: timeout=%ss", log_prefix, PI_TIMEOUT_SECONDS)
    logger.info("+ %s", shlex.join(cmd))

    first_attempt_at = time.monotonic()
    last_error = "pi command failed"
    last_detail = ""

    for attempt in range(PI_RETRY_ATTEMPTS):
        if attempt > 0:
            _retry_backoff_sleep(attempt, first_attempt_at)

        wait_for_rate_limit(model)
        start = time.monotonic()
        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=PI_TIMEOUT_SECONDS,
            )
        except subprocess.TimeoutExpired as e:
            elapsed = time.monotonic() - start
            logger.error("%s: pi timed out after %.2fs (attempt %d)", log_prefix, elapsed, attempt + 1)
            last_error = "pi command timed out"
            last_detail = _tail_text((e.stdout or "") + (e.stderr or ""))
            continue
        except FileNotFoundError:
            elapsed = time.monotonic() - start
            logger.error("%s: pi command not found on PATH (after %.2fs)", log_prefix, elapsed)
            last_error = "pi command not found"
            last_detail = ""
            continue

        elapsed = time.monotonic() - start
        logger.info(
            "%s: pi exited %d in %.2fs (attempt %d/%d)",
            log_prefix, result.returncode, elapsed, attempt + 1, PI_RETRY_ATTEMPTS,
        )

        if result.returncode == 0:
            text = result.stdout.strip()
            logger.info("%s: output summary: %r", log_prefix, text[:200])
            return text, elapsed

        detail = _tail_text((result.stdout or "") + (result.stderr or ""))
        logger.error("%s: pi exited %d; last output:\n%s", log_prefix, result.returncode, detail)
        last_error = f"pi exited {result.returncode}"
        last_detail = detail

    raise PiError(f"{last_error} after {PI_RETRY_ATTEMPTS} attempts", detail=last_detail)


def _tail_text(text: str, n: int = OUTPUT_TAIL_LINES) -> str:
    """Keep the last ``n`` non-empty lines of a captured output blob."""
    lines = [ln for ln in (text or "").splitlines() if ln.strip()]
    return "\n".join(lines[-n:])


def text_from_content(content) -> str:
    """Extract plain text from a pi message content block (string or list)."""
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return ""
    parts = []
    for block in content:
        if isinstance(block, dict) and block.get("type") == "text":
            parts.append(block.get("text", ""))
    return "".join(parts)


def apply_event_to_turn(event: dict, current_turn: dict) -> None:
    """Accumulate assistant text, thinking, and tool blocks from a pi JSON event.

    We only consume `message_end` events to avoid double-counting deltas; the final
    message carries the complete content for that turn. User messages are skipped.
    Tool results are merged into the matching toolCall block by id.
    """
    etype = event.get("type")
    if etype == "turn_start":
        current_turn.clear()
        current_turn.update({"text": "", "thinking": "", "tool_blocks": []})
        return

    if etype != "message_end":
        return

    message = event.get("message")
    if not isinstance(message, dict):
        return

    role = message.get("role")
    if role == "user":
        return

    if role == "toolResult":
        content = message.get("content", [])
        output = text_from_content(content)
        tool_call_id = message.get("toolCallId")
        # Merge the result into the matching toolCall block, if present.
        merged = False
        for block in current_turn.get("tool_blocks", []):
            if block.get("id") == tool_call_id:
                block["output"] = output
                merged = True
                break
        if not merged:
            current_turn.setdefault("tool_blocks", []).append(
                {
                    "id": tool_call_id,
                    "name": message.get("toolName", "tool"),
                    "input": "",
                    "output": output,
                }
            )
        return

    # Assistant (or other non-user) message.
    content = message.get("content")
    if not isinstance(content, list):
        return

    for block in content:
        if not isinstance(block, dict):
            continue
        btype = block.get("type")
        if btype == "text":
            current_turn["text"] += block.get("text", "")
        elif btype == "thinking":
            current_turn["thinking"] += block.get("thinking", "")
        elif btype == "toolCall":
            current_turn.setdefault("tool_blocks", []).append(
                {
                    "id": block.get("id"),
                    "name": block.get("name", "tool"),
                    "input": block.get("arguments", block.get("input", "")),
                    "output": "",
                }
            )


def turn_has_content(current_turn: dict) -> bool:
    return bool(
        current_turn.get("text", "").strip()
        or current_turn.get("thinking", "").strip()
        or current_turn.get("tool_blocks")
    )


def count_turn_groups(turns: list[dict]) -> int:
    """Count turn *groups*: a consecutive streak of tool-calling turns counts once.

    Turn caps budget model reasoning, not file reads, so a run of turns that only
    exist to drive tools is one group. This is the same rule run_agent_hop
    applies incrementally, so stage loops can derive the counted total from the
    persisted turn list."""
    count = 0
    prev_had_tools = False
    for turn in turns:
        had_tools = bool(turn.get("tool_blocks"))
        if not (had_tools and prev_had_tools):
            count += 1
        prev_had_tools = had_tools
    return count


def truncate_output(text: str, max_lines: int = READ_MAX_LINES, max_chars: int = READ_MAX_CHARS) -> tuple[str, str | None]:
    """Truncate tool output to max_lines / max_chars (whichever hits first).

    Returns (text, marker); marker is None when nothing was cut."""
    lines = text.splitlines()
    reason = None
    if len(lines) > max_lines:
        text = "\n".join(lines[:max_lines])
        reason = f"{max_lines} lines"
    if len(text) > max_chars:
        text = text[:max_chars]
        reason = f"{max_chars:,} chars"
    if reason is None:
        return text, None
    return text, f"… [truncated at {reason} — ask the agent to read specific line ranges if needed]"


def tail_truncate(text: str, max_chars: int) -> str:
    """Keep the tail of a long transcript (recent turns matter most to gate/summary)."""
    if len(text) <= max_chars:
        return text
    return "… [earlier transcript omitted]\n" + text[-max_chars:]


def fit_nano_transcript(template: str, transcript: str, *other_parts: str) -> str:
    """Tail-truncate a transcript so template + other parts + transcript fit the nano
    input limit. The hard cut in `run_pi_text` would otherwise chop the tail — the
    most recent, most useful turns."""
    used = len(template) + sum(len(p) for p in other_parts) + 200  # safety margin
    return tail_truncate(transcript, max(2_000, TITLE_MAX_INPUT_CHARS - used))


def render_transcript_plaintext(turns: list[dict]) -> str:
    """Render a plaintext transcript of agent turns for gate/summary prompts."""
    parts = []
    for i, turn in enumerate(turns, 1):
        parts.append(f"--- Turn {i} (hop {turn.get('hop', 1)}) ---")
        if turn.get("text"):
            parts.append(turn["text"])
        if turn.get("thinking"):
            parts.append("Thinking:\n" + turn["thinking"])
        for block in turn.get("tool_blocks", []):
            name = block.get("name", "tool")
            if block.get("input") not in (None, ""):
                parts.append(f"Tool {name}: {block['input']}")
            if block.get("output") not in (None, ""):
                parts.append(f"Result:\n{block['output']}")
    return "\n\n".join(parts)


def resolve_path_ref(root: Path, candidate: str) -> Path | None:
    """Resolve a model-emitted path candidate to a real file under the project root.

    The model emits paths like `workspace/src/lib.rs` or `/workspace/src/lib.rs` even
    though they are relative to the root itself, so normalize before checking:
      1. absolute path starting with str(root) → resolve directly;
      2. leading `root.name + "/"` (e.g. `workspace/…`) → strip it;
      3. plain `root / candidate`.
    First candidate that is an existing file under the root wins."""
    tries: list[Path] = []
    root_str = str(root)
    if candidate == root_str or candidate.startswith(root_str + "/"):
        tries.append(Path(candidate))
    if candidate.startswith(root.name + "/"):
        tries.append(root / candidate[len(root.name) + 1:])
    tries.append(root / candidate)

    for path in tries:
        try:
            resolved = path.resolve()
        except (OSError, RuntimeError):
            continue
        if resolved.is_file() and (resolved == root or root in resolved.parents):
            return resolved
    return None


def extract_path_refs(text: str) -> list[dict]:
    """Find file-path-looking strings in ``text`` and resolve them under the project root.

    Only references that resolve to real files are kept (unresolvable candidates are
    dropped). Returns dicts with keys ``rel_path``, ``start``, ``end``, deduped on all
    three."""
    root = paths.project_root()
    seen: set[tuple[str, int | None, int | None]] = set()
    refs: list[dict] = []

    for match in _PATH_REF_RE.finditer(text):
        candidate = match.group(1)
        start = int(match.group(2)) if match.group(2) else None
        end = int(match.group(3)) if match.group(3) else start

        abs_path = resolve_path_ref(root, candidate)
        if abs_path is None:
            continue
        rel_path = abs_path.relative_to(root).as_posix()

        key = (rel_path, start, end)
        if key in seen:
            continue
        seen.add(key)

        refs.append({"rel_path": rel_path, "start": start, "end": end})

    return refs


def read_file_lines(root: Path, rel_path: str, start: int | None, end: int | None) -> tuple[str, int, int, str | None]:
    """Read a clamped line range from a project file.

    Returns (text, start, end, truncation_marker). The range is capped at
    READ_MAX_LINES lines and the text at READ_MAX_CHARS chars."""
    path = root / rel_path
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return "", 0, 0, None

    n = len(lines)
    if start is None or start < 1:
        start = 1
    if end is None or end < start:
        end = start + 49
    if start > n:
        start = n
    if end > n:
        end = n
    if end - start + 1 > READ_MAX_LINES:
        end = start + READ_MAX_LINES - 1

    text, marker = truncate_output("\n".join(lines[start - 1 : end]))
    return text, start, end, marker


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
    turns_per_hop: int,
    max_turns: int,
    timeout_seconds: int,
    total_turns: int,
    persist_turn,
    hop: int = 1,
) -> str:
    """Run one hop of a tool-using agent and stream its JSON events.

    Generic form of the old `_run_explore_hop`, parameterized on model / session /
    tools / caps so multiple stages can share it. ``tools=None`` omits ``--tools``
    so pi runs with every tool it has (built-in + extension, e.g. MCP). A hop is
    capped at ``turns_per_hop`` *counted* turns; the pi session is persisted under
    ``sessions_dir`` so the next hop/step resumes it via the same --session-id.
    ``persist_turn(current_turn, hop)`` is called for every completed non-empty
    turn. Turn counting follows ``count_turn_groups``: a consecutive streak of
    tool-calling turns increments the counter only once, and turns with no
    content at all are neither persisted nor counted. ``sentinel`` (optional)
    ends the hop early when it appears in assistant text. Returns the stop
    reason: "sentinel", "hop_cap", "turn_cap", "time_cap", "agent_end" (pi
    finished on its own), or "empty" (pi repeatedly returned content-less turns).
    """
    sessions_dir.mkdir(parents=True, exist_ok=True)

    cmd = [
        "pi",
        "--mode",
        "json",
        "-p",
        "--model",
        model,
    ]
    if tools is not None:
        cmd += ["--tools", tools]
    cmd += [
        "--session-id",
        session_id,
        "--session-dir",
        str(sessions_dir),
        message,
    ]

    logger.info("%s hop %d: full prompt:\n%s", log_prefix, hop, message)
    logger.info("+ %s", shlex.join(cmd))

    def _attempt(attempt_idx: int) -> dict:
        """Run one pi subprocess: stream, persist completed turns, and report how
        it ended. ``persisted`` counts turns committed to disk this attempt, so the
        retry loop only retries a *transient* failure that made no forward progress
        (a partial-progress crash is kept, not silently replayed)."""
        wait_for_rate_limit(model)
        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        current_turn: dict = {}
        hop_turns = 0
        total = total_turns
        reason = "agent_end"
        terminated_by_us = False
        persisted = 0
        output_tail: list[str] = []
        # Timing (stamped here in the loop, not in the pure apply_event_to_turn):
        # monotonic start of the current turn and of each in-flight toolCall id, so
        # turn_end / toolResult events can attach elapsed seconds to the turn dict.
        turn_started_at: float | None = None
        tool_starts: dict = {}
        # Group counting (see count_turn_groups): a streak of consecutive
        # tool-calling turns increments the turn counter only once.
        prev_had_tools = False

        def _persist(turn: dict, h: int) -> None:
            nonlocal persisted
            persisted += 1
            persist_turn(turn, h)

        try:
            for line in proc.stdout or []:
                line = line.strip()
                if not line:
                    continue
                # Keep a rolling tail of the raw output (JSON or not) so a crash is
                # diagnosable inline in the UI, not just from server logs.
                output_tail.append(line[:500])
                del output_tail[:-OUTPUT_TAIL_LINES]

                try:
                    event = json.loads(line)
                except json.JSONDecodeError:
                    logger.warning("%s hop %d: non-JSON line: %s", log_prefix, hop, line[:200])
                    continue

                apply_event_to_turn(event, current_turn)
                etype = event.get("type")

                now = time.monotonic()
                if etype == "turn_start":
                    turn_started_at = now
                elif etype == "turn_end" and turn_started_at is not None:
                    current_turn["elapsed"] = round(now - turn_started_at, 3)
                elif etype == "message_end":
                    # Local name (not `message`) so we don't clobber the prompt param.
                    msg = event.get("message")
                    if isinstance(msg, dict):
                        role = msg.get("role")
                        if role == "toolResult":
                            started = tool_starts.pop(msg.get("toolCallId"), None)
                            if started is not None:
                                for block in current_turn.get("tool_blocks", []):
                                    if block.get("id") == msg.get("toolCallId"):
                                        block["elapsed"] = round(now - started, 3)
                                        break
                        elif role != "user":
                            content = msg.get("content")
                            if isinstance(content, list):
                                for block in content:
                                    if (
                                        isinstance(block, dict)
                                        and block.get("type") == "toolCall"
                                        and block.get("id") is not None
                                    ):
                                        tool_starts[block["id"]] = now

                if (
                    sentinel is not None
                    and etype == "message_end"
                    and sentinel in current_turn.get("text", "")
                ):
                    if turn_has_content(current_turn):
                        _persist(current_turn, hop)
                    logger.info("%s hop %d: sentinel %s received", log_prefix, hop, sentinel)
                    reason = "sentinel"
                    terminated_by_us = True
                    proc.terminate()
                    break

                if etype == "turn_end":
                    if not turn_has_content(current_turn):
                        # Content-less turns (empty model responses) are noise:
                        # never persist them and never spend turn budget on them.
                        logger.warning(
                            "%s hop %d: empty turn (no text/thinking/tool blocks); skipped",
                            log_prefix, hop,
                        )
                        prev_had_tools = False
                        continue
                    had_tools = bool(current_turn.get("tool_blocks"))
                    counted = not (had_tools and prev_had_tools)
                    prev_had_tools = had_tools
                    if counted:
                        hop_turns += 1
                        total += 1
                    _persist(current_turn, hop)
                    logger.info(
                        "%s hop %d: completed turn %d/%d (hop), %d/%d (total)%s",
                        log_prefix, hop, hop_turns, turns_per_hop, total, max_turns,
                        "" if counted else " (tool-group continuation, not counted)",
                    )
                    if total >= max_turns:
                        reason = "turn_cap"
                        terminated_by_us = True
                        proc.terminate()
                        break
                    if hop_turns >= turns_per_hop:
                        reason = "hop_cap"
                        terminated_by_us = True
                        proc.terminate()
                        break

                if time.monotonic() - start > timeout_seconds:
                    if turn_has_content(current_turn) and etype != "turn_end":
                        _persist(current_turn, hop)
                    reason = "time_cap"
                    terminated_by_us = True
                    proc.terminate()
                    break

                if etype in ("agent_end", "agent_settled"):
                    break
        finally:
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()

        elapsed = time.monotonic() - start
        logger.info(
            "%s hop %d: attempt %d finished reason=%s hop_turns=%d persisted=%d total_elapsed=%.2fs rc=%s",
            log_prefix, hop, attempt_idx + 1, reason, hop_turns, persisted, elapsed, proc.returncode,
        )
        return {
            "reason": reason,
            "terminated_by_us": terminated_by_us,
            "returncode": proc.returncode,
            "persisted": persisted,
            "detail": "\n".join(output_tail),
        }

    first_attempt_at = time.monotonic()
    last_detail = ""
    last_rc: int | None = None

    for attempt in range(PI_RETRY_ATTEMPTS):
        if attempt > 0:
            _retry_backoff_sleep(attempt, first_attempt_at)
        res = _attempt(attempt)

        failed = not res["terminated_by_us"] and res["returncode"] not in (0, None)
        if not failed:
            # A clean exit that persisted no real turns means the model returned
            # only content-less responses: retry, then surface "empty" so callers
            # fail the stage instead of treating silence as success.
            if res["persisted"] > 0 or res["reason"] != "agent_end":
                return res["reason"]
            if attempt == PI_RETRY_ATTEMPTS - 1:
                logger.error(
                    "%s hop %d: pi returned only empty turns after %d attempts",
                    log_prefix, hop, PI_RETRY_ATTEMPTS,
                )
                return "empty"
            logger.warning(
                "%s hop %d: pi returned only empty turns; retrying", log_prefix, hop
            )
            continue

        last_detail = res["detail"]
        last_rc = res["returncode"]
        # Partial progress (turns already committed to disk) is real work: don't
        # replay it. Only retry a clean transient failure that produced nothing.
        if res["persisted"] > 0:
            logger.info(
                "%s hop %d: pi exited %s after %d persisted turn(s); not retrying",
                log_prefix, hop, last_rc, res["persisted"],
            )
            break

    raise PiError(
        f"pi exited {last_rc} after {PI_RETRY_ATTEMPTS} attempts", detail=last_detail
    )
