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

    Stage ``_run_*`` methods record their own detailed error state; here we only
    guarantee the job is logged and dequeued so a failure never wedges the queue.
    """
    from crack_server import app, chats, queue, stages

    slug = job.get("slug")
    step = job.get("step")
    task_id = job.get("task_id")
    form = job.get("form")
    try:
        if slug == app.TITLE_JOB_SLUG:
            app._run_title_regen_worker(task_id)
        elif slug == chats.CHAT_JOB_SLUG:
            chats.run_chat(task_id)
        else:
            stage = stages.get(slug)
            if stage is None:
                logger.error("worker: unknown stage slug %r for job %s", slug, job.get("id"))
            else:
                stage.run_step(task_id, step, form)
        queue.complete(job)
    except Exception:
        logger.exception("worker: job %s (%s/%s) failed", job.get("id"), slug, step)
        queue.fail(job)


def _loop() -> None:
    """Claim and dispatch jobs forever, up to MAX_WORKERS at a time."""
    from crack_server import queue

    logger.info("crack-worker: starting (max_workers=%d)", MAX_WORKERS)
    queue.reclaim_orphans()

    executor = ThreadPoolExecutor(max_workers=MAX_WORKERS)
    in_flight: set[Future] = set()
    try:
        while True:
            in_flight = {f for f in in_flight if not f.done()}
            while len(in_flight) < MAX_WORKERS:
                job = queue.claim_next()
                if job is None:
                    break
                in_flight.add(executor.submit(_dispatch, job))
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
