"""Shared `pi` subprocess machinery: rate limiting, single-shot text calls, the
streaming JSON-mode hop runner, transcript rendering, and path-ref extraction.

A6 split the implementation into three modules:

- ``ratelimit.py`` — RateLimiter, the limiter registry, retry-offset helpers;
- ``pi_proc.py`` — run_pi_text, run_agent_hop, kill_pid_file, PiError;
- ``transcript.py`` — turn accumulation, transcript rendering, path-refs.

This module is kept as a thin re-export shim so existing ``pi_runner.<name>``
imports keep working without churning every caller in the same commit. Note
that monkeypatching must target the owning module (e.g.
``crack_server.ratelimit.TRANSIENT_RETRY_DELAYS``), not this shim. Everything
here logs through the uvicorn logger and is only ever called from background
threads.
"""

from __future__ import annotations

from crack_server.pi_proc import (
    OUTPUT_TAIL_LINES,
    PiError,
    PiStopped,
    kill_pid_file,
    run_agent_hop,
    run_pi_text,
)
from crack_server.ratelimit import (
    NVIDIA_CALLS_PER_MINUTE,
    PI_RETRY_ATTEMPTS,
    PI_RETRY_WINDOW_SECONDS,
    PI_TIMEOUT_SECONDS,
    RESUME_MESSAGE,
    TITLE_CALLS_PER_MINUTE,
    TITLE_MAX_INPUT_CHARS,
    TITLE_MODEL,
    TRANSIENT_RETRY_DELAYS,
    RateLimiter,
    is_transient,
    limiter_for,
    wait_for_rate_limit,
)
from crack_server.transcript import (
    READ_MAX_CHARS,
    READ_MAX_LINES,
    apply_event_to_turn,
    count_turn_groups,
    extract_path_refs,
    fit_nano_transcript,
    read_file_lines,
    render_transcript_plaintext,
    resolve_path_ref,
    tail_truncate,
    text_from_content,
    truncate_output,
    turn_has_content,
)

__all__ = [
    "NVIDIA_CALLS_PER_MINUTE",
    "OUTPUT_TAIL_LINES",
    "PI_RETRY_ATTEMPTS",
    "PI_RETRY_WINDOW_SECONDS",
    "PI_TIMEOUT_SECONDS",
    "READ_MAX_CHARS",
    "READ_MAX_LINES",
    "RESUME_MESSAGE",
    "TITLE_CALLS_PER_MINUTE",
    "TITLE_MAX_INPUT_CHARS",
    "TITLE_MODEL",
    "TRANSIENT_RETRY_DELAYS",
    "PiError",
    "PiStopped",
    "RateLimiter",
    "apply_event_to_turn",
    "count_turn_groups",
    "extract_path_refs",
    "fit_nano_transcript",
    "is_transient",
    "kill_pid_file",
    "limiter_for",
    "read_file_lines",
    "render_transcript_plaintext",
    "resolve_path_ref",
    "run_agent_hop",
    "run_pi_text",
    "tail_truncate",
    "text_from_content",
    "truncate_output",
    "turn_has_content",
    "wait_for_rate_limit",
]
