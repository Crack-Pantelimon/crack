"""Task routes: task/prompt CRUD, background title regeneration, and the task
pages (home, task view, prompt rows).

Title regeneration is its own queue job (pseudo-slug ``TITLE_JOB_SLUG``), not a
stage; the out-of-process worker (worker.py) dispatches it to
``_run_title_regen_worker``.
"""

from __future__ import annotations

import logging
import shutil
import time

from fastapi import APIRouter, File, Form, HTTPException, Query, Response, UploadFile
from fastapi.responses import HTMLResponse, RedirectResponse

from crack_server import attachments, chats, git_utils, paths, queue, stages, titles
from crack_server.routes_stages import (
    _check_task_id,
    _get_stage_or_404,
    _render_stage_follow,
    _render_stage_tabs_nav,
    _render_task_glyph,
    furthest_engaged_slug,
    task_status_glyph,
    view_url,
)
from crack_server.ui import (
    _esc,
    _format_time,
    _render_base,
    _render_prompt_row,
    _render_prompts_section,
    _render_title_h1,
    _render_title_input,
    _render_title_regen_error,
    _render_title_regen_pending,
)

# Pseudo-stage slug for the non-stage background title-regen job on the queue.
TITLE_JOB_SLUG = "__title__"

router = APIRouter()

# Use uvicorn's configured logger so INFO messages actually reach the console —
# the root logger has no handler attached under uvicorn's default logging config.
logger = logging.getLogger("uvicorn.error")


def _render_task_card(task_id: str, info: dict) -> str:
    """Render a single task card for the homepage."""
    safe_id = _esc(task_id)
    title = _esc(info.get("title", task_id))
    created = _format_time(info.get("created_at", 0))
    modified = _format_time(info.get("modified_at", 0))
    glyph_char, glyph_color = task_status_glyph(task_id)
    return f"""
    <article class="task-card">
      <div class="task-card-header">
        <div>
          <h3>
            <span class="task-glyph" style="color: {glyph_color};">{glyph_char}</span>
            <a href="/tasks/{safe_id}">{title}</a>
          </h3>
          <small class="muted">ID: {safe_id} • Created: {created} • Modified: {modified}</small>
        </div>
        <form hx-delete="/api/tasks/{safe_id}" hx-confirm="Delete task '{title}'?" hx-target="closest article" hx-swap="outerHTML swap:1s">
          <button type="submit" class="contrast">Delete</button>
        </form>
      </div>
    </article>
    """


def _render_task_header(task_id: str, info: dict) -> str:
    """Render the task page header, including the editable title form. This is the only
    title in the UI — prompt rows no longer have their own titles.

    Layout contract: `#title-h1-{id}`, `#title-slot-{id}` and the buttons are
    siblings. Every dynamic title swap (auto-save, regenerate pending/done/error)
    targets the slot with innerHTML and updates the h1 out-of-band, so neither the h1
    nor the buttons can ever be removed by a swap."""
    safe_id = _esc(task_id)
    created = _format_time(info.get("created_at", 0))
    modified = _format_time(info.get("modified_at", 0))
    title_h1 = _render_title_h1(task_id, info.get("title", task_id))
    title_input = _render_title_input(task_id, info.get("title", task_id))
    glyph = _render_task_glyph(task_id)
    return f"""
    <header class="task-header">
      <div class="title-row">
        {glyph}
        {title_h1}
      </div>
      <form class="title-edit-form" hx-put="/api/tasks/{safe_id}/info" hx-target="#title-slot-{safe_id}" hx-swap="innerHTML">
        <span id="title-slot-{safe_id}" class="title-slot">{title_input}</span>
        <button type="button" hx-post="/api/tasks/{safe_id}/regenerate-title" hx-target="#title-slot-{safe_id}" hx-swap="innerHTML" class="secondary">Regenerate Title</button>
        <button type="submit" class="secondary">Save</button>
      </form>
      <p class="muted">ID: {safe_id} • Created: {created} • Modified: {modified}</p>
      <p><a href="/">← All tasks</a></p>
    </header>
    """


# ---------------------------------------------------------------------------
# Background title regeneration (its own job — not a stage, unchanged)
# ---------------------------------------------------------------------------


def _start_title_regen_job(task_id: str) -> None:
    """Kick off a background title-regeneration job if one is not already running."""
    regen = paths.title_regen_state(task_id)
    if regen.read().get("status") == "running":
        return

    content = paths.read_all_prompts_joined(task_id)
    if not content:
        regen.write({"status": "error", "error": "no prompt files to summarize"})
        return

    regen.write({"status": "running", "started_at": time.time()})
    # Runs in the out-of-process worker (see worker.py's TITLE_JOB_SLUG handler).
    queue.enqueue(task_id, TITLE_JOB_SLUG, "title")


def _run_title_regen_worker(task_id: str) -> None:
    """Worker entrypoint for the title-regen job: re-reads prompts, runs the
    title model, and records the result in title_regen.json."""
    try:
        content = paths.read_all_prompts_joined(task_id)
        title = titles.generate_title(content, log_prefix="regenerate-title")
        paths.title_regen_state(task_id).write({"status": "done", "title": title})
    except Exception as e:
        logger.exception("regenerate-title worker failed for %s", task_id)
        paths.title_regen_state(task_id).write({"status": "error", "error": str(e)})


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------


@router.get("/")
def index() -> HTMLResponse:
    root = paths.project_root()
    tasks = paths.list_task_ids(root)

    if tasks:
        cards = "".join(
            _render_task_card(t, paths.read_info(t, root))
            for t in tasks
        )
    else:
        cards = '<p class="muted empty-tasks">No tasks yet — create one below.</p>'

    stage_items = "".join(
        f'<li><a href="/stages/{_esc(s.slug)}">{_esc(s.name)}</a> '
        f'<small class="muted">({_esc(s.slug)})</small></li>'
        for s in stages.REGISTRY
    )

    body = f"""
    <header>
      <h1>Crack Tasks</h1>
      <p class="muted">Project: {_esc(str(root))}</p>
    </header>

    <form class="new-task-form" hx-post="/api/tasks" hx-target="#task-list" hx-swap="afterbegin" hx-on::after-request="this.reset()">
      <h2>New Task</h2>
      <div class="new-task-row">
        <div>
          <label>Title <input type="text" name="title" placeholder="My Task Title" required></label>
        </div>
        <button type="submit">Create Task</button>
      </div>
    </form>

    <section id="task-list">
      {cards}
    </section>

    <section id="harness-stages" class="section-spaced">
      <h2>Harness Stages</h2>
      <ul>
        {stage_items}
        <li><a href="/sub_agents">Sub-agents</a>
          <small class="muted">(personas &amp; models)</small></li>
      </ul>
    </section>

    {chats.render_home_section()}
    """
    return HTMLResponse(_render_base("Crack Tasks", body))


@router.post("/api/tasks")
def api_create_task(title: str = Form(...)) -> HTMLResponse:
    """Create a new task with an auto-generated id (<ms_timestamp>_<slug title>) and
    return the task card HTML fragment (target: #task-list, swap: afterbegin)."""
    task_id = paths.generate_task_id(title)
    try:
        info = paths.create_task(task_id, title)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    return HTMLResponse(_render_task_card(task_id, info))


@router.delete("/api/tasks/{task_id}")
def api_delete_task(task_id: str) -> HTMLResponse:
    """Delete a task directory. Returns an empty fragment so htmx's outerHTML swap
    removes the task card from the DOM."""
    try:
        task_dir = paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    if not task_dir.exists():
        raise HTTPException(status_code=404, detail="not found")

    for item in task_dir.iterdir():
        if item.is_file():
            item.unlink()
        else:
            shutil.rmtree(item)
    task_dir.rmdir()
    return HTMLResponse("")


@router.get("/api/tasks")
def api_tasks() -> dict:
    root = paths.project_root()
    return {"project_root": str(root), "tasks": paths.list_task_ids(root)}


@router.get("/api/tasks/{task_id}/info")
def api_get_task_info(task_id: str) -> dict:
    _check_task_id(task_id)
    return {"task_id": task_id, "info": paths.read_info(task_id)}


@router.put("/api/tasks/{task_id}/info")
def api_update_task_info(task_id: str, title: str = Form(...)) -> HTMLResponse:
    """Update the task title. Returns the slot content (a fresh title input) plus an
    out-of-band h1 swap (targets: #title-slot innerHTML from both the input auto-save
    and the Save form) — the form submits x-www-form-urlencoded, not JSON."""
    _check_task_id(task_id)
    info = paths.read_info(task_id)
    info["title"] = title
    paths.write_info(task_id, info)
    return HTMLResponse(
        _render_title_input(task_id, title) + _render_title_h1(task_id, title, oob=True)
    )


@router.get("/api/tasks/{task_id}/prompts")
def api_list_prompts(task_id: str) -> dict:
    try:
        prompt_list = paths.list_prompt_files(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return {"task_id": task_id, "prompts": prompt_list}


@router.get("/api/tasks/{task_id}/prompts/{filename}")
def api_get_prompt(task_id: str, filename: str) -> dict:
    try:
        content = paths.read_prompt(task_id, filename)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except FileNotFoundError:
        raise HTTPException(status_code=404, detail="not found")
    return {"name": paths.validate_prompt_filename(filename), "content": content}


@router.post("/api/tasks/{task_id}/prompts")
def api_create_prompt(task_id: str, name: str = Form(default=""), content: str = Form(...)) -> HTMLResponse:
    """Create a prompt. If name is blank, auto-assign the next available filename
    (prompt.md, prompt2.md ... prompt9.md). Returns the re-rendered prompts section
    (target: #prompt-list, swap: innerHTML) plus an out-of-band title-regen placeholder."""
    _check_task_id(task_id)

    filename = name.strip()
    if not filename:
        auto_name = paths.next_prompt_filename(task_id)
        if auto_name is None:
            raise HTTPException(status_code=400, detail="No available prompt slot (prompt.md through prompt9.md all exist)")
        filename = auto_name

    try:
        paths.write_prompt(task_id, filename, content)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    git_utils.commit(paths.task_dir(task_id) / filename, f"change prompt file {filename}")
    _start_title_regen_job(task_id)
    return HTMLResponse(
        _render_prompts_section(task_id) + _render_title_regen_pending(task_id, oob=True)
    )


@router.put("/api/tasks/{task_id}/prompts/{filename}")
def api_update_prompt(task_id: str, filename: str, content: str = Form(...)) -> HTMLResponse:
    """Save prompt content. Returns the re-rendered read-only row (target: closest
    article, swap: outerHTML) so the row toggles back from editable to non-editable."""
    try:
        old_content = paths.read_prompt(task_id, filename)
    except FileNotFoundError:
        old_content = ""

    try:
        paths.write_prompt(task_id, filename, content)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    if content != old_content:
        git_utils.commit(paths.task_dir(task_id) / filename, f"change prompt file {filename}")
        _start_title_regen_job(task_id)
        return HTMLResponse(
            _render_prompt_row(task_id, filename, editing=False)
            + _render_title_regen_pending(task_id, oob=True)
        )

    return HTMLResponse(_render_prompt_row(task_id, filename, editing=False))


@router.delete("/api/tasks/{task_id}/prompts/{filename}")
def api_delete_prompt(task_id: str, filename: str) -> HTMLResponse:
    """Returns an empty fragment so htmx's outerHTML swap removes the row."""
    try:
        paths.delete_prompt(task_id, filename)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except FileNotFoundError:
        raise HTTPException(status_code=404, detail="not found")

    git_utils.commit(paths.task_dir(task_id), f"change prompt file {filename}")
    _start_title_regen_job(task_id)
    return HTMLResponse("" + _render_title_regen_pending(task_id, oob=True))


@router.post("/api/tasks/{task_id}/regenerate-title")
def api_regenerate_task_title(task_id: str) -> HTMLResponse:
    """Kick off a background title regeneration and return the polling placeholder."""
    _check_task_id(task_id)
    _start_title_regen_job(task_id)
    return HTMLResponse(_render_title_regen_pending(task_id))


@router.get("/tasks/{task_id}/title-regen-status", response_class=HTMLResponse)
def title_regen_status(task_id: str) -> HTMLResponse:
    """Poll endpoint for the background title regeneration. When it first observes a
    'done' state it also writes the new title to info.json (auto-save)."""
    _check_task_id(task_id)

    state = paths.title_regen_state(task_id).read()
    status = state.get("status")

    if status == "running":
        return HTMLResponse(_render_title_regen_pending(task_id))

    if status == "done":
        title = state.get("title", task_id)
        info = paths.read_info(task_id)
        info["title"] = title
        paths.write_info(task_id, info)
        # Mark the job as saved so future polls return the normal input without re-saving.
        paths.title_regen_state(task_id).write({"status": "saved", "title": title})
        return HTMLResponse(
            _render_title_input(task_id, title) + _render_title_h1(task_id, title, oob=True)
        )

    if status == "error":
        return HTMLResponse(_render_title_regen_error(task_id, state.get("error", "unknown error")))

    # saved, idle, or missing state — just render the current title input.
    info = paths.read_info(task_id)
    return HTMLResponse(_render_title_input(task_id, info.get("title", task_id)))


# ---------------------------------------------------------------------------
# Task page
# ---------------------------------------------------------------------------


def _render_task_view_body(task_id: str, info: dict, active_slug: str) -> str:
    """Full task-page body: header, prompts, the tab bar (links), and the single
    active stage's panel plus its auto-follow poller."""
    safe_id = _esc(task_id)
    header = _render_task_header(task_id, info)
    next_name = paths.next_prompt_filename(task_id) or "prompt.md"
    tabs_nav = _render_stage_tabs_nav(task_id, active_slug)
    stage = _get_stage_or_404(active_slug)
    panel = stage.render_section(task_id)
    follow = _render_stage_follow(task_id, active_slug)

    return f"""
    {header}
    <section id="prompt-list">
      <div hx-get="/tasks/{safe_id}/prompts-list" hx-trigger="load"></div>
    </section>
    {attachments.render_strip("tasks", task_id, paths.task_attachments_state(task_id), "task-attachments")}

    <details class="add">
      <summary style="font-size: 0.95rem; cursor: pointer;">Add Prompt</summary>
      <form hx-post="/api/tasks/{safe_id}/prompts" hx-target="#prompt-list" hx-swap="innerHTML" hx-on::after-request="this.reset()">
        <label>Filename (optional) <input type="text" name="name" placeholder="blank = {_esc(next_name)}" pattern="[a-zA-Z0-9][a-zA-Z0-9_.-]*\\.md"></label>
        <label>Content <textarea name="content" rows="4" placeholder="Markdown content…" required></textarea></label>
        <button type="submit">Add Prompt</button>
      </form>
    </details>

    {tabs_nav}
    <div id="stage-panels" data-active="{_esc(active_slug)}">
      <section class="stage-panel active" id="panel-{_esc(active_slug)}" data-slug="{_esc(active_slug)}">{panel}</section>
    </div>
    {follow}
    """


@router.get("/tasks/{task_id}", response_class=HTMLResponse)
def task_page(task_id: str) -> Response:
    """Bare task URL redirects to the furthest engaged stage's tab page."""
    _check_task_id(task_id)
    active = furthest_engaged_slug(task_id)
    return RedirectResponse(url=view_url(task_id, active), status_code=302)


@router.get("/tasks/{task_id}/view/{slug}", response_class=HTMLResponse)
def stage_view(task_id: str, slug: str) -> HTMLResponse:
    """One tab = one page: shows a single stage's panel. Tabs are <a> links between
    these pages, so the server can force-navigate the user by responding with a
    redirect to another tab's URL."""
    _check_task_id(task_id)
    _get_stage_or_404(slug)
    info = paths.read_info(task_id)
    safe_title = _esc(info.get("title", task_id))
    body = _render_task_view_body(task_id, info, slug)
    return HTMLResponse(_render_base(f"Crack Task: {safe_title}", body, task_id))


@router.get("/tasks/{task_id}/prompts-list", response_class=HTMLResponse)
def task_prompts_list(task_id: str) -> HTMLResponse:
    """Return the prompt list HTML fragment for htmx (initial load on the task page)."""
    _check_task_id(task_id)
    return HTMLResponse(_render_prompts_section(task_id))


@router.get("/tasks/{task_id}/prompt-row/{filename}", response_class=HTMLResponse)
def prompt_row(task_id: str, filename: str, editing: bool = Query(default=False)) -> HTMLResponse:
    """Return one prompt row in view or edit mode (target: closest article, swap:
    outerHTML) — this is how Edit/Cancel toggle a row in place without a separate panel."""
    try:
        return HTMLResponse(_render_prompt_row(task_id, filename, editing=editing))
    except FileNotFoundError as e:
        raise HTTPException(status_code=404, detail="not found") from e


# ---------------------------------------------------------------------------
# Media (persisted turn thumbnails) + prompt-image attachments
# ---------------------------------------------------------------------------


@router.get("/tasks/{task_id}/media/{filename}")
def task_media(task_id: str, filename: str):
    """Serve a persisted image copy from the task's media/ dir."""
    _check_task_id(task_id)
    return attachments.serve_file(paths.task_media_dir(task_id), filename)


@router.get("/tasks/{task_id}/attachments/{filename}")
def task_attachment_file(task_id: str, filename: str):
    """Serve a user-uploaded prompt attachment image."""
    _check_task_id(task_id)
    return attachments.serve_file(paths.task_attachments_dir(task_id), filename)


@router.post("/api/tasks/{task_id}/attachments", response_class=HTMLResponse)
async def api_task_attachment_upload(task_id: str, file: UploadFile = File(...)) -> HTMLResponse:
    """Save a pasted/dropped image, auto-describe it, return its thumbnail chip."""
    _check_task_id(task_id)
    data = await file.read()
    try:
        entry = await attachments.add_attachment(
            paths.task_attachments_state(task_id),
            paths.task_attachments_dir(task_id),
            data,
            file.filename or "image.png",
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return HTMLResponse(attachments.render_chip("tasks", task_id, entry))


@router.delete("/api/tasks/{task_id}/attachments/{attachment_id}", response_class=HTMLResponse)
def api_task_attachment_delete(task_id: str, attachment_id: str) -> HTMLResponse:
    """Remove one attachment (file + manifest entry); empty fragment drops the chip."""
    _check_task_id(task_id)
    try:
        deleted = attachments.delete_attachment(
            paths.task_attachments_state(task_id),
            paths.task_attachments_dir(task_id),
            attachment_id,
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    if not deleted:
        raise HTTPException(status_code=404, detail="not found")
    return HTMLResponse("")
