"""Filesystem job queue for the crack worker (no external infra dependency).

Jobs live as JSON files under ``.pi/crack/harness/queue/`` with two subdirs:

- ``pending/``    — enqueued, not yet claimed. Filenames sort chronologically
  (``<ms_epoch>_<uuid8>.json``) so the oldest job is always claimed first.
- ``processing/`` — claimed by the worker, in flight. On a clean completion the
  file is removed; on a crash/restart it is reclaimed back to ``pending/``.

The queue is process- and thread-safe by construction: claiming a job is a
single ``os.rename`` (atomic within a filesystem), so two racing claimers can
never both win the same file — the loser gets ``FileNotFoundError`` and moves on.

Job spec (dict, persisted as the file body)::

    {"id", "task_id", "slug", "step", "form": {...} | None, "enqueued_at", "run_id"?}

The in-memory job dict additionally carries ``_path`` (the processing-file path)
so ``complete`` / ``fail`` can remove the right file.

Dedupe key for exclusive enqueue is ``(task_id, slug, run_id)`` where a missing
``run_id`` matches only other jobs with no ``run_id`` (backward compatible with
stage and chat jobs).
"""

from __future__ import annotations

import json
import logging
import os
import time
import uuid
from pathlib import Path

from crack_server import paths

logger = logging.getLogger("uvicorn.error")

# Processing files older than this (seconds) are treated as orphaned by a crashed
# or restarted worker and reclaimed back to pending on startup.
ORPHAN_THRESHOLD_SECONDS = 5.0

# In-process wakeup hooks (single merged server/worker process): the worker's
# async loop registers a callback here so an enqueue wakes dispatch immediately
# instead of waiting out the poll interval. Best-effort: exceptions in a
# callback never break the enqueue.
_WAKEUP_CALLBACKS: list = []


def register_wakeup(callback) -> None:
    """Register a no-arg callback fired after every successful enqueue."""
    _WAKEUP_CALLBACKS.append(callback)


def _notify_enqueued() -> None:
    for callback in _WAKEUP_CALLBACKS:
        try:
            callback()
        except Exception:
            logger.exception("queue: wakeup callback raised")


def _ensure_dirs() -> tuple[Path, Path]:
    pending = paths.queue_pending_dir()
    processing = paths.queue_processing_dir()
    pending.mkdir(parents=True, exist_ok=True)
    processing.mkdir(parents=True, exist_ok=True)
    return pending, processing


def enqueue(
    task_id: str,
    slug: str,
    step: str,
    form: dict | None = None,
    run_id: str | None = None,
) -> str:
    """Write a job into pending/ atomically. Returns the job id."""
    pending, _ = _ensure_dirs()
    job_id = f"{int(time.time() * 1000)}_{uuid.uuid4().hex[:8]}"
    job = {
        "id": job_id,
        "task_id": task_id,
        "slug": slug,
        "step": step,
        "form": form,
        "enqueued_at": time.time(),
    }
    if run_id is not None:
        job["run_id"] = run_id
    path = pending / f"{job_id}.json"
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(job, indent=2), encoding="utf-8")
    os.replace(tmp, path)
    logger.info("queue: enqueued %s (%s/%s) for %s", job_id, slug, step, task_id)
    _notify_enqueued()
    return job_id


def _job_matches(job: dict, task_id: str, slug: str, run_id: str | None) -> bool:
    if job.get("task_id") != task_id or job.get("slug") != slug:
        return False
    job_run_id = job.get("run_id")
    if run_id is None:
        return job_run_id is None
    return job_run_id == run_id


def _find_job(
    task_id: str,
    slug: str,
    run_id: str | None = None,
    ignore_job_id: str | None = None,
) -> tuple[dict, str] | None:
    """Scan pending/ + processing/ for a job matching (task_id, slug, run_id),
    skipping ``ignore_job_id``. Returns (job, directory name) or None."""
    pending, processing = _ensure_dirs()
    for directory in (pending, processing):
        for path in directory.glob("*.json"):
            try:
                job = json.loads(path.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, OSError):
                continue
            if not _job_matches(job, task_id, slug, run_id):
                continue
            if ignore_job_id is not None and job.get("id") == ignore_job_id:
                continue
            return job, directory.name
    return None


def has_job(task_id: str, slug: str, run_id: str | None = None) -> bool:
    """True when any job for (task_id, slug, run_id) is pending or in flight."""
    return _find_job(task_id, slug, run_id) is not None


def enqueue_exclusive(
    task_id: str,
    slug: str,
    step: str,
    form: dict | None = None,
    ignore_job_id: str | None = None,
    run_id: str | None = None,
) -> str | None:
    """Enqueue unless a job for the same (task_id, slug, run_id) is already
    pending or in flight — the double-run guard (B1). Returns the job id, or
    None when the duplicate was dropped."""
    existing = _find_job(task_id, slug, run_id, ignore_job_id)
    if existing is not None:
        job, dirname = existing
        logger.info(
            "queue: dropping duplicate %s/%s for %s (job %s already %s)",
            slug, step, task_id, job.get("id"), dirname,
        )
        return None
    return enqueue(task_id, slug, step, form, run_id=run_id)


def claim_next() -> dict | None:
    """Atomically claim the oldest pending job into processing/.

    Returns the job dict (with ``_path`` set) or None if the queue is empty.
    Racing claimers that lose a rename simply skip to the next candidate.
    """
    pending, processing = _ensure_dirs()
    candidates = sorted(p for p in pending.glob("*.json") if not p.name.endswith(".tmp"))
    for src in candidates:
        dst = processing / src.name
        try:
            os.rename(src, dst)
        except (FileNotFoundError, OSError):
            continue  # another claimer won it, or it vanished; try the next
        try:
            job = json.loads(dst.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError) as e:
            logger.error("queue: corrupt job file %s: %s; discarding", dst.name, e)
            dst.unlink(missing_ok=True)
            continue
        job["_path"] = str(dst)
        logger.info("queue: claimed %s (%s/%s)", job.get("id"), job.get("slug"), job.get("step"))
        return job
    return None


def _remove_processing(job: dict) -> None:
    path = job.get("_path")
    if path:
        Path(path).unlink(missing_ok=True)


def complete(job: dict) -> None:
    """Remove a job's processing file after a successful run."""
    _remove_processing(job)


def fail(job: dict) -> None:
    """Remove a job's processing file after a failed run (error already recorded
    on the stage's own state; we do not retry to avoid poison-job loops)."""
    _remove_processing(job)


def reclaim_orphans(threshold_seconds: float = ORPHAN_THRESHOLD_SECONDS) -> int:
    """Move stale processing/* files back to pending/ (reentrancy after a crash
    or watchfiles restart). Returns the number reclaimed."""
    pending, processing = _ensure_dirs()
    now = time.time()
    reclaimed = 0
    for path in processing.glob("*.json"):
        try:
            age = now - path.stat().st_mtime
        except OSError:
            continue
        if age < threshold_seconds:
            continue
        try:
            os.rename(path, pending / path.name)
            reclaimed += 1
        except (FileNotFoundError, OSError) as e:
            logger.warning("queue: could not reclaim %s: %s", path.name, e)
    if reclaimed:
        logger.info("queue: reclaimed %d orphaned job(s)", reclaimed)
    return reclaimed
