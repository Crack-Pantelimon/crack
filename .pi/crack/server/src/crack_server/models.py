"""Available pi models, cached from `pi --list-models` into harness/models_list.json.

Page renders read the cache only (B21): when it is stale (>24h) or missing,
the render path enqueues a ``__models__`` refresh job on the worker queue
(mirroring the chat/sub-agent job pattern) instead of shelling out mid-render
— a page load can never block on the 60s ``pi --list-models`` subprocess. The
worker's refresh writes the cache; on fetch failure the stale cache (or a
two-model fallback list) is kept.
"""

from __future__ import annotations

import logging
import subprocess
import time

from crack_server import paths

logger = logging.getLogger("uvicorn.error")

FALLBACK_MODELS = [
    "nvidia/nemotron-3-nano-30b-a3b",
    "nvidia/nemotron-3-ultra-550b-a55b",
]
MAX_AGE_SECONDS = 24 * 3600
FETCH_TIMEOUT_SECONDS = 60

# Pseudo-stage slug for the models-cache refresh job on the queue (worker.py).
MODELS_JOB_SLUG = "__models__"


def _fetch_models() -> list[str]:
    """Run `pi --list-models` and parse its whitespace-column table.

    Rows look like `nvidia  nvidia/nemotron-3-nano-30b-a3b  131.1K ...` — the model
    column may or may not already carry the provider prefix (nvidia's does,
    google's doesn't), so only prepend the provider when missing.

    Retried twice quickly on failure (transient network / provider hiccups).
    Only ever called from the worker (via refresh_models), never in a render."""
    result = None
    for attempt in range(3):
        if attempt > 0:
            time.sleep(2)
        result = subprocess.run(
            ["pi", "--list-models"],
            capture_output=True,
            text=True,
            timeout=FETCH_TIMEOUT_SECONDS,
        )
        if result.returncode == 0:
            break
        logger.warning(
            "models: pi --list-models exited %d (attempt %d/3)", result.returncode, attempt + 1
        )
    if result is None or result.returncode != 0:
        raise RuntimeError(f"pi --list-models exited {result.returncode}: {result.stderr[:200]}")

    models: set[str] = set()
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) < 2 or parts[0] == "provider":
            continue
        provider, model = parts[0], parts[1]
        full = model if model.startswith(provider + "/") else f"{provider}/{model}"
        models.add(full)
    if not models:
        raise RuntimeError("pi --list-models produced no parseable rows")
    return sorted(models)


def models_for_render(force: bool = False) -> list[str]:
    """Cache-only model list for page renders (B21): never shells out.

    When the cache is stale/missing (or ``force`` is set, e.g. by
    ``GET /api/models?force=true``) a background refresh job is enqueued —
    deduped, so a page full of dropdowns enqueues at most one — and the next
    render sees the fresh list. Callers keep a saved value as an option even
    when it is missing from this list."""
    cache = paths.models_cache_state().read()
    fetched_at = float(cache.get("fetched_at", 0) or 0)
    cached = cache.get("models")
    if force or not cached or (time.time() - fetched_at) >= MAX_AGE_SECONDS:
        _enqueue_refresh()
    if cached:
        return sorted(str(m) for m in cached)
    return list(FALLBACK_MODELS)


def _enqueue_refresh() -> None:
    """Enqueue a __models__ refresh job unless one is already pending/in flight."""
    from crack_server import queue

    queue.enqueue_exclusive("", MODELS_JOB_SLUG, "refresh")


def refresh_models() -> None:
    """Worker side of a ``MODELS_JOB_SLUG`` job: fetch `pi --list-models` and
    rewrite the cache. A fetch failure keeps the stale cache/fallback (logged,
    never raised — the queue job always completes)."""
    try:
        models = _fetch_models()
    except Exception as e:
        logger.warning("models: refresh failed (%s); keeping stale cache/fallback", e)
        return
    paths.models_cache_state().write({"fetched_at": time.time(), "models": models})
    logger.info("models: cache refreshed (%d models)", len(models))
