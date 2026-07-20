"""Settings page: global harness knobs (currently just the vision model)."""

from __future__ import annotations

from fastapi import APIRouter, Form
from fastapi.responses import HTMLResponse

from crack_server import ui as _ui
from crack_server import vision
from crack_server.stages.render import model_select

router = APIRouter()


def _render_vision_model_row() -> str:
    select = model_select(
        "model",
        vision.vision_model(),
        "/api/settings/vision_model",
        target="closest .part-row",
        swap="outerHTML",
        indent=" " * 10,
    )
    return f"""
    <div class="part-row">
      <span class="part-label">Vision model</span>
      <small>used by the <code>analyze_image</code> tool and prompt-image attachments</small>
{select}
    </div>
    """


@router.get("/settings", response_class=HTMLResponse)
def settings_page() -> HTMLResponse:
    body = f"""
    <header>
      <h1>Settings</h1>
      <p><a href="/">← Home</a></p>
    </header>
    <section>
      <h2>Models</h2>
      {_render_vision_model_row()}
    </section>
    """
    return HTMLResponse(_ui._render_base("Settings", body))


@router.post("/api/settings/vision_model", response_class=HTMLResponse)
def api_set_vision_model(model: str = Form(...)) -> HTMLResponse:
    """Persist the global vision model (dropdown saves on change)."""
    vision.set_vision_model(model)
    return HTMLResponse(_render_vision_model_row())
