from __future__ import annotations

from contextlib import asynccontextmanager
from pathlib import Path

from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from crack_server import (
    routes_chats,
    routes_settings,
    routes_stages,
    routes_sub_agents,
    routes_tasks,
    routes_vision,
    worker,
)

# Re-exported for worker.py, which dispatches queue jobs via these names.
from crack_server.routes_tasks import TITLE_JOB_SLUG, _run_title_regen_worker  # noqa: F401

STATIC_DIR = Path(__file__).resolve().parent / "static"


@asynccontextmanager
async def lifespan(_app: FastAPI):
    """The queue worker runs in-process (one asyncio process, no second
    console script): start it with the app, cancel it on shutdown."""
    import asyncio as _asyncio

    from crack_server.sub_agents import signals as _signals

    _signals.bind_loop(_asyncio.get_running_loop())
    worker_task = worker.start_background()
    try:
        yield
    finally:
        await worker.stop_background(worker_task)


app = FastAPI(title="crack-pi-server", lifespan=lifespan)
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

app.include_router(routes_tasks.router)
app.include_router(routes_stages.router)
app.include_router(routes_chats.router)
app.include_router(routes_sub_agents.router)
app.include_router(routes_vision.router)
app.include_router(routes_settings.router)
