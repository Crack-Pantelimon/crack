"""FastAPI app: HTML editor + JSON API with htmx + pico.css."""

from __future__ import annotations

import html
import logging
import shlex
import subprocess
import textwrap
import time
from pathlib import Path

from fastapi import FastAPI, Form, HTTPException, Query
from fastapi.responses import HTMLResponse
from fastapi.staticfiles import StaticFiles

from crack_server import paths

STATIC_DIR = Path(__file__).resolve().parent / "static"

app = FastAPI(title="crack-pi-server")
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

# Use uvicorn's configured logger so INFO messages actually reach the console —
# the root logger has no handler attached under uvicorn's default logging config.
logger = logging.getLogger("uvicorn.error")

PI_MODEL = "google/gemma-4-31b-it"
PI_TIMEOUT_SECONDS = 120


PROMPT_TITLE_TEMPLATE = textwrap.dedent("""
    You are a helpful assistant that generates short, descriptive titles for prompts.

    Given the following prompt content, generate a short, descriptive title (max 60 characters).
    The title should be a concise summary of what the prompt is about.

    Prompt content:
    {content}

    Title:
""").strip()


def _esc(text: str) -> str:
    return html.escape(text, quote=True)


def _format_time(ts: float) -> str:
    """Format timestamp as YYYY-MM-DD HH:MM."""
    return time.strftime("%Y-%m-%d %H:%M", time.localtime(ts))


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
    return f"""
    <article class="task-card" style="border: 1px solid #ddd; border-radius: 8px; padding: 1rem; margin-bottom: 1rem;">
      <div style="display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem;">
        <div>
          <h3 style="margin: 0 0 0.5rem 0;"><a href="/tasks/{safe_id}" style="text-decoration: none;">{title}</a></h3>
          <small style="color: #666;">ID: {safe_id} • Created: {created} • Modified: {modified}</small>
        </div>
        <form hx-delete="/api/tasks/{safe_id}" hx-confirm="Delete task '{title}'?" hx-target="closest article" hx-swap="outerHTML swap:1s">
          <button type="submit" class="secondary" style="margin: 0;">Delete</button>
        </form>
      </div>
    </article>
    """


def _render_title_input(task_id: str, title: str) -> str:
    """Render the title <input> alone, so the regenerate-title endpoint can swap just
    this element (outerHTML) without resubmitting/saving the form."""
    safe_id = _esc(task_id)
    safe_title = _esc(title)
    return (
        f'<input type="text" name="title" id="title-input-{safe_id}" class="title-input" '
        f'value="{safe_title}" placeholder="Task title" '
        f'hx-put="/api/tasks/{safe_id}/info" hx-trigger="change delay:500ms, blur" '
        f'hx-target="closest header" hx-swap="outerHTML">'
    )


def _render_task_header(task_id: str, info: dict) -> str:
    """Render the task page header, including the editable title form. This is the only
    title in the UI — prompt rows no longer have their own titles."""
    safe_id = _esc(task_id)
    safe_title = _esc(info.get("title", task_id))
    created = _format_time(info.get("created_at", 0))
    modified = _format_time(info.get("modified_at", 0))
    title_input = _render_title_input(task_id, info.get("title", task_id))
    return f"""
    <header style="margin-bottom: 1.5rem;">
      <div class="title-row" style="margin-bottom: 1rem;">
        <h1 style="margin: 0; flex: 1;">{safe_title}</h1>
        <form hx-put="/api/tasks/{safe_id}/info" hx-target="closest header" hx-swap="outerHTML" style="flex: 1; display: flex; gap: 0.5rem; align-items: center;">
          {title_input}
          <button type="button" hx-post="/api/tasks/{safe_id}/regenerate-title" hx-target="#title-input-{safe_id}" hx-swap="outerHTML" class="secondary">Regenerate Title</button>
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

    import shutil

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
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return {"task_id": task_id, "info": paths.read_info(task_id)}


@app.put("/api/tasks/{task_id}/info")
def api_update_task_info(task_id: str, title: str = Form(...)) -> HTMLResponse:
    """Update the task title. Returns the re-rendered header fragment (target: closest
    header, swap: outerHTML) — the form submits x-www-form-urlencoded, not JSON."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    info = paths.read_info(task_id)
    info["title"] = title
    paths.write_info(task_id, info)
    return HTMLResponse(_render_task_header(task_id, info))


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
    (target: #prompt-list, swap: innerHTML)."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

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

    return HTMLResponse(_render_prompts_section(task_id))


@app.put("/api/tasks/{task_id}/prompts/{filename}")
def api_update_prompt(task_id: str, filename: str, content: str = Form(...)) -> HTMLResponse:
    """Save prompt content. Returns the re-rendered read-only row (target: closest
    article, swap: outerHTML) so the row toggles back from editable to non-editable."""
    try:
        paths.write_prompt(task_id, filename, content)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
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
    return HTMLResponse("")


def _run_pi_title_generation(prompt: str) -> str:
    """Run `pi` non-interactively to turn prompt content into a short title. Logs the
    full prompt, the exact command line, the timeout, the elapsed time, and an output
    summary so failures are diagnosable from server logs alone."""
    cmd = ["pi", "--model", PI_MODEL, "--print", "--no-session", "--no-tools", prompt]

    logger.info("regenerate-title: full prompt:\n%s", prompt)
    logger.info("regenerate-title: timeout=%ss", PI_TIMEOUT_SECONDS)
    logger.info("+ %s", shlex.join(cmd))

    start = time.monotonic()
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=PI_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired:
        elapsed = time.monotonic() - start
        logger.error("regenerate-title: pi timed out after %.2fs", elapsed)
        raise HTTPException(status_code=500, detail="pi command timed out")
    except FileNotFoundError:
        elapsed = time.monotonic() - start
        logger.error("regenerate-title: pi command not found on PATH (after %.2fs)", elapsed)
        raise HTTPException(status_code=500, detail="pi command not found")

    elapsed = time.monotonic() - start
    logger.info("regenerate-title: pi exited %d in %.2fs", result.returncode, elapsed)

    if result.returncode != 0:
        logger.error("regenerate-title: pi stderr:\n%s", result.stderr)
        raise HTTPException(status_code=500, detail=f"pi command failed: {result.stderr}")

    title = result.stdout.strip()
    logger.info("regenerate-title: output summary: %r", title[:200])
    return title


@app.post("/api/tasks/{task_id}/regenerate-title")
def api_regenerate_task_title(task_id: str) -> HTMLResponse:
    """Regenerate the task title from the combined content of its prompt files, using pi
    with the gemma-4-31b-it model. Returns the re-rendered title <input> (target: the
    title input, swap: outerHTML) — the new title is a draft in the input until the user
    clicks Save."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    prompts = paths.list_prompt_files(task_id)
    if not prompts:
        raise HTTPException(status_code=400, detail="no prompt files to summarize")

    contents = []
    for p in prompts:
        try:
            contents.append(paths.read_prompt(task_id, str(p["name"])))
        except FileNotFoundError:
            continue  # deleted between listing and reading
    content = "\n\n---\n\n".join(contents)

    prompt = PROMPT_TITLE_TEMPLATE.format(content=content)
    title = _run_pi_title_generation(prompt)

    return HTMLResponse(_render_title_input(task_id, title))


@app.get("/tasks/{task_id}", response_class=HTMLResponse)
def task_page(task_id: str) -> HTMLResponse:
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e

    info = paths.read_info(task_id)
    safe_id = _esc(task_id)
    safe_title = _esc(info.get("title", task_id))
    header = _render_task_header(task_id, info)
    next_name = paths.next_prompt_filename(task_id) or "prompt.md"

    body = f"""
    {header}
    <section id="prompt-list">
      <div hx-get="/tasks/{safe_id}/prompts-list" hx-trigger="load"></div>
    </section>

    <section class="add">
      <h2>Add Prompt</h2>
      <form hx-post="/api/tasks/{safe_id}/prompts" hx-target="#prompt-list" hx-swap="innerHTML" hx-on::after-request="this.reset()" style="display: flex; gap: 0.5rem; flex-wrap: wrap; align-items: flex-end;">
        <div style="flex: 1; min-width: 200px;">
          <label>Filename (optional) <input type="text" name="name" placeholder="blank = {_esc(next_name)}" pattern="[a-zA-Z0-9][a-zA-Z0-9_.-]*\\.md"></label>
        </div>
        <div style="flex: 2; min-width: 300px;">
          <label>Content <textarea name="content" rows="4" placeholder="Markdown content…" required></textarea></label>
        </div>
        <button type="submit">Add Prompt</button>
      </form>
    </section>
    """
    return HTMLResponse(_render_base(f"Crack Task: {safe_title}", body, task_id))


@app.get("/tasks/{task_id}/prompts-list", response_class=HTMLResponse)
def task_prompts_list(task_id: str) -> HTMLResponse:
    """Return the prompt list HTML fragment for htmx (initial load on the task page)."""
    try:
        paths.task_dir(task_id)
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return HTMLResponse(_render_prompts_section(task_id))


@app.get("/tasks/{task_id}/prompt-row/{filename}", response_class=HTMLResponse)
def prompt_row(task_id: str, filename: str, editing: bool = Query(default=False)) -> HTMLResponse:
    """Return one prompt row in view or edit mode (target: closest article, swap:
    outerHTML) — this is how Edit/Cancel toggle a row in place without a separate panel."""
    try:
        return HTMLResponse(_render_prompt_row(task_id, filename, editing=editing))
    except FileNotFoundError as e:
        raise HTTPException(status_code=404, detail="not found") from e
