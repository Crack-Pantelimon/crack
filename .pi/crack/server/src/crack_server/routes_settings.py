"""Settings page: global harness knobs (vision model + the three agent models)."""

from __future__ import annotations

from fastapi import APIRouter, Form, HTTPException
from fastapi.responses import HTMLResponse

from crack_server import models as _models
from crack_server import settings as _settings
from crack_server import ui as _ui
from crack_server import vision
from crack_server.render import model_select

router = APIRouter()


def _render_vision_model_row() -> str:
    select = model_select(
        "model",
        vision.vision_model(),
        "/api/settings/vision_model",
        target="closest .part-row",
        swap="outerHTML",
        indent=" " * 10,
        models=_models.image_models_for_render(),
    )
    return f"""
    <div class="part-row">
      <span class="part-label">Vision model</span>
      <small>only image-capable models — used by the <code>analyze_image</code> tool and prompt-image attachments</small>
{select}
    </div>
    """


# One row per agent-model kind — the defaults new chats and spawned sub-agents
# start from.
_AGENT_MODEL_ROWS = {
    "plan_planner": ("Plan · planner", "frontier model that explores + writes the todo list (plan mode)"),
    "plan_implementer": ("Plan · implementer", "cheaper model the run swaps to after the first edit"),
    "nonplan": ("Non-plan model", "single model used when plan mode is off"),
}


def _render_agent_model_row(kind: str) -> str:
    label, blurb = _AGENT_MODEL_ROWS[kind]
    select = model_select(
        "model",
        _settings.get_model(kind),
        f"/api/settings/agent_model/{kind}",
        target="closest .part-row",
        swap="outerHTML",
        indent=" " * 10,
    )
    return f"""
    <div class="part-row">
      <span class="part-label">{label}</span>
      <small>{blurb}</small>
{select}
    </div>
    """


@router.get("/settings", response_class=HTMLResponse)
def settings_page() -> HTMLResponse:
    agent_rows = "".join(_render_agent_model_row(k) for k in _AGENT_MODEL_ROWS)
    body = f"""
    <header>
      <h1>Settings</h1>
      <p><a href="/">← Home</a></p>
    </header>
    <section>
      <h2>Agent models</h2>
      <p class="muted">Defaults for new chats (locked at creation) and spawned sub-agents.</p>
      {agent_rows}
    </section>
    <section>
      <h2>Vision</h2>
      {_render_vision_model_row()}
    </section>
    """
    return HTMLResponse(_ui._render_base("Settings", body))


@router.post("/api/settings/vision_model", response_class=HTMLResponse)
def api_set_vision_model(model: str = Form(...)) -> HTMLResponse:
    """Persist the global vision model (dropdown saves on change)."""
    vision.set_vision_model(model)
    return HTMLResponse(_render_vision_model_row())


@router.post("/api/settings/agent_model/{kind}", response_class=HTMLResponse)
def api_set_agent_model(kind: str, model: str = Form(...)) -> HTMLResponse:
    """Persist one global agent-model default (dropdown saves on change)."""
    if kind not in _AGENT_MODEL_ROWS:
        raise HTTPException(status_code=404, detail="unknown model kind")
    _settings.set_model(kind, model)
    return HTMLResponse(_render_agent_model_row(kind))
