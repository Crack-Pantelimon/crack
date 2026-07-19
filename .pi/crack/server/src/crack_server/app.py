from __future__ import annotations
from pathlib import Path

from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from crack_server import routes_chats, routes_stages, routes_sub_agents, routes_tasks

# Re-exported for worker.py, which dispatches queue jobs via these names.
from crack_server.routes_tasks import TITLE_JOB_SLUG, _run_title_regen_worker  # noqa: F401

STATIC_DIR = Path(__file__).resolve().parent / "static"

app = FastAPI(title="crack-pi-server")
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

app.include_router(routes_tasks.router)
app.include_router(routes_stages.router)
app.include_router(routes_chats.router)
app.include_router(routes_sub_agents.router)
