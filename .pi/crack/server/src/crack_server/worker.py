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

MAX_WORKERS = 24
POLL_INTERVAL_SECONDS = 0.5


def _dispatch(job: dict) -> None:
    """Run one claimed job, then remove its processing file (complete/fail).

    Stage ``_run_*`` methods record their own detailed error state; here we
    guarantee the job is logged and dequeued so a failure never wedges the
    queue, and — for exceptions that escaped the stage's own handling — that
    the stage's state lands in ``error`` so the UI never spins forever (B6).
    """
    from crack_server import app, chats, models as models_mod, paths, queue, stages
    from crack_server.sub_agents import constants as sub_constants
    from crack_server.sub_agents import registry as sub_agents_registry

    slug = job.get("slug")
    step = job.get("step")
    task_id = job.get("task_id")
    form = job.get("form")
    run_id = job.get("run_id") or (form or {}).get("run_id")
    persona = None
    try:
        stage = None
        successor: tuple | None = None
        if slug == app.TITLE_JOB_SLUG:
            app._run_title_regen_worker(task_id)
        elif slug == models_mod.MODELS_JOB_SLUG:
            models_mod.refresh_models()
        elif slug == chats.CHAT_JOB_SLUG:
            chats.run_chat(task_id)
        elif slug == sub_constants.SUBAGENT_JOB_SLUG:
            if not run_id:
                logger.error("worker: sub-agent job %s missing run_id", job.get("id"))
            else:
                run_state = paths.run_state_by_id(run_id).read()
                persona_slug = run_state.get("persona", "")
                persona = sub_agents_registry.get(persona_slug)
                if persona is None:
                    logger.error(
                        "worker: unknown persona %r for run %s", persona_slug, run_id
                    )
                else:
                    successor = persona.dispatch_step(run_id, step, form)
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
        if slug == chats.CHAT_JOB_SLUG:
            # Race guard: a child finish may have appended to child_inbox while
            # this job was still in processing (exclusive enqueue dropped). With
            # the processing file gone, re-enqueue if work remains.
            chat_state = paths.chat_state(task_id).read()
            if chat_state.get("pending") or chat_state.get("child_inbox"):
                def _reopen(s: dict) -> dict:
                    s["phase"] = "chatting"
                    return s

                paths.chat_state(task_id).update(_reopen)
                queue.enqueue_exclusive(task_id, chats.CHAT_JOB_SLUG, "chat")
        if persona is not None and successor is not None:
            next_step, next_form = successor
            persona.enqueue_step(run_id, next_step, next_form, ignore_job_id=job.get("id"))
        elif stage is not None and successor is not None:
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
            elif slug == sub_constants.SUBAGENT_JOB_SLUG and run_id:
                persona_slug = paths.run_state_by_id(run_id).read().get("persona", "")
                persona = sub_agents_registry.get(persona_slug)
                if persona is not None:
                    persona.record_dispatch_error(run_id, str(exc))
                else:
                    from crack_server.sub_agents import runner

                    runner.finish(run_id, "error")
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
        pid_files += list(chats_dir.glob("*/sub_agent_runs/*/agent.pid"))
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
        for sessions_dir in chats_dir.glob("*/sub_agent_runs/*/sessions"):
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
    """RC6 reconciliation: flag stuck running phases with no queued job."""
    from crack_server import paths, stages
    from crack_server.sub_agents import registry as sub_agents_registry

    _RUN_TERMINAL = {"done", "error", "stopped", "awaiting_answers"}

    try:
        task_ids = paths.list_task_ids()
    except OSError:
        task_ids = []
    for task_id in task_ids:
        for stage in stages.REGISTRY:
            try:
                stage.check_orphaned(task_id)
            except Exception:
                logger.exception(
                    "crack-worker: orphan check failed for %s/%s", task_id, stage.slug
                )

    try:
        chat_ids = paths.list_chat_ids()
    except OSError:
        chat_ids = []
    for chat_id in chat_ids:
        for run_id in paths.list_run_ids(chat_id):
            try:
                state = paths.run_state(chat_id, run_id).read()
            except OSError:
                continue
            if state.get("phase") in _RUN_TERMINAL:
                continue
            persona = sub_agents_registry.get(state.get("persona", ""))
            if persona is None:
                continue
            try:
                persona.check_orphaned(run_id)
            except Exception:
                logger.exception("crack-worker: orphan check failed for run %s", run_id)


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
