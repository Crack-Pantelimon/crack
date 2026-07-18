# Plan

## Initial build/check instructions
The server runs live in a Docker container at `http://localhost:9847` with
`uvicorn reload=True`, so editing any `.py` under `src/crack_server/` is picked
up in ~1 second — no rebuild/restart needed. CSS edits under `static/` are
served immediately too (may need a hard browser refresh to defeat cache).

```bash
# Verify server is responding
curl -s http://localhost:9847/ | head -20

# Verify the task page renders
curl -s http://localhost:9847/api/tasks | jq -r '.[0].id' \
  | xargs -I{} curl -s "http://localhost:9847/tasks/{}" | head -30

# Syntax check the edited module
python -m py_compile src/crack_server/app.py
```

## Problem statement
The crack-pi-server **task page** (`GET /tasks/{task_id}`, the `task_page()`
function at `src/crack_server/app.py:811`) currently has no footer identifying
the server instance. The goal is a small, muted footer note showing the
server name at the bottom of the main content area, **inside `<main>`**, after
all dynamic content (prompt list, Add Prompt `<details>`, stage tabs/panels).

Per the plan review Q&A, the agreed decisions are:

- **Scope: task page only.** The footer must NOT appear on the home page (`/`)
  or stage config pages (`/stages/<slug>`). It is therefore *not* added to the
  shared `_render_base()` layout helper; instead a footer fragment is appended
  to the `body` string built inside `task_page()`. Because `_render_base`
  always wraps `{body}` inside `<main>`, appending to `body` guarantees the
  footer lands inside `<main>` (verified against actual code, line 77).
- **Server name is configurable via env var.** Following the existing
  `CRACK_PI_*` convention (`CRACK_PI_HOST`, `CRACK_PI_PORT`,
  `CRACK_PI_PROJECT_ROOT` in `main.py`/`paths.py`), read
  `os.environ.get("CRACK_PI_FOOTER_NAME", "crack-pi-server")` directly inside
  `task_page()`. (This resolves the Round-2 name conflict: the final agreed
  env var is `CRACK_PI_FOOTER_NAME`.)
- **Use the ACTUAL code structure.** `_render_base` is
  Pico *classless* — `<main>` has no `class="container"` — and a
  `<script src="/static/app.js">` follows `</main>`. The plan targets the real
  `<main>{body}</main>` block; earlier "class=container" sketch details were
  illustrative only.
- **Styling: inline now AND extract to a CSS class in the same change.** Use a
  task-scoped class `.task-page-footer` (not the generic `.page-footer`) to
  avoid collision with future classes, added to `static/app.css` — consistent
  with the existing task-scoped section classes (`.add`, `.explore-meta`,
  `.plan-meta`).
- **Placement: inside `<main>`, after the body content.** It scrolls with the
  page (task pages can be tall due to htmx-swapped Explore/Plan panels); a
  non-sticky flow footer is the intended behavior here.

## Changes

### 1. `src/crack_server/app.py` — ensure `os` is imported (top of file)

`os` is **not** currently imported in `app.py` (verified: the import block at
the top has `html`, `logging`, `shutil`, `threading`, `time`, `pathlib`, fastapi,
markdown_it, and the `crack_server.*` submodules — no `import os`). Add it in
the stdlib group so `os.environ.get` works.

**Edit (near `import html` / `import logging`):**
```python
import html
import logging
import os
import shutil
import threading
import time
```

**Motivation:** `os.environ.get` is needed for the configurable server name.
(`main.py` already imports `os`; `paths.py` already does too — this just
brings `app.py` in line.)

---

### 2. `src/crack_server/app.py` — `task_page()` (lines 811-841): append footer to `body`

Append a `<footer class="task-page-footer">…</footer>` fragment to the end of
the `body` f-string (after `{tabs_panels}`), then pass the unchanged body to
`_render_base`. The footer content is the escaped, env-var-driven server name
inside a `<small>`.

**Before (excerpt, lines 811-841):**
```python
def task_page(task_id: str) -> HTMLResponse:
    _check_task_id(task_id)

    info = paths.read_info(task_id)
    safe_id = _esc(task_id)
    safe_title = _esc(info.get("title", task_id))
    header = _render_task_header(task_id, info)
    next_name = paths.next_prompt_filename(task_id) or "prompt.md"
    tabs_nav, tabs_panels, _ = _render_stage_tabs(task_id)

    body = f"""
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
    {tabs_panels}
    """
    return HTMLResponse(_render_base(f"Crack Task: {safe_title}", body, task_id))
```

**After:**
```python
def task_page(task_id: str) -> HTMLResponse:
    _check_task_id(task_id)

    info = paths.read_info(task_id)
    safe_id = _esc(task_id)
    safe_title = _esc(info.get("title", task_id))
    header = _render_task_header(task_id, info)
    next_name = paths.next_prompt_filename(task_id) or "prompt.md"
    tabs_nav, tabs_panels, _ = _render_stage_tabs(task_id)

    footer_name = _esc(os.environ.get("CRACK_PI_FOOTER_NAME", "crack-pi-server"))
    body = f"""
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
    {tabs_panels}
    <footer class="task-page-footer"><small>{footer_name}</small></footer>
    """
    return HTMLResponse(_render_base(f"Crack Task: {safe_title}", body, task_id))
```

**Motivation / decisions encoded:**
- Footer is appended *inside `body`*, so `_render_base`'s `<main>{body}</main>`
  wrap guarantees placement inside `<main>`. No `_render_base` signature change
  — other callers (`index()`, stage config) are untouched and never render a
  footer, satisfying "task page only".
- `os.environ.get("CRACK_PI_FOOTER_NAME", "crack-pi-server")` matches the
  `CRACK_PI_*` env-var convention. Fallback literal matches the package name in
  `pyproject.toml`.
- Value is wrapped in `_esc(...)` (the project's existing HTML-escape helper)
  before interpolation, so a configured name containing `<`, `&`, etc. cannot
  inject markup or break the page.
- `<small>` keeps the text small per the existing muted-small pattern seen
  elsewhere (task cards: `<small style="color: #666;">…</small>`); the
  `.task-page-footer` class controls margin/border/color.

---

### 3. `src/crack_server/static/app.css` — add `.task-page-footer` styling

Append a task-scoped footer block. Keep it consistent with the existing
section classes (`.add`: `margin-top: 1.5rem; padding-top: 1rem; border-top: 1px
solid #ccc;`; `.explore-meta` / `.plan-meta` use `color: #666;`).

**Add near the existing `.add` rule (top section of app.css) or at end of file:**
```css
/* Server footer at bottom of task page main content (task pages only) */
.task-page-footer {
  margin-top: 2rem;
  padding-top: 1rem;
  border-top: 1px solid #ccc;
  text-align: center;
}

.task-page-footer small {
  color: #666;
}
```

**Motivation:** Plain hardcoded values (`#ccc` border, `#666` muted text) match
the rest of `app.css`, which consistently uses literal hex colors (the file does
not rely on Pico CSS variables for these section styles). `text-align: center`
gives the centered "small note" look. No `max-width` override needed — `<main>`
already centers content (`max-width: 52rem; margin: 0 auto;`).

---

## What NOT to change
- **`_render_base()` signature or body** — it stays as-is; the footer is added
  by appending to `body` in `task_page()`, not by a new `footer=` parameter.
- **Homepage `index()` and stage config pages** — they do not pass footer
  markup, so they continue to render no footer. (This is the mechanism that
  enforces "task page only.")
- **`src/crack_server/main.py` / `src/crack_server/paths.py`** — unrelated to
  rendering; `main.py` already imports `os` and that is not touched here.
- **Any API route handlers** (`api_create_task`, `api_delete_prompt`, the
  title-regen / explore / plan endpoints) — they return JSON or HTML fragments,
  never the base layout.
- **The `task_page()` `_render_task_header` / `_render_prompts_section` /
  `_render_stage_tabs` helpers** — only the `body` f-string literal gains one
  trailing line.
- **Static asset mount** (`app.mount("/static", ...)`) — already configured.
- **`app.js`** — no JS needed for a static footer.

---

## Automatic verification
```bash
# 1. Syntax check the edited module
python -m py_compile src/crack_server/app.py

# 2. Task page contains exactly one footer (use the scoped class, not the name string,
#    so a default or custom name both match and we don't depend on the literal text)
TASK_ID=$(curl -s -X POST http://localhost:9847/api/tasks -d "title=Footer Test" | jq -r .id)
curl -s "http://localhost:9847/tasks/$TASK_ID" | grep -c 'class="task-page-footer"'
# Expected: 1

# 3. Footer is inside <main> (the <footer> appears between the tabs and </main>)
curl -s "http://localhost:9847/tasks/$TASK_ID" | sed -n '/<main>/,/<\/main>/p' | grep -c 'class="task-page-footer"'
# Expected: 1

# 4. Homepage does NOT contain the footer
curl -s http://localhost:9847/ | grep -c 'class="task-page-footer"'
# Expected: 0

# 5. Stage config pages do NOT contain the footer
curl -s http://localhost:9847/stages/plan | grep -c 'class="task-page-footer"'
# Expected: 0

# 6. Default name renders when env var unset (server process must NOT have
#    CRACK_PI_FOOTER_NAME in its environment to see the fallback)
curl -s "http://localhost:9847/tasks/$TASK_ID" | grep -o '<footer class="task-page-footer"><small>[^<]*</small></footer>'
# Expected: <footer class="task-page-footer"><small>crack-pi-server</small></footer>

# 7. Configured name renders when env var is set (verify locally, not against
#    the running container, since the live container's env is fixed). With the
#    var exported before starting uvicorn, the value would appear verbatim.
export CRACK_PI_FOOTER_NAME="my-test-instance"
python -c "import os;from crack_server.app import task_page" 2>/dev/null; echo "env name: $CRACK_PI_FOOTER_NAME"

# 8. HTML escaping: a name with markup characters is escaped (set the env var,
#    restart the server, check the footer text is escaped — manual sanity, see #7)
unset CRACK_PI_FOOTER_NAME

# Cleanup
curl -s -X DELETE "http://localhost:9847/api/tasks/$TASK_ID"
```

Steps 2-6 should each print the shown `0`/`1`. Step 1 exits 0 on success.

---

## Manual verification
1. Open `http://localhost:9847/tasks/<any_task_id>` in a browser.
2. Scroll to the bottom of the main content (below the stage tabs/panels).
3. Verify a small centered "crack-pi-server" note appears with a subtle top
   border and ~2rem of space above it, in muted gray (`#666`).
4. Open the homepage `http://localhost:9847/` — confirm **no** footer appears.
5. Open a stage config page (`/stages/explore`, `/stages/plan`) — confirm **no**
   footer appears.
6. Inspect element: confirm the markup is
   `<footer class="task-page-footer"><small>crack-pi-server</small></footer>`
   and that it sits **inside** `<main>` (sibling to the prompt/stage content,
   before `</main>`, before the `<script src="/static/app.js">` that follows
   `</main>`).
7. (Optional) Set `CRACK_PI_FOOTER_NAME` in the server's environment, restart,
   and confirm the configured value is rendered verbatim (and escaped if it
   contains `<` / `&`).

---

## Overview / Summary
**Goal:** Add a small, muted footer note with the (configurable) server name at
the bottom of the **task page only**, inside `<main>`.

**Solution:**
- Add `import os` to `app.py`.
- In `task_page()`, read
  `os.environ.get("CRACK_PI_FOOTER_NAME", "crack-pi-server")`, HTML-escape it
  via `_esc()`, and append
  `<footer class="task-page-footer"><small>{name}</small></footer>` to the
  `body` f-string (after `{tabs_panels}`).
- Add a `.task-page-footer` rule (margin-top, top border, centered) and a
  `.task-page-footer small` rule (`color: #666`) to `static/app.css`.

**Why this shape:**
- Appending to `body` (not a `_render_base` parameter) keeps the change scoped
  to the task page and guarantees placement inside `<main>` without touching the
  shared layout helper or any other caller.
- The `CRACK_PI_FOOTER_NAME` env var follows the existing project convention
  and makes the server instance identifiable when multiple instances run.
- `_esc()` prevents configured values containing `<`/`&` from injecting markup.
- `.task-page-footer` mirrors the existing task-scoped class naming (`.add`,
  `.explore-meta`) and avoids colliding with a future generic footer class.

**Main risks / gotchas:**
- The live Docker container's environment is fixed; the env var only takes
  effect if set before the server starts. The `"crack-pi-server"` fallback
  covers the default (unset) case seen by the running container.
- `os` is not currently imported in `app.py` — step 1 adds it. Forgetting this
  causes `NameError: name 'os' is not defined` on the first task-page request.
- CSS may be browser-cached; a hard refresh (Ctrl+F5) may be needed for manual
  verification of the new `.task-page-footer` styling. Auto-reload handles the
  Python side; CSS is served from `/static` directly.
