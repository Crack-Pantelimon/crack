"""Shared HTML rendering helpers (leaf module — imports paths only).

Home of the escape/format/markdown helpers and the base-page renderer.
Chats and sub-agents import this module (never app.py).
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


# Raw HTML is disabled: anything the model emits renders as escaped text.
# GFM pipe tables are enabled on top of CommonMark.
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


def _format_duration(seconds: float) -> str:
    """Human duration for a run/exchange span (``42.3s`` / ``3m 12s`` / ``1h 5m``)."""
    seconds = max(0.0, float(seconds))
    if seconds < 60:
        return f"{seconds:.1f}s"
    mins = int(seconds // 60)
    secs = int(seconds % 60)
    if mins < 60:
        return f"{mins}m {secs}s"
    hours = mins // 60
    mins = mins % 60
    return f"{hours}h {mins}m"


def _render_sidebar() -> str:
    """Persistent left-nav: Home, Sub-agents, Settings, Chats (with status dots)."""
    from crack_server import chats

    chat_links = "".join(
        f'<a class="sidebar-chat-link" href="/chats/{_esc(cid)}">'
        f"{chats.render_chat_dot(cid)}"
        f"<span>{_esc(title)}</span></a>\n"
        for cid, title in chats.list_chat_links()
    ) or '<small class="muted">No chats</small>\n'
    return f"""
    <nav class="sidebar-nav">
      <a href="/"><strong>Home</strong></a>
      <a href="/sub_agents">Sub-agents</a>
      <a href="/rag">RAG search</a>
      <a href="/settings">Settings</a>
      <h6>Chats</h6>
      {chat_links}
    </nav>
    """


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
    """View/edit article for one web-editable file (sub-agent persona templates).

    View mode shows the read-only content plus an Edit button (hx-get on
    ``view_url?editing=true``); edit mode swaps the same row into a Save/Cancel
    form (hx-put on ``save_url``, Cancel hx-gets ``view_url``) — both target
    ``closest article`` with outerHTML. ``meta`` is the ``<small>`` header note
    (``"<size> bytes • <mtime>"``). ``extra_actions`` is optional pre-rendered
    HTML appended inside view mode's ``.actions`` div.

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
            f'{e3}<div class="file-row-header">\n'
            f'{e4}<label class="file-row-label">Filename <input type="text" value="{safe_name}" readonly></label>\n'
            f'{e4}<small class="muted">{safe_meta}</small>\n'
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
        f'{i2}<div class="file-row-header">\n'
        f'{i3}<span class="name">{safe_name}</span>\n'
        f'{i3}<small class="muted">{safe_meta}</small>\n'
        f"{i2}</div>\n"
        f'{i2}<textarea readonly rows="4">{safe_content}</textarea>\n'
        f'{i2}<div class="actions">\n'
        f"{actions}\n"
        f"{i2}</div>\n"
        f"{indent}</article>\n{indent}"
    )


def _render_base(title: str, body: str, right: str = "") -> str:
    """Render base HTML with class-based Pico CSS v2 + sidebar shell.

    ``right`` is an optional right-rail (same width as the left sidebar) — the
    chat page passes its sub-agent control tree here. Page-specific
    layout/customizations live in static/app.css; interaction JS in
    static/app.js (linked here, not inlined)."""
    right_aside = f'<aside class="sidebar right-sidebar">{right}</aside>' if right else ""
    return f"""<!DOCTYPE html>
<html lang="en" data-theme="light">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{_esc(title)}</title>
  <link
    rel="stylesheet"
    href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css"
  >
  <link rel="stylesheet" href="/static/app.css">
  <script
    src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.10/dist/htmx.min.js"
    integrity="sha384-H5SrcfygHmAuTDZphMHqBJLc3FhssKjG7w/CeCpFReSfwBWDTKpkzPP8c+cLsK+V"
    crossorigin="anonymous"
  ></script>
</head>
<body>
  <div class="layout">
    <aside class="sidebar">{_render_sidebar()}</aside>
    <main class="container-fluid">
      {body}
    </main>
    {right_aside}
  </div>
  <script src="/static/app.js"></script>
</body>
</html>"""
