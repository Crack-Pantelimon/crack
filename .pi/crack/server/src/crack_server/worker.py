"""Out-of-process worker that drains the on-disk command queue.

All ``pi`` execution lives here, not in the web process: routes only write fast
state and enqueue jobs (see ``queue.py`` + ``Stage.enqueue_step``). The worker
claims jobs and dispatches them to ``Stage.run_step`` (or the title-job handler),
running up to ``MAX_WORKERS`` concurrently so multiple tasks interleave while
sharing the process-global ``pi_runner`` rate limiter.

Reentrancy: ``main()`` runs ``_loop`` under ``watchfiles.run_process`` (mirroring
uvicorn ``reload=True``), so editing worker/stage source auto-restarts it; each
restart calls ``queue.reclaim_orphans()`` to requeue any job that was in flight.
Single-instance is enforced by the flock in ``_docker/_cont_start.sh``.
"""

from __future__ import annotations

import logging
import time
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
logger = logging.getLogger("uvicorn.error")

MAX_WORKERS = 4
POLL_INTERVAL_SECONDS = 0.5


def _dispatch(job: dict) -> None:
    """Run one claimed job, then remove its processing file (complete/fail).

    Stage ``_run_*`` methods record their own detailed error state; here we
    guarantee the job is logged and dequeued so a failure never wedges the
    queue, and — for exceptions that escaped the stage's own handling — that
    the stage's state lands in ``error`` so the UI never spins forever (B6).
    """
    from crack_server import app, chats, models as models_mod, paths, queue, stages

    slug = job.get("slug")
    step = job.get("step")
    task_id = job.get("task_id")
    form = job.get("form")
    try:
        stage = None
        successor: tuple | None = None
        if slug == app.TITLE_JOB_SLUG:
            app._run_title_regen_worker(task_id)
        elif slug == models_mod.MODELS_JOB_SLUG:
            models_mod.refresh_models()
        elif slug == chats.CHAT_JOB_SLUG:
            chats.run_chat(task_id)
        else:
            stage = stages.get(slug)
            if stage is None:
                logger.error("worker: unknown stage slug %r for job %s", slug, job.get("id"))
            else:
                successor = stage.dispatch_step(task_id, step, form)
        # Complete first, then enqueue the step's successor: with the job's
        # processing file gone, the B1 exclusive guard can't mistake the chain
        # for a double-run (RC1). A crash in this gap loses the successor; the
        # orphan-phase watchdog turns that into a visible error, not a spinner.
        queue.complete(job)
        if stage is not None and successor is not None:
            next_step, next_form = successor
            stage.enqueue_step(task_id, next_step, next_form, ignore_job_id=job.get("id"))
    except Exception as exc:
        logger.exception("worker: job %s (%s/%s) failed", job.get("id"), slug, step)
        queue.fail(job)
        try:
            detail = f"worker dispatch failed: {exc}"
            if slug == app.TITLE_JOB_SLUG:
                paths.title_regen_state(task_id).write({"status": "error", "error": detail})
            elif slug == chats.CHAT_JOB_SLUG:
                def _fail(state: dict) -> dict:
                    state["phase"] = "idle"
                    state["error"] = detail
                    state["error_detail"] = ""
                    return state

                paths.chat_state(task_id).update(_fail)
            else:
                stage = stages.get(slug)
                if stage is not None:
                    stage.record_dispatch_error(task_id, str(exc))
        except Exception:
            logger.exception("worker: could not record dispatch error for job %s", job.get("id"))


def _kill_orphaned_agents() -> None:
    """Kill pi process groups left behind by a crashed/killed worker (B4): any
    surviving *.agent.pid under tasks/ or unscripted_chats/ names an agent whose
    owning job is gone, so it must die before its job is reclaimed and re-run."""
    from crack_server import paths, pi_runner

    pid_files: list[Path] = []
    tasks = paths.tasks_dir()
    if tasks.is_dir():
        pid_files += list(tasks.glob("*/*.agent.pid"))
    chats_dir = paths.unscripted_chats_dir()
    if chats_dir.is_dir():
        pid_files += list(chats_dir.glob("*/agent.pid"))
    for pid_file in pid_files:
        killed = pi_runner.kill_pid_file(pid_file)
        logger.info("crack-worker: orphaned agent pid file %s (killed=%s)", pid_file, killed)
        try:
            pid_file.unlink()
        except OSError:
            pass


SESSION_RETENTION_DAYS = 14

# State phases that mean "nothing is running here"; anything else (running,
# resuming, chatting, drafting, …) makes the owner off-limits to the janitor.
_TERMINAL_PHASES = {"idle", "done", "error", "stopped"}


def _owner_is_active(owner_dir: Path) -> bool:
    """True if any JSON state file in the task/chat dir reports a live phase."""
    import json

    for state_file in owner_dir.glob("*.json"):
        try:
            data = json.loads(state_file.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        for key in ("status", "phase"):
            value = str(data.get(key, "")).strip().lower()
            if value and value not in _TERMINAL_PHASES:
                return True
    return False


def _newest_mtime(directory: Path) -> float:
    """Newest mtime inside a session dir (file appends don't touch the dir mtime)."""
    latest = directory.stat().st_mtime
    for path in directory.rglob("*"):
        try:
            latest = max(latest, path.stat().st_mtime)
        except OSError:
            continue
    return latest


def _prune_old_session_dirs() -> None:
    """B23: delete pi session dirs idle for more than SESSION_RETENTION_DAYS.

    Candidates are ``…/tasks/<id>/<stage>/(sessions|review_sessions)/`` and
    ``…/unscripted_chats/<id>/sessions/``. A candidate is removed only when
    its owning task/chat has no live stage/chat phase AND nothing inside the
    dir was touched within the retention window — a running pi appends to its
    session JSONL continuously, so active dirs are always fresh.
    """
    import shutil

    from crack_server import paths

    candidates: list[tuple[Path, Path]] = []  # (sessions_dir, owner_dir)
    tasks = paths.tasks_dir()
    if tasks.is_dir():
        for pattern in ("*/*/sessions", "*/*/review_sessions"):
            for sessions_dir in tasks.glob(pattern):
                if sessions_dir.is_dir():
                    candidates.append((sessions_dir, sessions_dir.parent.parent))
    chats_dir = paths.unscripted_chats_dir()
    if chats_dir.is_dir():
        for sessions_dir in chats_dir.glob("*/sessions"):
            if sessions_dir.is_dir():
                candidates.append((sessions_dir, sessions_dir.parent))

    for sessions_dir, owner_dir in candidates:
        if _owner_is_active(owner_dir):
            continue
        age_days = (time.time() - _newest_mtime(sessions_dir)) / 86400
        if age_days <= SESSION_RETENTION_DAYS:
            continue
        shutil.rmtree(sessions_dir, ignore_errors=True)
        logger.info(
            "crack-worker: pruned idle session dir %s (idle %.1f days)",
            sessions_dir, age_days,
        )


ORPHAN_SWEEP_INTERVAL_SECONDS = 30.0


def _sweep_orphaned_phases() -> None:
    """RC6 reconciliation: flag any stage stuck in a running phase with no
    pending/processing job (see Stage.check_orphaned), so a lost job surfaces
    as an error even when nobody is watching the task page."""
    from crack_server import paths, stages

    try:
        task_ids = paths.list_task_ids()
    except OSError:
        return
    for task_id in task_ids:
        for stage in stages.REGISTRY:
            try:
                stage.check_orphaned(task_id)
            except Exception:
                logger.exception(
                    "crack-worker: orphan check failed for %s/%s", task_id, stage.slug
                )


def _loop() -> None:
    """Claim and dispatch jobs forever, up to MAX_WORKERS at a time."""
    from crack_server import queue

    logger.info("crack-worker: starting (max_workers=%d)", MAX_WORKERS)
    _kill_orphaned_agents()
    _prune_old_session_dirs()
    queue.reclaim_orphans()

    executor = ThreadPoolExecutor(max_workers=MAX_WORKERS)
    in_flight: set[Future] = set()
    last_sweep = time.monotonic()
    try:
        while True:
            in_flight = {f for f in in_flight if not f.done()}
            while len(in_flight) < MAX_WORKERS:
                job = queue.claim_next()
                if job is None:
                    break
                in_flight.add(executor.submit(_dispatch, job))
            if time.monotonic() - last_sweep > ORPHAN_SWEEP_INTERVAL_SECONDS:
                last_sweep = time.monotonic()
                _sweep_orphaned_phases()
            time.sleep(POLL_INTERVAL_SECONDS)
    finally:
        executor.shutdown(wait=False)


def main() -> None:
    """Console-script entrypoint: run _loop under watchfiles for auto-reload."""
    from watchfiles import run_process

    pkg_dir = Path(__file__).resolve().parent
    run_process(str(pkg_dir), target=_loop)


if __name__ == "__main__":
    main()
