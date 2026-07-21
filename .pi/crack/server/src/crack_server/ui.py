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
      <a href="/settings">Settings</a>
      <h6>Chats</h6>
      {chat_links}
    </nav>
    """


def _render_base(title: str, body: str) -> str:
    """Render base HTML with class-based Pico CSS v2 + sidebar shell.

    Page-specific layout/customizations live in static/app.css; interaction JS
    in static/app.js (linked here, not inlined)."""
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
  </div>
  <script src="/static/app.js"></script>
</body>
</html>"""
