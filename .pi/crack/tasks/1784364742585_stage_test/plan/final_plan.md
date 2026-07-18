# Plan

## Initial build/check instructions
```bash
# Verify server is running and accessible
curl -s http://localhost:9847/ | head -20

# Check Python syntax / imports
docker exec crack-dev python -m py_compile src/crack_server/app.py
docker exec crack-dev python -m py_compile src/crack_server/paths.py
```

## Problem statement
The task page (`/tasks/{task_id}`) currently has no footer identification. Users need a subtle server-name note at the bottom of the main content area to confirm which server instance they're interacting with. The HTML for the task page is assembled in `task_page()` (src/crack_server/app.py:451-482), which builds a body fragment and passes it to `_render_base()` (src/crack_server/app.py:130-190). The base template wraps the body in `<main>` and includes the static JS. Static assets (CSS/JS) live in `src/crack_server/static/`. The footer should appear on task pages only, inside `<main>` after all dynamic content (prompt list, stage tabs) but before `</main>`.

## Changes

### 1. `src/crack_server/app.py` — `_render_base()` (lines 130-190)
Add an optional `footer` parameter and inject it inside `<main>` after the body content.

**Before (excerpt):**
```python
def _render_base(
    title: str,
    body: str,
    *,
    task_id: str | None = None,
    extra_head: str = "",
) -> str:
    return f"""<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title}</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css">
    <link rel="stylesheet" href="/static/app.css">
    {extra_head}
  </head>
  <body data-task-id="{task_id or ''}">
    <main>
      {body}
    </main>
    <script src="/static/app.js"></script>
  </body>
</html>"""
```

**After:**
```python
def _render_base(
    title: str,
    body: str,
    *,
    task_id: str | None = None,
    extra_head: str = "",
    footer: str = "",
) -> str:
    return f"""<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title}</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css">
    <link rel="stylesheet" href="/static/app.css">
    {extra_head}
  </head>
  <body data-task-id="{task_id or ''}">
    <main>
      {body}
      {footer}
    </main>
    <script src="/static/app.js"></script>
  </body>
</html>"""
```

**Motivation:** Makes footer injection opt-in and reusable; only callers that pass `footer=` will render one.

---

### 2. `src/crack_server/app.py` — `task_page()` (lines 451-482)
Add a small helper to render the footer fragment and pass it to `_render_base`.

**Before (excerpt):**
```python
@app.get("/tasks/{task_id}", response_class=HTMLResponse)
def task_page(task_id: str) -> HTMLResponse:
    info = read_info(task_id)
    title = info.get("title") or task_id
    body = f"""
      {_render_task_header(task_id, info)}
      {_render_prompts_section(task_id)}
      {_render_stage_tabs(task_id)}
    """
    return HTMLResponse(_render_base(f"{title} · crack-pi-server", body, task_id=task_id))
```

**After:**
```python
def _render_server_footer() -> str:
    """Small footer note shown at bottom of task pages."""
    return '<footer class="server-footer">crack-pi-server</footer>'

@app.get("/tasks/{task_id}", response_class=HTMLResponse)
def task_page(task_id: str) -> HTMLResponse:
    info = read_info(task_id)
    title = info.get("title") or task_id
    body = f"""
      {_render_task_header(task_id, info)}
      {_render_prompts_section(task_id)}
      {_render_stage_tabs(task_id)}
    """
    return HTMLResponse(_render_base(
        f"{title} · crack-pi-server",
        body,
        task_id=task_id,
        footer=_render_server_footer(),
    ))
```

**Motivation:** Scopes footer to task pages only; keeps HTML fragment small and styleable via CSS class.

---

### 3. `src/crack_server/static/app.css` — Add footer styles
Append styles for the new footer class.

**Add at end of file:**
```css
/* Server footer at bottom of task page main content */
main > footer.server-footer {
  margin-top: 2rem;
  padding-top: 1rem;
  border-top: 1px solid var(--border-color, #e5e5e5);
  text-align: center;
  font-size: 0.75rem;
  color: var(--muted-color, #888);
}
```

**Motivation:** Pico.css uses CSS variables for colors; this matches the muted/small aesthetic used elsewhere (e.g., `.htmx-indicator`). The top margin separates footer from stage tabs; border-top provides a subtle visual break.

---

## What NOT to change
- **`_render_base` signature for other callers** — `index()` and any future routes must continue to work without passing `footer=` (default `""` handles this).
- **Homepage (`/`)** — must not show the server footer.
- **Static asset mounting** — `app.mount("/static", ...)` already configured; no changes needed.
- **Task ID format, prompt CRUD, stage pipeline** — completely unrelated.
- **`app.js`** — no JS needed for a static footer.

---

## Automatic verification
```bash
# 1. Syntax check
docker exec crack-dev python -m py_compile src/crack_server/app.py

# 2. Verify task page HTML contains footer (create a test task first)
TASK_ID=$(curl -s -X POST http://localhost:9847/api/tasks -d "title=Footer Test" | jq -r .id)
curl -s "http://localhost:9847/tasks/$TASK_ID" | grep -c 'class="server-footer"'
# Expected: 1

# 3. Verify homepage does NOT contain footer
curl -s http://localhost:9847/ | grep -c 'class="server-footer"'
# Expected: 0

# 4. Clean up test task
curl -s -X DELETE "http://localhost:9847/api/tasks/$TASK_ID"
```

---

## Manual verification
1. Open `http://localhost:9847/tasks/<any_task_id>` in browser
2. Scroll to bottom of main content (below stage tabs)
3. Verify small centered text "crack-pi-server" appears with muted color, subtle top border, and ~2rem spacing above it
4. Open homepage `http://localhost:9847/` — confirm no footer appears
5. Open any stage config page (`/stages/explore`, `/stages/plan`) — confirm no footer appears
6. Inspect element: footer should be `<footer class="server-footer">crack-pi-server</footer>` inside `<main>`

---

## Overview / Summary
**Goal:** Add a subtle server-name footer ("crack-pi-server") at the bottom of every task page only.

**Solution shape:** 
- Extend `_render_base()` with optional `footer` parameter (backward compatible)
- `task_page()` passes a small footer fragment via new helper `_render_server_footer()`
- CSS styles the footer as muted, centered, small text with top border/margin

**Main risks:** 
- Low — change is additive, default parameter preserves all existing callers
- Hot reload picks up Python changes in ~1s; CSS changes may need hard refresh (Ctrl+F5)

Remember: DO NOT write or edit any files yet. This is a read-only exploration and planning phase.
