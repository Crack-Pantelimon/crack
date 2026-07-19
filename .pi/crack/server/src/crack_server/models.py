"""Available pi models, cached from `pi --list-models` into harness/models_list.json.

The cache is refreshed when missing, older than 24h, or force=True. On fetch
failure a stale cache is kept; with no cache at all we fall back to the two
model ids the harness ships with.
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


def _fetch_models() -> list[str]:
    """Run `pi --list-models` and parse its whitespace-column table.

    Rows look like `nvidia  nvidia/nemotron-3-nano-30b-a3b  131.1K ...` — the model
    column may or may not already carry the provider prefix (nvidia's does,
    google's doesn't), so only prepend the provider when missing.

    Retried twice quickly on failure (transient network / provider hiccups)."""
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


def get_models(force: bool = False) -> list[str]:
    """Return the cached model list, refetching when stale (>24h) or forced."""
    cache = paths.read_models_cache()
    fetched_at = float(cache.get("fetched_at", 0) or 0)
    fresh = bool(cache.get("models")) and (time.time() - fetched_at) < MAX_AGE_SECONDS

    if force or not fresh:
        try:
            models = _fetch_models()
            paths.write_models_cache({"fetched_at": time.time(), "models": models})
            return models
        except Exception as e:
            logger.warning("models: fetch failed (%s); keeping stale cache/fallback", e)

    cached = cache.get("models")
    if cached:
        return sorted(str(m) for m in cached)
    return list(FALLBACK_MODELS)
