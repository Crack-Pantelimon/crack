"""FastAPI app: HTML editor + JSON API with htmx + pico.css.

Thin routing layer: task/prompt CRUD and title regeneration live here; pipeline
work (Explore, Plan, …) lives in the auto-discovered stages package — routes
just delegate to ``stages.REGISTRY`` / ``stages.get(slug)``. Shared pi-subprocess
machinery is in pi_runner.py; the models cache in models.py; all filesystem
access in paths.py.
"""

from __future__ import annotations

import html
import logging
import shutil
import time
from pathlib import Path

from fastapi import FastAPI, Form, HTTPException, Query, Request, Response
from fastapi.responses import HTMLResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from markdown_it import MarkdownIt

from crack_server import git_utils
from crack_server import models as models_mod
from crack_server import chats, paths, pi_runner, queue, stages
from crack_server.stages.base import STATUS_COLORS

# Pseudo-stage slug for the non-stage background title-regen job on the queue.
TITLE_JOB_SLUG = "__title__"

STATIC_DIR = Path(__file__).resolve().parent / "static"

app = FastAPI(title="crack-pi-server")
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

# Use uvicorn's configured logger so INFO messages actually reach the console —
# the root logger has no handler attached under uvicorn's default logging config.
logger = logging.getLogger("uvicorn.error")


def _esc(text: str) -> str:
    return html.escape(text, quote=True)


def _format_time(ts: float) -> str:
    """Format timestamp as YYYY-MM-DD HH:MM."""
    return time.strftime("%Y-%m-%d %H:%M", time.localtime(ts))


def _load_template(name: str) -> str:
    """Read a prompt template from disk fresh on every call (no caching)."""
    path = paths.templates_dir() / f"{name}.md"
    if not path.is_file():
        raise RuntimeError(f"missing prompt template: {path}")
    return path.read_text(encoding="utf-8")


# Raw HTML is disabled: anything the model emits renders as escaped text, so the
# summary cannot inject markup into the task page.
_markdown = MarkdownIt("commonmark", {"html": False})


def _render_markdown(md_text: str) -> str:
    """Render markdown to HTML (CommonMark, raw HTML disabled)."""
    return _markdown.render(md_text)


def _format_ago(ts: float) -> str:
    """Human 'X ago' for an epoch timestamp."""
    delta = max(0, int(time.time() - ts))
    if delta < 60:
        return f"{delta}s ago"
    if delta < 3600:
        return f"{delta // 60}m ago"
    if delta < 86400:
        return f"{delta // 3600}h ago"
    return f"{delta // 86400}d ago"


def _render_base(title: str, body: str, task_id: str | None = None) -> str:
    """Render base HTML template with htmx + pico.css. All page/interaction styling and
    JS lives in static/app.css and static/app.js (linked here, not inlined)."""
    task_attr = f' data-task-id="{_esc(task_id)}"' if task_id else ""
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>{_esc(title)}</title>
  <!-- Pico.css -->
  <link
    rel="stylesheet"
    href="https://cdn.jsdelivr.net/npm/@picocss/pico@2.1.1/css/pico.classless.min.css"
  >
  <link rel="stylesheet" href="/static/app.css">
  <!-- htmx -->
  <script
    src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.10/dist/htmx.min.js"
    integrity="sha384-H5SrcfygHmAuTDZphMHqBJLc3FhssKjG7w/CeCpFReSfwBWDTKpkzPP8c+cLsK+V"
    crossorigin="anonymous"
  ></script>
</head>
<body{task_attr}>
  <main>
    {body}
  </main>
  <script src="/static/app.js"></script>
</body>
</html>"""


def _render_task_card(task_id: str, info: dict) -> str:
    """Render a single task card for the homepage."""
    safe_id = _esc(task_id)
    title = _esc(info.get("title", task_id))
    created = _format_time(info.get("created_at", 0))
    modified = _format_time(info.get("modified_at", 0))
    glyph_char, glyph_color = task_status_glyph(task_id)
    return f"""
    <article class="task-card" style="border: 1px solid #ddd; border-radius: 8px; padding: 1rem; margin-bottom: 1rem;">
      <div style="display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem;">
        <div>
          <h3 style="margin: 0 0 0.5rem 0;">
            <span class="task-glyph" style="color: {glyph_color}; margin-right: 0.35rem;">{glyph_char}</span>
            <a href="/tasks/{safe_id}" style="text-decoration: none;">{title}</a>
          </h3>
          <small style="color: #666;">ID: {safe_id} • Created: {created} • Modified: {modified}</small>
        </div>
        <form hx-delete="/api/tasks/{safe_id}" hx-confirm="Delete task '{title}'?" hx-target="closest article" hx-swap="outerHTML swap:1s">
          <button type="submit" class="secondary" style="margin: 0;">Delete</button>
        </form>
      </div>
    </article>
    """


def _render_title_h1(task_id: str, title: str, oob: bool = False) -> str:
    """The big page title. Rendered out-of-band (outerHTML on the same id) whenever the
    title changes via slot swaps, so the h1 always tracks the saved value."""
    safe_id = _esc(task_id)
    oob_attr = ' hx-swap-oob="true"' if oob else ""
    return f'<h1 id="title-h1-{safe_id}" style="margin: 0; flex: 1;"{oob_attr}>{_esc(title)}</h1>'


def _render_title_input(task_id: str, title: str) -> str:
    """Render the title <input> alone — the inner content of `#title-slot-{id}`.

    The auto-save (change/blur) targets the slot with innerHTML, never `closest
    header`, so a blur can never clobber the h1 or the Regenerate/Save buttons."""
    safe_id = _esc(task_id)
    safe_title = _esc(title)
    return (
        f'<input type="text" name="title" id="title-input-{safe_id}" class="title-input" '
        f'value="{safe_title}" placeholder="Task title" '
        f'hx-put="/api/tasks/{safe_id}/info" hx-trigger="change delay:500ms, blur" '
        f'hx-target="#title-slot-{safe_id}" hx-swap="innerHTML">'
    )


def _render_title_regen_pending(task_id: str, oob: bool = False) -> str:
    """Slot content shown while a title-regeneration job is running.

    The polling span targets `#title-slot-{id}` (innerHTML), so the h1 and buttons —
    siblings of the slot, outside it — survive every swap. With ``oob=True`` the
    fragment instead carries the slot id + hx-swap-oob so prompt CRUD routes can
    refresh the header out-of-band."""
    safe_id = _esc(task_id)
    current_title = _esc(paths.read_info(task_id).get("title", task_id))
    inner = (
        f'<span class="title-input-pending" aria-busy="true" '
        f'hx-trigger="every 1.5s" hx-get="/tasks/{safe_id}/title-regen-status" '
        f'hx-target="#title-slot-{safe_id}" hx-swap="innerHTML">'
        f'<input type="text" name="title" disabled value="{current_title}">'
        f'<input type="hidden" name="title" value="{current_title}">'
        f'<small>generating title…</small>'
        f'</span>'
    )
    if oob:
        return f'<span id="title-slot-{safe_id}" hx-swap-oob="innerHTML">{inner}</span>'
    return inner


def _render_title_regen_error(task_id: str, error: str) -> str:
    """Terminal state for a failed background title regeneration: restore the normal
    input into the slot plus an inline error note (title attribute has the detail)."""
    safe_error = _esc(error)
    info = paths.read_info(task_id)
    return (
        _render_title_input(task_id, info.get("title", task_id))
        + f'<small class="error" title="{safe_error}">title generation failed</small>'
    )


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
    """A tiny self-polling element that force-navigates the user forward.

    Included only when viewing the *frontier* stage (and it isn't the last one).
    It polls ``/follow/{slug}``; when a later stage becomes engaged, that endpoint
    responds with an ``HX-Redirect`` header and htmx navigates to the new tab —
    this is how "jump the user to a different tab" is enforced from the server."""
    if not stages.REGISTRY or slug == stages.REGISTRY[-1].slug:
        return ""
    if furthest_engaged_slug(task_id) != slug:
        return ""  # browsing a past stage — do not auto-follow
    safe = _esc(slug)
    return (
        f'<div class="stage-follow" hx-get="/tasks/{_esc(task_id)}/follow/{safe}"'
        ' hx-trigger="every 1.5s" hx-swap="outerHTML"></div>'
    )


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
    <header style="margin-bottom: 1.5rem;">
      <div class="title-row" style="margin-bottom: 1rem;">
        {glyph}
        {title_h1}
        <form hx-put="/api/tasks/{safe_id}/info" hx-target="#title-slot-{safe_id}" hx-swap="innerHTML" style="flex: 1; display: flex; gap: 0.5rem; align-items: center;">
          <span id="title-slot-{safe_id}" class="title-slot">{title_input}</span>
          <button type="button" hx-post="/api/tasks/{safe_id}/regenerate-title" hx-target="#title-slot-{safe_id}" hx-swap="innerHTML" class="secondary">Regenerate Title</button>
          <button type="submit" class="secondary">Save</button>
        </form>
      </div>
      <p style="color: #666; margin: 0;">ID: {safe_id} • Created: {created} • Modified: {modified}</p>
      <p><a href="/">← All tasks</a></p>
    </header>
    """


def _render_prompt_row(task_id: str, filename: str, editing: bool = False) -> str:
    """Render one prompt row. View mode always shows the file content (read-only);
    Edit mode swaps the same row (closest article) into an editable form in place."""
    content = paths.read_prompt(task_id, filename)  # raises FileNotFoundError if missing

    stat = (paths.task_dir(task_id) / filename).stat()
    size = stat.st_size
    mtime = _format_time(stat.st_mtime)

    safe_id = _esc(task_id)
    safe_name = _esc(filename)
    safe_content = _esc(content)

    if editing:
        return f"""
        <article class="prompt-row">
          <form hx-put="/api/tasks/{safe_id}/prompts/{safe_name}" hx-target="closest article" hx-swap="outerHTML">
            <div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem;">
              <label style="flex: 1;">Filename <input type="text" value="{safe_name}" readonly></label>
              <small style="color: #666;">{size} bytes • {mtime}</small>
            </div>
            <label>Content
              <textarea name="content" rows="12" required>{safe_content}</textarea>
            </label>
            <div class="actions">
              <button type="submit">Save</button>
              <button type="button" hx-get="/tasks/{safe_id}/prompt-row/{safe_name}" hx-target="closest article" hx-swap="outerHTML" class="secondary">Cancel</button>
            </div>
          </form>
        </article>
        """

    return f"""
    <article class="prompt-row">
      <div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem;">
        <span class="name">{safe_name}</span>
        <small style="color: #666;">{size} bytes • {mtime}</small>
      </div>
      <textarea readonly rows="4">{safe_content}</textarea>
      <div class="actions">
        <button hx-get="/tasks/{safe_id}/prompt-row/{safe_name}?editing=true" hx-target="closest article" hx-swap="outerHTML">Edit</button>
        <form hx-delete="/api/tasks/{safe_id}/prompts/{safe_name}" hx-target="closest article" hx-swap="outerHTML swap:1s" hx-confirm="Delete '{safe_name}'?" style="margin: 0;">
          <button type="submit" class="secondary" style="color: #c44; border-color: #c44;">Remove</button>
        </form>
      </div>
    </article>
    """


def _render_prompts_section(task_id: str) -> str:
    """Render the full list of prompt rows (always shown, content always viewable)."""
    prompts = paths.list_prompt_files(task_id)
    if not prompts:
        return '<p style="color: #666;">No .md files in this task folder yet.</p>'

    rows = []
    for p in prompts:
        try:
            rows.append(_render_prompt_row(task_id, str(p["name"])))
        except FileNotFoundError:
            continue  # deleted between listing and rendering

    return f"""
    <h2>Prompt files</h2>
    <div id="prompt-list-inner">
      {"".join(rows)}
    </div>
    """


# ---------------------------------------------------------------------------
# Background title regeneration (its own job — not a stage, unchanged)
# ---------------------------------------------------------------------------


def _start_title_regen_job(task_id: str) -> None:
    """Kick off a background title-regeneration job if one is not already running."""
    state = paths.read_title_regen_state(task_id)
    if state.get("status") == "running":
        return

    content = paths.read_all_prompts_joined(task_id)
    if not content:
        paths.write_title_regen_state(
            task_id, {"status": "error", "error": "no prompt files to summarize"}
        )
        return

    paths.write_title_regen_state(task_id, {"status": "running", "started_at": time.time()})
    # Runs in the out-of-process worker (see worker.py's TITLE_JOB_SLUG handler).
    queue.enqueue(task_id, TITLE_JOB_SLUG, "title")


def _run_title_regen_worker(task_id: str) -> None:
    """Worker entrypoint for the title-regen job: re-reads prompts, runs the
    title model, and records the result in title_regen.json."""
    try:
        content = paths.read_all_prompts_joined(task_id)
        prompt = _load_template("title").replace("{content}", content)
        title, _ = pi_runner.run_pi_text(
            prompt,
            log_prefix="regenerate-title",
            model=pi_runner.TITLE_MODEL,
            max_input_chars=pi_runner.TITLE_MAX_INPUT_CHARS,
        )
        paths.write_title_regen_state(task_id, {"status": "done", "title": title})
    except Exception as e:
        logger.exception("regenerate-title worker failed for %s", task_id)
        paths.write_title_regen_state(task_id, {"status": "error", "error": str(e)})


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


# ---------------------------------------------------------------------------
# Routes
# ---------------------------------------------------------------------------


@app.get("/")
def index() -> HTMLResponse:
    root = paths.project_root()
    tasks = paths.list_task_ids(root)

    if tasks:
        cards = "".join(
            _render_task_card(t, paths.read_info(t, root))
            for t in tasks
        )
    else:
        cards = '<p style="color: #666; text-align: center; padding: 2rem;">No tasks yet — create one below.</p>'

    stage_items = "".join(
        f'<li><a href="/stages/{_esc(s.slug)}">{_esc(s.name)}</a> '
        f'<small style="color: #666;">({_esc(s.slug)})</small></li>'
        for s in stages.REGISTRY
    )

    body = f"""
    <header style="margin-bottom: 2rem;">
      <h1>Crack Tasks</h1>
      <p style="color: #666;">Project: {_esc(str(root))}</p>
    </header>

    <form hx-post="/api/tasks" hx-target="#task-list" hx-swap="afterbegin" hx-on::after-request="this.reset()" style="margin-bottom: 2rem;">
      <h2 style="margin-top: 0;">New Task</h2>
      <div style="display: flex; gap: 0.5rem; flex-wrap: wrap; align-items: flex-end;">
        <div style="flex: 1; min-width: 200px;">
          <label>Title <input type="text" name="title" placeholder="My Task Title" required></label>
        </div>
        <button type="submit" class="primary">Create Task</button>
      </div>
    </form>

    <section id="task-list">
      {cards}
    </section>

    <section id="harness-stages" style="margin-top: 2rem;">
      <h2># Harness Stages</h2>
      <ul>
        {stage_items}
      </ul>
    </section>

    {chats.render_home_section()}
    """
    return HTMLResponse(_render_base("Crack Tasks", body))


@app.post("/api/tasks")
def api_create_task(title: str = Form(...)) -> HTMLResponse:
    """Create a new task with an auto-generated id (<ms_timestamp>_<slug title>) and
    return the task card HTML fragment (target: #task-list, swap: afterbegin)."""
    task_id = paths.generate_task_id(title)
    try:
        info = paths.create_task(task_id, title)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    return HTMLResponse(_render_task_card(task_id, info))


@app.delete("/api/tasks/{task_id}")
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


@app.get("/api/tasks")
def api_tasks() -> dict:
    root = paths.project_root()
    return {"project_root": str(root), "tasks": paths.list_task_ids(root)}


@app.get("/api/tasks/{task_id}/info")
def api_get_task_info(task_id: str) -> dict:
    _check_task_id(task_id)
    return {"task_id": task_id, "info": paths.read_info(task_id)}


@app.put("/api/tasks/{task_id}/info")
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


@app.get("/api/tasks/{task_id}/prompts")
def api_list_prompts(task_id: str) -> dict:
    try:
        prompt_list = paths.list_prompt_files(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return {"task_id": task_id, "prompts": prompt_list}


@app.get("/api/tasks/{task_id}/prompts/{filename}")
def api_get_prompt(task_id: str, filename: str) -> dict:
    try:
        content = paths.read_prompt(task_id, filename)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except FileNotFoundError:
        raise HTTPException(status_code=404, detail="not found")
    return {"name": paths.validate_prompt_filename(filename), "content": content}


@app.post("/api/tasks/{task_id}/prompts")
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


@app.put("/api/tasks/{task_id}/prompts/{filename}")
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


@app.delete("/api/tasks/{task_id}/prompts/{filename}")
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


@app.post("/api/tasks/{task_id}/regenerate-title")
def api_regenerate_task_title(task_id: str) -> HTMLResponse:
    """Kick off a background title regeneration and return the polling placeholder."""
    _check_task_id(task_id)
    _start_title_regen_job(task_id)
    return HTMLResponse(_render_title_regen_pending(task_id))


@app.get("/tasks/{task_id}/title-regen-status", response_class=HTMLResponse)
def title_regen_status(task_id: str) -> HTMLResponse:
    """Poll endpoint for the background title regeneration. When it first observes a
    'done' state it also writes the new title to info.json (auto-save)."""
    _check_task_id(task_id)

    state = paths.read_title_regen_state(task_id)
    status = state.get("status")

    if status == "running":
        return HTMLResponse(_render_title_regen_pending(task_id))

    if status == "done":
        title = state.get("title", task_id)
        info = paths.read_info(task_id)
        info["title"] = title
        paths.write_info(task_id, info)
        # Mark the job as saved so future polls return the normal input without re-saving.
        paths.write_title_regen_state(task_id, {"status": "saved", "title": title})
        return HTMLResponse(
            _render_title_input(task_id, title) + _render_title_h1(task_id, title, oob=True)
        )

    if status == "error":
        return HTMLResponse(_render_title_regen_error(task_id, state.get("error", "unknown error")))

    # saved, idle, or missing state — just render the current title input.
    info = paths.read_info(task_id)
    return HTMLResponse(_render_title_input(task_id, info.get("title", task_id)))


# ---------------------------------------------------------------------------
# Stage routes (Explore, Plan, and any future stage — nothing hard-coded)
# ---------------------------------------------------------------------------


@app.post("/api/tasks/{task_id}/explore")
def api_explore(task_id: str) -> HTMLResponse:
    """Start a background Explore run, or return the current status if one is running."""
    _check_task_id(task_id)
    stage = _get_stage_or_404("explore")
    stage.start(task_id)  # idempotent: no-op while a run is active
    return HTMLResponse(stage.render_status(task_id))


@app.get("/tasks/{task_id}/explore-status", response_class=HTMLResponse)
def explore_status(task_id: str) -> HTMLResponse:
    """Poll endpoint for the background Explore run."""
    _check_task_id(task_id)
    return HTMLResponse(_get_stage_or_404("explore").render_status(task_id))


@app.post("/api/tasks/{task_id}/plan")
def api_plan(task_id: str) -> HTMLResponse:
    """Start/re-run the Plan draft, or return the current status if running."""
    _check_task_id(task_id)
    stage = _get_stage_or_404("plan")
    stage.start(task_id)
    return HTMLResponse(stage.render_status(task_id))


@app.get("/tasks/{task_id}/plan-status", response_class=HTMLResponse)
def plan_status(task_id: str) -> HTMLResponse:
    """Poll endpoint for the background Plan run."""
    _check_task_id(task_id)
    return HTMLResponse(_get_stage_or_404("plan").render_status(task_id))


@app.post("/api/tasks/{task_id}/plan/answers")
async def api_plan_answers(task_id: str, request: Request) -> HTMLResponse:
    """Record one round of Q&A answers and resume the draft agent (alias)."""
    _check_task_id(task_id)
    stage = _get_stage_or_404("plan")
    form = await request.form()
    stage.handle_action("answers", task_id, form)
    return HTMLResponse(stage.render_status(task_id))


# ---------------------------------------------------------------------------
# Generic stage routes (extensible — new stages need no app.py change)
# ---------------------------------------------------------------------------


@app.post("/api/tasks/{task_id}/stages/{slug}/start")
def api_stage_start(task_id: str, slug: str) -> HTMLResponse:
    _check_task_id(task_id)
    stage = _get_stage_or_404(slug)
    stage.start(task_id)
    return HTMLResponse(stage.render_status(task_id))


@app.get("/tasks/{task_id}/stages/{slug}/status", response_class=HTMLResponse)
def stage_status(task_id: str, slug: str) -> HTMLResponse:
    _check_task_id(task_id)
    return HTMLResponse(_get_stage_or_404(slug).render_status(task_id))


@app.post("/api/tasks/{task_id}/stages/{slug}/actions/{action}")
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


# ---------------------------------------------------------------------------
# Stage config screen (/stages/<slug>) and models cache
# ---------------------------------------------------------------------------


@app.get("/stages/{slug}", response_class=HTMLResponse)
def stage_page(slug: str) -> HTMLResponse:
    """Per-stage config page: model dropdowns per part, editable prompt
    templates, and the stage's .py source (read-only)."""
    stage = _get_stage_or_404(slug)
    return HTMLResponse(_render_base(f"Stage: {stage.name}", stage.render_config_body()))


@app.post("/api/stages/{slug}/parts/{part}/model")
def api_set_part_model(slug: str, part: str, model: str = Form(...)) -> HTMLResponse:
    """Save a part's model override (harness/<slug>.json) and re-render the row
    (target: closest .part-row, swap: outerHTML)."""
    stage = _get_stage_or_404(slug)
    try:
        stage.set_model(part, model)
    except KeyError as e:
        raise HTTPException(status_code=404, detail=str(e)) from e
    return HTMLResponse(stage.render_part_row(stage.part(part)))


@app.get("/stages/{slug}/template-row/{filename}", response_class=HTMLResponse)
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


@app.put("/api/stages/{slug}/templates/{filename}")
def api_update_stage_template(slug: str, filename: str, content: str = Form(...)) -> HTMLResponse:
    """Save stage template content. Returns the re-rendered read-only row."""
    stage = _get_stage_or_404(slug)
    try:
        paths.write_stage_template(stage.slug, filename, content)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return HTMLResponse(stage.render_template_row(filename, editing=False))


@app.get("/api/models")
def api_models(force: bool = Query(default=False)) -> dict:
    """Debug view of the models cache (harness/models_list.json)."""
    return {"models": models_mod.get_models(force=force)}


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


@app.get("/tasks/{task_id}", response_class=HTMLResponse)
def task_page(task_id: str) -> Response:
    """Bare task URL redirects to the furthest engaged stage's tab page."""
    _check_task_id(task_id)
    active = furthest_engaged_slug(task_id)
    return RedirectResponse(url=view_url(task_id, active), status_code=302)


@app.get("/tasks/{task_id}/view/{slug}", response_class=HTMLResponse)
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


@app.get("/tasks/{task_id}/follow/{slug}", response_class=HTMLResponse)
def stage_follow_poll(task_id: str, slug: str) -> HTMLResponse:
    """Auto-follow poll: redirect the browser to the frontier stage's tab when it
    moves past ``slug``; otherwise re-emit the poller so it keeps watching."""
    _check_task_id(task_id)
    active = furthest_engaged_slug(task_id)
    if active != slug:
        return HTMLResponse("", headers={"HX-Redirect": view_url(task_id, active)})
    return HTMLResponse(_render_stage_follow(task_id, slug))


@app.get("/tasks/{task_id}/prompts-list", response_class=HTMLResponse)
def task_prompts_list(task_id: str) -> HTMLResponse:
    """Return the prompt list HTML fragment for htmx (initial load on the task page)."""
    _check_task_id(task_id)
    return HTMLResponse(_render_prompts_section(task_id))


@app.get("/tasks/{task_id}/prompt-row/{filename}", response_class=HTMLResponse)
def prompt_row(task_id: str, filename: str, editing: bool = Query(default=False)) -> HTMLResponse:
    """Return one prompt row in view or edit mode (target: closest article, swap:
    outerHTML) — this is how Edit/Cancel toggle a row in place without a separate panel."""
    try:
        return HTMLResponse(_render_prompt_row(task_id, filename, editing=editing))
    except FileNotFoundError as e:
        raise HTTPException(status_code=404, detail="not found") from e


# ---------------------------------------------------------------------------
# Unscripted chats (logic in chats.py; worker dispatch via chats.CHAT_JOB_SLUG)
# ---------------------------------------------------------------------------


@app.post("/api/chats")
def api_create_chat() -> Response:
    """Create a new unscripted chat and redirect (303) into its chat page."""
    return chats.create_chat()


@app.get("/chats/{chat_id}", response_class=HTMLResponse)
def chat_page(chat_id: str) -> HTMLResponse:
    chats.check_chat_id(chat_id)
    info = paths.read_chat_info(chat_id)
    title = info.get("title") or f"Chat {chat_id}"
    return HTMLResponse(_render_base(f"Crack Chat: {_esc(title)}", chats.render_chat_page_body(chat_id)))


@app.get("/chats/{chat_id}/status", response_class=HTMLResponse)
def chat_status(chat_id: str) -> HTMLResponse:
    """Polling fragment returned while the agent is working on a reply."""
    chats.check_chat_id(chat_id)
    return HTMLResponse(chats.render_chat_content(chat_id))


@app.post("/api/chats/{chat_id}/messages", response_class=HTMLResponse)
def api_chat_message(
    chat_id: str,
    msg: str = Form(default=""),
    model: str = Form(default=""),
) -> HTMLResponse:
    """Append a user message, enqueue the agent, return the updated chat fragment."""
    return chats.post_message(chat_id, msg, model or None)


@app.post("/api/chats/{chat_id}/model", response_class=HTMLResponse)
def api_chat_model(chat_id: str, model: str = Form(...)) -> HTMLResponse:
    """Persist the chat's model selection (dropdown saves on change)."""
    return chats.set_model(chat_id, model)
