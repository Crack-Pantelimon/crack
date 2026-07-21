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


def _parse_count(raw: str) -> int | None:
    """Parse a pi size cell like ``200K`` / ``1M`` / ``131.1K`` into an int of
    tokens. Returns None for blanks or unparseable values."""
    s = (raw or "").strip()
    if not s:
        return None
    mult = 1
    if s[-1] in "kKmMgG":
        mult = {"k": 1_000, "m": 1_000_000, "g": 1_000_000_000}[s[-1].lower()]
        s = s[:-1]
    try:
        return int(round(float(s) * mult))
    except ValueError:
        return None


def _fetch_models() -> dict:
    """Run `pi --list-models` and parse its whitespace-column table into rich
    per-model metadata.

    Columns: ``provider  model  context  max-out  thinking  images``. The model
    column may or may not already carry the provider prefix (nvidia's does,
    google's doesn't), so only prepend the provider when missing. We keep the
    raw column cells verbatim (``raw_columns``) so no pi-provided datum is lost,
    plus parsed ``context_tokens`` / ``max_out_tokens`` for the context meter.

    Returns ``{"models": [id...], "info": {id: {...}}}``.

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

    info: dict[str, dict] = {}
    for line in result.stdout.splitlines():
        parts = line.split()
        if len(parts) < 2 or parts[0] == "provider":
            continue
        provider, model = parts[0], parts[1]
        full = model if model.startswith(provider + "/") else f"{provider}/{model}"
        context = parts[2] if len(parts) > 2 else ""
        max_out = parts[3] if len(parts) > 3 else ""
        thinking = parts[4] if len(parts) > 4 else ""
        images = parts[5] if len(parts) > 5 else ""
        info[full] = {
            "provider": provider,
            "context": context,
            "context_tokens": _parse_count(context),
            "max_out": max_out,
            "max_out_tokens": _parse_count(max_out),
            "thinking": thinking.lower() == "yes",
            "images": images.lower() == "yes",
            "raw_columns": parts,
        }
    if not info:
        raise RuntimeError("pi --list-models produced no parseable rows")
    return {"models": sorted(info), "info": info}


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


def model_info(model: str) -> dict | None:
    """Cached pi metadata for one model id (context window etc.), or None."""
    cache = paths.models_cache_state().read()
    info = cache.get("info") or {}
    return info.get(model)


def context_window(model: str) -> int | None:
    """Cached context-window token count for a model, or None when unknown."""
    entry = model_info(model)
    if not entry:
        return None
    return entry.get("context_tokens")


def refresh_models() -> None:
    """Worker side of a ``MODELS_JOB_SLUG`` job: fetch `pi --list-models` and
    rewrite the cache. A fetch failure keeps the stale cache/fallback (logged,
    never raised — the queue job always completes)."""
    try:
        fetched = _fetch_models()
    except Exception as e:
        logger.warning("models: refresh failed (%s); keeping stale cache/fallback", e)
        return
    paths.models_cache_state().write({
        "fetched_at": time.time(),
        "models": fetched["models"],
        "info": fetched["info"],
    })
    logger.info("models: cache refreshed (%d models)", len(fetched["models"]))
