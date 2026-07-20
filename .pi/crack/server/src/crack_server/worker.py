"""In-process async worker that drains the on-disk command queue.

The worker runs inside the FastAPI server process (started from the app
lifespan): routes only write fast state and enqueue jobs (see ``queue.py`` +
``Stage.enqueue_step``); the worker claims jobs and dispatches them as
``asyncio`` tasks, so a waiting hop costs a coroutine instead of a thread and
there is no concurrency cap — the process-global ``pi_runner`` rate limiter
remains the LLM-pressure guard. Sub-agent and chat jobs run their (fully
async) dispatch chain in the loop; legacy sync stage/title/models jobs are
wrapped in ``asyncio.to_thread``.

Reentrancy: uvicorn ``reload=True`` restarts the whole process on source
edits; each start calls ``queue.reclaim_orphans()`` to requeue any job that
was in flight, and ``recover_detached_hops()`` leaves pi processes that
survived the reload running (their jobs re-attach via the hop manifest)
instead of killing them.
"""

from __future__ import annotations

import asyncio
import contextlib
import logging
import signal
import time
from pathlib import Path

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
logger = logging.getLogger("uvicorn.error")

POLL_INTERVAL_SECONDS = 0.5

# Set by async_loop while it runs: queue enqueue wakeups are routed here.
_WAKEUP: asyncio.Event | None = None


async def _dispatch(job: dict) -> None:
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
            await asyncio.to_thread(app._run_title_regen_worker, task_id)
        elif slug == models_mod.MODELS_JOB_SLUG:
            await asyncio.to_thread(models_mod.refresh_models)
        elif slug == chats.CHAT_JOB_SLUG:
            await chats.run_chat(task_id)
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
                    successor = await persona.dispatch_step(run_id, step, form)
        else:
            stage = stages.get(slug)
            if stage is None:
                logger.error("worker: unknown stage slug %r for job %s", slug, job.get("id"))
            else:
                # Sync stage machinery (its own sync pi_runner wrappers): keep
                # it off the event loop.
                successor = await asyncio.to_thread(
                    stage.dispatch_step, task_id, step, form
                )
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


# Grace added to a hop manifest's own timeout when deciding whether a
# detached pi is stale (never re-attached, e.g. a lost queue file) and may be
# killed/cleaned at startup.
DETACHED_HOP_GRACE_SECONDS = 120.0


def recover_detached_hops() -> None:
    """Reload survival (replaces the old kill-orphans sweep): a pi detached by
    a server reload keeps running (tini reaps it) and the reclaimed job
    re-attaches. For each hop manifest next to a pid file:

    - pid alive and within its timeout budget → leave it running;
    - pid alive but long past its budget (never re-attached) → kill the
      process group and clean up;
    - pid dead, manifest fresh → leave it: the resumed job drains the output
      backlog (a pi that finished mid-restart completes from the file);
    - pid dead and stale → clean up.
    Stale pid files with no manifest keep the old behavior (kill + unlink).
    """
    from crack_server import paths, pi_runner
    from crack_server.pi_proc import _pid_alive, _read_hop_manifest, _terminate_group

    pid_files: list[Path] = []
    tasks = paths.tasks_dir()
    if tasks.is_dir():
        pid_files += list(tasks.glob("*/*.agent.pid"))
    chats_dir = paths.unscripted_chats_dir()
    if chats_dir.is_dir():
        pid_files += list(chats_dir.glob("*/agent.pid"))
        pid_files += list(chats_dir.glob("*/sub_agent_runs/*/agent.pid"))

    for pid_file in pid_files:
        manifest_path = paths.hop_manifest_path(pid_file)
        output_path = paths.hop_output_path(pid_file)
        manifest = _read_hop_manifest(manifest_path)
        pid = manifest.get("pid")
        if manifest.get("status") == "running" and isinstance(pid, int):
            alive = _pid_alive(pid, str(manifest.get("session_id") or "") or None)
            budget = float(manifest.get("timeout") or 0) + DETACHED_HOP_GRACE_SECONDS
            stale = time.time() - float(manifest.get("started_at") or 0) > budget
            if alive and not stale:
                logger.info(
                    "crack-worker: detached hop %s still running (pid %d); leaving for re-attach",
                    manifest_path, pid,
                )
                continue
            if alive:
                logger.warning(
                    "crack-worker: detached hop %s stale (pid %d); killing group",
                    manifest_path, pid,
                )
                _terminate_group(pid, signal.SIGKILL)
            elif not stale:
                logger.info(
                    "crack-worker: detached hop %s ended during restart; leaving for drain",
                    manifest_path,
                )
                continue
            for path in (pid_file, manifest_path, output_path):
                path.unlink(missing_ok=True)
            continue
        # No live detached hop: legacy orphan behavior (B4).
        killed = pi_runner.kill_pid_file(pid_file)
        logger.info("crack-worker: orphaned agent pid file %s (killed=%s)", pid_file, killed)
        for path in (pid_file, manifest_path, output_path):
            path.unlink(missing_ok=True)


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

    _RUN_TERMINAL = {"done", "error", "stopped", "awaiting_answers", "awaiting_user"}

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
            if state.get("waiting_on"):
                # Suspended in a blocking wait_join: no job is *meant* to be
                # queued while the hop waits for children.
                continue
            persona = sub_agents_registry.get(state.get("persona", ""))
            if persona is None:
                continue
            try:
                persona.check_orphaned(run_id)
            except Exception:
                logger.exception("crack-worker: orphan check failed for run %s", run_id)


async def async_loop() -> None:
    """Claim and dispatch jobs forever, one asyncio task per job (no cap)."""
    from crack_server import queue

    global _WAKEUP
    logger.info("crack-worker: starting (async, in-process)")
    loop = asyncio.get_running_loop()
    wakeup = asyncio.Event()
    _WAKEUP = wakeup
    # Enqueues can come from any thread (routes, to_thread stage jobs), so go
    # through call_soon_threadsafe.
    queue.register_wakeup(lambda: loop.call_soon_threadsafe(wakeup.set))

    await asyncio.to_thread(recover_detached_hops)
    await asyncio.to_thread(_prune_old_session_dirs)
    await asyncio.to_thread(queue.reclaim_orphans)

    in_flight: set[asyncio.Task] = set()
    last_sweep = time.monotonic()
    try:
        while True:
            in_flight = {t for t in in_flight if not t.done()}
            while True:
                job = queue.claim_next()
                if job is None:
                    break
                in_flight.add(asyncio.create_task(_dispatch(job)))
            if time.monotonic() - last_sweep > ORPHAN_SWEEP_INTERVAL_SECONDS:
                last_sweep = time.monotonic()
                await asyncio.to_thread(_sweep_orphaned_phases)
            wakeup.clear()
            try:
                await asyncio.wait_for(wakeup.wait(), timeout=POLL_INTERVAL_SECONDS)
            except asyncio.TimeoutError:
                pass
    finally:
        _WAKEUP = None
        for task in in_flight:
            task.cancel()
        if in_flight:
            await asyncio.gather(*in_flight, return_exceptions=True)


def start_background() -> asyncio.Task:
    """Lifespan hook: start the worker loop as a background task."""
    return asyncio.create_task(async_loop(), name="crack-worker")


async def stop_background(task: asyncio.Task) -> None:
    """Lifespan hook: cancel the worker loop and let it reap in-flight jobs."""
    task.cancel()
    with contextlib.suppress(asyncio.CancelledError):
        await task


def main() -> None:
    """Deprecated: the worker now runs inside the server process (app lifespan)."""
    raise SystemExit(
        "crack-worker is retired: the worker runs inside crack-server "
        "(uvicorn app lifespan). Launch crack-server only."
    )


if __name__ == "__main__":
    main()
