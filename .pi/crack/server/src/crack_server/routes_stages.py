"""Stage routes: the generic /stages/{slug}/… view/status/action routes, the
per-stage config screen (/stages/<slug>), the models cache, the explore
file-ref lazy loader, and the task long-poll (/wait) + auto-follow endpoints.

Also home of the shared stage-navigation helpers (tab bar, status glyph,
follow frontier) that the task pages in routes_tasks.py reuse.
"""

from __future__ import annotations

import asyncio
import time
from pathlib import Path

from fastapi import APIRouter, Form, HTTPException, Query, Request
from fastapi.responses import HTMLResponse, JSONResponse

from crack_server import models as models_mod
from crack_server import paths, pi_runner, stages
from crack_server.state import task_state_mtimes
from crack_server.ui import _esc, _render_base

router = APIRouter()


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _get_stage_or_404(slug: str) -> stages.Stage:
    stage = stages.get(slug)
    if stage is None:
        raise HTTPException(status_code=404, detail="unknown stage")
    return stage


def _check_task_id(task_id: str) -> None:
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e


def task_status_glyph(task_id: str) -> tuple[str, str]:
    """Furthest-stage status glyph: (char, css color)."""
    review = stages.get("plan_review")
    if review is not None and review.status(task_id) == "done":
        return "✓", "#2a7fd4"

    furthest_status = "idle"
    for stage in stages.REGISTRY:
        st = stage.status(task_id)
        if st not in ("idle", "disabled"):
            furthest_status = st

    if furthest_status == "idle":
        return "○", "#999"
    if furthest_status == "error":
        return "●", "#c44"
    if furthest_status in ("running", "awaiting"):
        return "●", "#28be5a"
    if furthest_status == "done":
        return "●", "#2a7fd4"
    return "○", "#999"


def _render_task_glyph(task_id: str, oob: bool = False) -> str:
    char, color = task_status_glyph(task_id)
    safe_id = _esc(task_id)
    oob_attr = ' hx-swap-oob="innerHTML"' if oob else ""
    return (
        f'<span id="task-glyph-{safe_id}" class="task-glyph" '
        f'style="color: {color}; font-size: 1.2rem; margin-right: 0.35rem;"{oob_attr}>'
        f"{char}</span>"
    )


def furthest_engaged_slug(task_id: str) -> str:
    """The last stage that has been *engaged* (status not idle/disabled — i.e.
    running, awaiting, done, or error), else the first stage.

    This is the auto-follow frontier: while the user views this stage, the page
    polls and jumps forward the moment a later stage becomes engaged."""
    if not stages.REGISTRY:
        return ""
    frontier = stages.REGISTRY[0].slug
    for stage in stages.REGISTRY:
        if stage.status(task_id) not in ("idle", "disabled"):
            frontier = stage.slug
    return frontier


def _stage_viewable(task_id: str, stage: "stages.Stage") -> bool:
    """A stage's tab is navigable once it is enabled or has been engaged."""
    return stage.is_enabled(task_id) or stage.status(task_id) not in ("idle", "disabled")


def view_url(task_id: str, slug: str) -> str:
    return f"/tasks/{task_id}/view/{slug}"


def _render_stage_tabs_nav(task_id: str, active_slug: str) -> str:
    """Tab bar as real links (<a> for viewable stages, disabled <span> for locked
    ones). Navigating between tabs is a full page load — no client-side tab state —
    so the server can force-jump the user by redirecting to another tab's URL."""
    from crack_server.stages.base import STATUS_COLORS

    tabs: list[str] = []
    for stage in stages.REGISTRY:
        st = stage.status(task_id)
        viewable = _stage_viewable(task_id, stage)
        color_cls = STATUS_COLORS.get(st, "tab--idle")
        if not viewable:
            color_cls = "tab--disabled"
        selected = " selected" if stage.slug == active_slug else ""
        safe_slug = _esc(stage.slug)
        safe_name = _esc(stage.name)
        label = f'{safe_name} <span class="tab-dot"></span>'
        if viewable:
            tabs.append(
                f'<a class="tab {color_cls}{selected}" href="{_esc(view_url(task_id, stage.slug))}"'
                f' data-slug="{safe_slug}">{label}</a>'
            )
        else:
            tabs.append(
                f'<span class="tab {color_cls}{selected} disabled" data-slug="{safe_slug}">{label}</span>'
            )
    return f'<nav id="stage-tabs" class="stage-tabs">{"".join(tabs)}</nav>'


def _render_stage_follow(task_id: str, slug: str) -> str:
    """Legacy no-op: auto-follow is folded into the ``/wait`` long-poll (plan 4.2)."""
    return ""


# ---------------------------------------------------------------------------
# Generic stage routes (extensible — new stages need no route changes)
# ---------------------------------------------------------------------------


@router.post("/api/tasks/{task_id}/stages/{slug}/start")
def api_stage_start(task_id: str, slug: str) -> HTMLResponse:
    _check_task_id(task_id)
    stage = _get_stage_or_404(slug)
    stage.start(task_id)
    return HTMLResponse(stage.render_status(task_id))


@router.get("/tasks/{task_id}/stages/{slug}/status", response_class=HTMLResponse)
def stage_status(
    task_id: str,
    slug: str,
    after: int | None = Query(default=None),
) -> HTMLResponse:
    _check_task_id(task_id)
    stage = _get_stage_or_404(slug)
    # RC6 watchdog at poll time: a running phase with no queued job behind it
    # renders as an error immediately instead of an infinite spinner.
    stage.check_orphaned(task_id)
    return HTMLResponse(stage.render_status(task_id, after=after))


@router.get("/tasks/{task_id}/fileref", response_class=HTMLResponse)
def task_fileref(
    task_id: str,
    path: str = Query(...),
    start: int | None = Query(default=None),
    end: int | None = Query(default=None),
) -> HTMLResponse:
    """Lazy file-ref body for explore path_refs (loaded on first <details> expand)."""
    _check_task_id(task_id)
    rel = path.lstrip("/")
    if not rel or ".." in Path(rel).parts:
        return HTMLResponse('<pre class="fileref-missing">(file not found)</pre>')
    lines, _, _, marker = pi_runner.read_file_lines(
        paths.project_root(), rel, start, end
    )
    if not lines and not (paths.project_root() / rel).is_file():
        return HTMLResponse('<pre class="fileref-missing">(file not found)</pre>')
    body = f"<pre>{_esc(lines)}</pre>"
    if marker:
        body += f'<small class="trunc-marker">{_esc(marker)}</small>'
    return HTMLResponse(body)


@router.get("/tasks/{task_id}/wait")
async def task_wait(
    task_id: str,
    since: float = Query(default=0.0),
    slug: str = Query(default=""),
) -> JSONResponse:
    """Long-poll until task state mtimes advance or the follow frontier moves.

    Parks for up to 25s (async — does not occupy the sync threadpool). Idle
    pages hold one open request instead of stacked 1.5s pollers."""
    _check_task_id(task_id)
    # Only auto-follow when the client is currently on the frontier (matches the
    # old poller's "only render when furthest == slug" rule).
    follow = bool(slug) and furthest_engaged_slug(task_id) == slug
    deadline = time.monotonic() + 25.0
    while True:
        mtime = task_state_mtimes(task_id)
        frontier = furthest_engaged_slug(task_id)
        if follow and frontier != slug:
            return JSONResponse(
                {
                    "since": mtime,
                    "redirect": view_url(task_id, frontier),
                    "changed": True,
                }
            )
        if mtime > since:
            return JSONResponse(
                {"since": mtime, "redirect": None, "changed": True}
            )
        if time.monotonic() >= deadline:
            return JSONResponse(
                {"since": since, "redirect": None, "changed": False}
            )
        await asyncio.sleep(0.3)


@router.post("/api/tasks/{task_id}/stages/{slug}/actions/{action}")
async def api_stage_action(task_id: str, slug: str, action: str, request: Request) -> HTMLResponse:
    _check_task_id(task_id)
    stage = _get_stage_or_404(slug)
    form = await request.form()
    stage.handle_action(action, task_id, form)
    # The panel swaps in place; the tab glyph is refreshed out-of-band. Any tab
    # *jump* (e.g. approve → implementation) is handled by the auto-follow poller,
    # which force-navigates the browser once a later stage becomes engaged.
    fragment = stage.render_status(task_id) + _render_task_glyph(task_id, oob=True)
    return HTMLResponse(fragment)


@router.get("/tasks/{task_id}/follow/{slug}", response_class=HTMLResponse)
def stage_follow_poll(task_id: str, slug: str) -> HTMLResponse:
    """Auto-follow poll: redirect the browser to the frontier stage's tab when it
    moves past ``slug``; otherwise re-emit the poller so it keeps watching."""
    _check_task_id(task_id)
    active = furthest_engaged_slug(task_id)
    if active != slug:
        return HTMLResponse("", headers={"HX-Redirect": view_url(task_id, active)})
    return HTMLResponse(_render_stage_follow(task_id, slug))


# ---------------------------------------------------------------------------
# Stage config screen (/stages/<slug>) and models cache
# ---------------------------------------------------------------------------


@router.get("/stages/{slug}", response_class=HTMLResponse)
def stage_page(slug: str) -> HTMLResponse:
    """Per-stage config page: model dropdowns per part, editable prompt
    templates, and the stage's .py source (read-only)."""
    stage = _get_stage_or_404(slug)
    return HTMLResponse(_render_base(f"Stage: {stage.name}", stage.render_config_body()))


@router.post("/api/stages/{slug}/parts/{part}/model")
def api_set_part_model(slug: str, part: str, model: str = Form(...)) -> HTMLResponse:
    """Save a part's model override (harness/<slug>.json) and re-render the row
    (target: closest .part-row, swap: outerHTML)."""
    stage = _get_stage_or_404(slug)
    try:
        stage.set_model(part, model)
    except KeyError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    return HTMLResponse(stage.render_part_row(stage.part(part)))


@router.get("/stages/{slug}/template-row/{filename}", response_class=HTMLResponse)
def stage_template_row(slug: str, filename: str, editing: bool = Query(default=False)) -> HTMLResponse:
    """Return one stage-template row in view or edit mode (target: closest
    article, swap: outerHTML) — same in-place toggle as task prompt rows."""
    stage = _get_stage_or_404(slug)
    try:
        return HTMLResponse(stage.render_template_row(filename, editing=editing))
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except FileNotFoundError as e:
        raise HTTPException(status_code=404, detail="not found") from e


@router.put("/api/stages/{slug}/templates/{filename}")
def api_update_stage_template(slug: str, filename: str, content: str = Form(...)) -> HTMLResponse:
    """Save stage template content. Returns the re-rendered read-only row."""
    stage = _get_stage_or_404(slug)
    try:
        paths.write_stage_template(stage.slug, filename, content)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return HTMLResponse(stage.render_template_row(filename, editing=False))


@router.get("/api/models")
def api_models(force: bool = Query(default=False)) -> dict:
    """Debug view of the models cache (harness/models_list.json).

    Never shells out (B21): a stale cache — or ``force=true`` — enqueues a
    background refresh job; the response always comes from the current cache."""
    return {"models": models_mod.models_for_render(force=force)}
