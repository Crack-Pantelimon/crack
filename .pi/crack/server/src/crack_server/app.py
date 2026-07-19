"""FastAPI app construction: mounts static files and the three routers.

The routing layer is split by surface: task/prompt CRUD + title regen in
routes_tasks.py, stage view/status/action/config routes in routes_stages.py,
chat routes in routes_chats.py, and shared HTML helpers in ui.py. Pipeline work
(Explore, Plan, …) lives in the auto-discovered stages package — routes just
delegate to ``stages.REGISTRY`` / ``stages.get(slug)``. Shared pi-subprocess
machinery is in pi_runner.py; the models cache in models.py; path construction
in paths.py; JSON state-file storage in state.py.
"""

from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from crack_server import routes_chats, routes_stages, routes_tasks

# Re-exported for worker.py, which dispatches queue jobs via these names.
from crack_server.routes_tasks import TITLE_JOB_SLUG, _run_title_regen_worker  # noqa: F401

STATIC_DIR = Path(__file__).resolve().parent / "static"

app = FastAPI(title="crack-pi-server")
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

app.include_router(routes_tasks.router)
app.include_router(routes_stages.router)
app.include_router(routes_chats.router)
