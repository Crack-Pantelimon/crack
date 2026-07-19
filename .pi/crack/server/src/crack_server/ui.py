"""Shared HTML rendering helpers (leaf module — imports paths only).

Home of the escape/format/markdown helpers, the base-page renderer, and the
title-slot renderers. Stages and chats import this module (never app.py), so
there is no app↔stages import cycle.
"""

from __future__ import annotations

import html
import time

from markdown_it import MarkdownIt

from crack_server import paths


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
# summary cannot inject markup into the task page. GFM pipe tables are enabled on
# top of CommonMark — models routinely answer with tables (tool lists, comparisons).
_markdown = MarkdownIt("commonmark", {"html": False}).enable("table")


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

    No self-poller — app.js refetches ``title-regen-status`` when the task
    long-poll reports a state change. With ``oob=True`` the fragment carries the
    slot id + hx-swap-oob so prompt CRUD routes can refresh the header OOB."""
    safe_id = _esc(task_id)
    current_title = _esc(paths.read_info(task_id).get("title", task_id))
    inner = (
        f'<span class="title-input-pending" data-title-pending="1" aria-busy="true">'
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


def render_file_row(
    view_url: str,
    save_url: str,
    name: str,
    content: str,
    meta: str,
    editing: bool,
    *,
    extra_actions: str = "",
    indent: str = "",
) -> str:
    """View/edit article for one web-editable file (task prompts, stage templates).

    View mode shows the read-only content plus an Edit button (hx-get on
    ``view_url?editing=true``); edit mode swaps the same row into a Save/Cancel
    form (hx-put on ``save_url``, Cancel hx-gets ``view_url``) — both target
    ``closest article`` with outerHTML. ``meta`` is the ``<small>`` header note
    (``"<size> bytes • <mtime>"``). ``extra_actions`` is optional pre-rendered
    HTML appended inside view mode's ``.actions`` div (prompt rows pass their
    Remove form at ``indent + 4 spaces``; template rows pass nothing).

    ``indent`` is the view-mode article indent; edit mode renders one level
    (4 spaces) deeper, matching the historic call-site layouts."""
    safe_view = _esc(view_url)
    safe_save = _esc(save_url)
    safe_name = _esc(name)
    safe_content = _esc(content)
    safe_meta = _esc(meta)

    if editing:
        e1 = indent + "    "
        e2, e3, e4 = e1 + "  ", e1 + "    ", e1 + "      "
        return (
            f'\n{e1}<article class="prompt-row">\n'
            f'{e2}<form hx-put="{safe_save}" hx-target="closest article" hx-swap="outerHTML">\n'
            f'{e3}<div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem;">\n'
            f'{e4}<label style="flex: 1;">Filename <input type="text" value="{safe_name}" readonly></label>\n'
            f'{e4}<small style="color: #666;">{safe_meta}</small>\n'
            f'{e3}</div>\n'
            f"{e3}<label>Content\n"
            f'{e4}<textarea name="content" rows="12" required>{safe_content}</textarea>\n'
            f"{e3}</label>\n"
            f'{e3}<div class="actions">\n'
            f"{e4}<button type=\"submit\">Save</button>\n"
            f'{e4}<button type="button" hx-get="{safe_view}" hx-target="closest article" hx-swap="outerHTML" class="secondary">Cancel</button>\n'
            f"{e3}</div>\n"
            f"{e2}</form>\n"
            f"{e1}</article>\n{e1}"
        )

    i2, i3 = indent + "  ", indent + "    "
    actions = (
        f'{i3}<button hx-get="{safe_view}?editing=true" hx-target="closest article" hx-swap="outerHTML">Edit</button>'
    )
    if extra_actions:
        actions += "\n" + extra_actions
    return (
        f'\n{indent}<article class="prompt-row">\n'
        f'{i2}<div style="display: flex; justify-content: space-between; align-items: center; gap: 0.5rem;">\n'
        f'{i3}<span class="name">{safe_name}</span>\n'
        f'{i3}<small style="color: #666;">{safe_meta}</small>\n'
        f"{i2}</div>\n"
        f'{i2}<textarea readonly rows="4">{safe_content}</textarea>\n'
        f'{i2}<div class="actions">\n'
        f"{actions}\n"
        f"{i2}</div>\n"
        f"{indent}</article>\n{indent}"
    )


def _render_prompt_row(task_id: str, filename: str, editing: bool = False) -> str:
    """Render one prompt row. View mode always shows the file content (read-only);
    Edit mode swaps the same row (closest article) into an editable form in place."""
    content = paths.read_prompt(task_id, filename)  # raises FileNotFoundError if missing

    stat = (paths.task_dir(task_id) / filename).stat()
    meta = f"{stat.st_size} bytes • {_format_time(stat.st_mtime)}"

    safe_id = _esc(task_id)
    safe_name = _esc(filename)
    extra_actions = (
        f'<form hx-delete="/api/tasks/{safe_id}/prompts/{safe_name}" hx-target="closest article" hx-swap="outerHTML swap:1s" hx-confirm="Delete \'{safe_name}\'?" style="margin: 0;">\n'
        '          <button type="submit" class="secondary" style="color: #c44; border-color: #c44;">Remove</button>\n'
        "        </form>"
    )
    return render_file_row(
        f"/tasks/{task_id}/prompt-row/{filename}",
        f"/api/tasks/{task_id}/prompts/{filename}",
        filename,
        content,
        meta,
        editing,
        extra_actions="        " + extra_actions,
        indent="    ",
    )


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
