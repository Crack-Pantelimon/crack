"""Rate limiting and retry scheduling for `pi` subprocess calls.

Split out of pi_runner.py (A6): the thread-safe minimum-interval limiter, the
provider/model limiter registry, and the retry-offset helpers shared by
run_pi_text and run_agent_hop. Everything here logs through the uvicorn logger
and is only ever called from background threads.
"""

from __future__ import annotations

import logging
import threading
import time

logger = logging.getLogger("uvicorn.error")

# The title model is hosted behind the nvidia provider, so it shares the
# nvidia-wide 40 calls/minute budget; it additionally has its own tighter
# 30 calls/minute budget and a ~4k-token (~10,000 char) input limit.
TITLE_MODEL = "nvidia/nemotron-3-nano-30b-a3b"

PI_TIMEOUT_SECONDS = 120

NVIDIA_CALLS_PER_MINUTE = 40
TITLE_CALLS_PER_MINUTE = 30
TITLE_MAX_INPUT_CHARS = 10_000

# Every pi invocation is retried on a hard process failure (nonzero exit,
# SIGKILL/-9 OOM, timeout, missing binary). We make at least PI_RETRY_ATTEMPTS
# attempts spaced by exponential backoff, anchored so the final attempt begins
# exactly PI_RETRY_WINDOW_SECONDS after the first attempt started.
PI_RETRY_ATTEMPTS = 4
PI_RETRY_WINDOW_SECONDS = 61.0

# Transient upstream failures (rate limits, 5xx, connection resets) get their
# own longer, flatter backoff that crosses the provider's per-minute window
# before giving up: one sleep per reattempt, in order.
TRANSIENT_RETRY_DELAYS = [20.0, 45.0, 75.0]

_TRANSIENT_MARKERS = (
    "resourceexhausted",
    "429",
    "rate limit",
    "overloaded",
    "temporarily",
    "503",
    "502",
    "504",
    "connection reset",
    "connection refused",
    "etimedout",
)

# Continuation message used when a failure interrupted a hop that had already
# persisted turns: the pi session dir is preserved, so the next attempt resumes
# the session instead of replaying the original message.
RESUME_MESSAGE = "Continue where you left off."


def is_transient(text: str) -> bool:
    """True when a pi failure's captured stdout/stderr tail looks like a
    transient upstream error (rate limit / overload / connectivity) that is
    worth resuming rather than surfacing."""
    low = (text or "").lower()
    return any(marker in low for marker in _TRANSIENT_MARKERS)


def _transient_backoff_sleep(reattempt: int) -> None:
    """Sleep before transient reattempt ``reattempt`` (0-based)."""
    delay = TRANSIENT_RETRY_DELAYS[min(reattempt, len(TRANSIENT_RETRY_DELAYS) - 1)]
    if delay > 0:
        logger.info(
            "pi-retry: transient failure; sleeping %.1fs before reattempt %d",
            delay, reattempt + 1,
        )
        time.sleep(delay)


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


class RateLimiter:
    """Thread-safe minimum-interval limiter: converts a calls/minute budget into
    a minimum spacing between calls. Waiting reserves the next free slot under
    the lock and sleeps *outside* it, so concurrent callers queue up their slots
    instantly instead of serializing on the lock across sleeps."""

    def __init__(self, name: str, calls_per_minute: float) -> None:
        self._name = name
        self._min_interval = 60.0 / calls_per_minute
        self._lock = threading.Lock()
        self._next_free = 0.0

    def wait(self) -> None:
        with self._lock:
            now = time.monotonic()
            slot = max(now, self._next_free)
            self._next_free = slot + self._min_interval
        delay = slot - now
        if delay > 0:
            logger.info("rate-limit(%s): sleeping %.2fs for reserved slot", self._name, delay)
            time.sleep(delay)


# Limiters are keyed by provider (the shared per-provider budget) and by model id
# (a tighter per-model budget on top). Only nvidia-hosted models are rate-limited;
# other providers make back-to-back calls.
_limiters_lock = threading.Lock()
_provider_limiters: dict[str, RateLimiter] = {}
_model_limiters: dict[str, RateLimiter] = {
    TITLE_MODEL: RateLimiter(f"model:{TITLE_MODEL}", TITLE_CALLS_PER_MINUTE),
}


def limiter_for(model: str) -> RateLimiter | None:
    """The shared provider limiter for ``model``, or None when its provider has
    no known budget (created lazily so future providers slot in trivially)."""
    provider = model.split("/", 1)[0]
    if provider != "nvidia":
        return None
    with _limiters_lock:
        limiter = _provider_limiters.get(provider)
        if limiter is None:
            limiter = RateLimiter(f"provider:{provider}", NVIDIA_CALLS_PER_MINUTE)
            _provider_limiters[provider] = limiter
    return limiter


def wait_for_rate_limit(model: str) -> None:
    # Both limiters are deficit-only: each tracks its own schedule and sleeps
    # just the remainder of its own interval, so running them sequentially does
    # not double-count — time spent waiting on the shared provider budget also
    # counts toward the per-model interval.
    limiter = limiter_for(model)
    if limiter is not None:
        limiter.wait()
    model_limiter = _model_limiters.get(model)
    if model_limiter is not None:
        model_limiter.wait()
