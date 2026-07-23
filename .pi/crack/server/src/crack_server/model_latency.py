"""Per-model moving-average latency (EMA), persisted under harness/.

Updated asynchronously after each hop via :func:`record_latency`; render paths
read the cache with :func:`latencies` (tolerant of a missing/corrupt file).
"""

from __future__ import annotations

import asyncio

from crack_server import paths

_MIN_SECONDS = 0.1
_MAX_SECONDS = 400.0
_EMA_ALPHA = 0.1

_lock = asyncio.Lock()


def _clamp(seconds: float) -> float:
    return max(_MIN_SECONDS, min(_MAX_SECONDS, float(seconds)))


async def record_latency(model: str, seconds: float) -> None:
    """Update the EMA for ``model`` with a clamped sample (async + file-locked)."""
    if not model:
        return
    clamped = _clamp(seconds)

    def _update(data: dict) -> dict:
        old = data.get(model)
        if old is None:
            data[model] = clamped
        else:
            data[model] = float(old) * (1.0 - _EMA_ALPHA) + clamped * _EMA_ALPHA
        return data

    async with _lock:
        await paths.model_latency_state().aupdate(_update)


def latencies() -> dict[str, float]:
    """Sync read of ``{model_id: avg_seconds}`` (``{}`` when missing/corrupt)."""
    raw = paths.model_latency_state().read()
    out: dict[str, float] = {}
    for key, val in raw.items():
        try:
            out[str(key)] = float(val)
        except (TypeError, ValueError):
            continue
    return out
