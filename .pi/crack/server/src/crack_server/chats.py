"""Unscripted chats: free-form pi chat sessions outside the task pipeline.

Chats live under ``.pi/crack/unscripted_chats/<chat_id>/`` (``info.json``,
``chat.json``, ``sessions/``) with a ms-epoch id. The web process only writes
state and enqueues ``CHAT_JOB_SLUG`` jobs; the worker runs the agent here via
``run_chat`` with *all* pi tools enabled (``tools=None``), resuming the chat's
own pi session across messages.
"""

from __future__ import annotations

import logging
import time

from fastapi import HTTPException
from fastapi.responses import HTMLResponse, RedirectResponse

from crack_server import app as _ui
from crack_server import models as models_mod
from crack_server import paths, pi_runner, queue
from crack_server.stages.base import (
    _clean_turn_text,
    render_error_msg,
    render_spinner,
    render_turns_trajectory,
)

logger = logging.getLogger("uvicorn.error")

# Pseudo-stage slug for the non-stage chat job on the queue (see worker.py).
CHAT_JOB_SLUG = "__chat__"

DEFAULT_CHAT_MODEL = "nvidia/z-ai/glm-5.2"

CHAT_TURNS_PER_HOP = 10
CHAT_MAX_HOPS = 3
CHAT_MAX_TURNS = 30
CHAT_TIMEOUT_SECONDS = 1800

RECENT_CHATS = 5


def check_chat_id(chat_id: str) -> None:
    """404 on malformed or unknown chat ids (mirrors app._check_task_id)."""
    try:
        directory = paths.chat_dir(chat_id)
    except ValueError:
        raise HTTPException(status_code=404, detail="chat not found") from None
    if not directory.is_dir():
        raise HTTPException(status_code=404, detail="chat not found")


# -- home-page section --------------------------------------------------------


def _render_chat_list(ids: list[str]) -> str:
    if not ids:
        return '<p style="color: #888;">No chats yet.</p>'
    items = []
    for cid in ids:
        info = paths.read_chat_info(cid)
        title = info.get("title") or "(untitled chat)"
        created = info.get("created_at")
        when = _ui._format_time(created) if created else ""
        items.append(
            f'<li><a href="/chats/{_ui._esc(cid)}">{_ui._esc(title)}</a> '
            f'<small style="color: #666;">({_ui._esc(cid)}{" · " + when if when else ""})</small></li>'
        )
    return "<ul>" + "".join(items) + "</ul>"


def render_home_section() -> str:
    """The 'Unscripted Chats' block appended to the home page body."""
    ids = paths.list_chat_ids()
    recent = _render_chat_list(ids[:RECENT_CHATS])
    rest = ""
    if len(ids) > RECENT_CHATS:
        rest = (
            f"<details><summary>All chats ({len(ids)})</summary>"
            f"{_render_chat_list(ids)}</details>"
        )
    return f"""
    <hr>
    <section id="unscripted-chats" style="margin-top: 2rem;">
      <h2>Unscripted Chats</h2>
      <form method="post" action="/api/chats" style="margin-bottom: 1rem;">
        <button type="submit">New Chat</button>
      </form>
      {recent}
      {rest}
    </section>
    """


# -- chat page ----------------------------------------------------------------


def render_chat_answer(turns: list[dict]) -> str:
    """One exchange's agent output: the read/think/tool trajectory as a compact
    table (assistant text excluded), then that assistant text rendered as
    markdown. The model answers in markdown, so it must be rendered as HTML
    rather than shown as an escaped snippet in the actions table."""
    parts: list[str] = []
    trajectory = render_turns_trajectory(turns, include_text=False)
    if trajectory:
        parts.append(trajectory)
    answer = "\n\n".join(
        cleaned
        for turn in turns
        if (cleaned := _clean_turn_text(turn.get("text", "")))
    )
    if answer:
        parts.append(
            '<div class="stage-msg chat-assistant"><strong>Assistant:</strong>'
            f"{_ui._render_markdown(answer)}</div>"
        )
    return "".join(parts)


def render_chat_form(chat_id: str, info: dict) -> str:
    """Bottom form: cached-model dropdown (saves on change) + multiline input + Send."""
    current = info.get("model") or DEFAULT_CHAT_MODEL
    options = models_mod.get_models()
    if current not in options:
        options = [current] + options
    opts = "".join(
        f'<option value="{_ui._esc(m)}"{" selected" if m == current else ""}>{_ui._esc(m)}</option>'
        for m in options
    )
    safe_id = _ui._esc(chat_id)
    return f"""
    <form class="stage-msg chat-form" hx-post="/api/chats/{safe_id}/messages"
          hx-target="#chat-content" hx-swap="outerHTML">
      <label>Model
        <select name="model" hx-post="/api/chats/{safe_id}/model"
                hx-trigger="change" hx-swap="none">
          {opts}
        </select>
      </label>
      <label>Message
        <textarea name="msg" rows="4" required placeholder="Type a message…"></textarea>
      </label>
      <button type="submit">Send</button>
    </form>
    """


def render_chat_content(chat_id: str) -> str:
    """Chat exchanges + status + form. This is also the htmx polling fragment."""
    info = paths.read_chat_info(chat_id)
    state = paths.read_chat_state(chat_id)
    phase = state.get("phase")
    parts: list[str] = []

    for exchange in state.get("exchanges", []):
        parts.append(
            f'<div class="stage-msg chat-user"><strong>You:</strong> '
            f"{_ui._esc(exchange.get('user', ''))}</div>"
        )
        turns = exchange.get("turns", [])
        if turns:
            parts.append(render_chat_answer(turns))

    if phase != "chatting" and state.get("error"):
        parts.append(render_error_msg(state.get("error", ""), state.get("error_detail", "")))

    if phase == "chatting":
        parts.append(render_spinner("Thinking…"))

    parts.append(render_chat_form(chat_id, info))

    poll = ""
    if phase == "chatting":
        poll = (
            f' hx-get="/chats/{_ui._esc(chat_id)}/status"'
            ' hx-trigger="every 1.5s" hx-swap="outerHTML"'
        )
    return f'<div id="chat-content"{poll}>{"".join(parts)}</div>'


def render_chat_page_body(chat_id: str) -> str:
    info = paths.read_chat_info(chat_id)
    title = info.get("title") or f"Chat {chat_id}"
    return f"""
    <header style="margin-bottom: 1rem;">
      <p><a href="/">← Home</a></p>
      <h1>{_ui._esc(title)}</h1>
      <p><small style="color: #666;">id {_ui._esc(chat_id)} · all tools enabled</small></p>
    </header>
    {render_chat_content(chat_id)}
    """


# -- route handlers (registered in app.py) -------------------------------------


def create_chat() -> RedirectResponse:
    """POST /api/chats: create a chat and redirect into its page."""
    chat_id = paths.generate_chat_id()
    paths.create_chat(chat_id, DEFAULT_CHAT_MODEL)
    logger.info("chats: created %s", chat_id)
    return RedirectResponse(url=f"/chats/{chat_id}", status_code=303)


def post_message(chat_id: str, msg: str, model: str | None) -> HTMLResponse:
    """POST /api/chats/{id}/messages: queue the agent for a new user message."""
    check_chat_id(chat_id)
    if model:
        info = paths.read_chat_info(chat_id)
        info["model"] = model
        paths.write_chat_info(chat_id, info)
    msg = msg.strip()
    if msg:
        state = paths.read_chat_state(chat_id)
        state.setdefault("exchanges", []).append({"user": msg, "turns": []})
        state["phase"] = "chatting"
        paths.write_chat_state(chat_id, state)
        queue.enqueue(chat_id, CHAT_JOB_SLUG, "chat")
    return HTMLResponse(render_chat_content(chat_id))


def set_model(chat_id: str, model: str) -> HTMLResponse:
    """POST /api/chats/{id}/model: persist the dropdown selection."""
    check_chat_id(chat_id)
    info = paths.read_chat_info(chat_id)
    info["model"] = model
    paths.write_chat_info(chat_id, info)
    return HTMLResponse("")


# -- worker entry point ---------------------------------------------------------


def run_chat(chat_id: str) -> None:
    """Worker side of a CHAT_JOB_SLUG job: run the agent for the latest message.

    Mirrors s06_finished._run_chat, but with the chat's own pi session and
    ``tools=None`` (every built-in + extension tool, including MCP)."""
    start = time.monotonic()
    try:
        state = paths.read_chat_state(chat_id)
        exchanges = state.get("exchanges", [])
        if not exchanges:
            state["phase"] = "idle"
            paths.write_chat_state(chat_id, state)
            return
        idx = len(exchanges) - 1
        message = exchanges[idx].get("user", "")
        model = paths.read_chat_info(chat_id).get("model") or DEFAULT_CHAT_MODEL

        existing = list(exchanges[idx].get("turns", []))
        new_turns: list[dict] = []

        def persist(current_turn: dict, hop: int) -> None:
            new_turns.append(
                {
                    "hop": hop,
                    "text": current_turn.get("text", ""),
                    "thinking": current_turn.get("thinking", ""),
                    "tool_blocks": list(current_turn.get("tool_blocks", [])),
                    "elapsed": current_turn.get("elapsed"),
                }
            )
            st = paths.read_chat_state(chat_id)
            st["exchanges"][idx]["turns"] = existing + new_turns
            paths.write_chat_state(chat_id, st)

        reason = "hop_cap"
        hop = 0
        while reason == "hop_cap" and hop < CHAT_MAX_HOPS:
            hop += 1
            reason = pi_runner.run_agent_hop(
                log_prefix="unscripted-chat",
                model=model,
                session_id=f"unscripted-{chat_id}",
                sessions_dir=paths.chat_sessions_dir(chat_id),
                tools=None,
                message=message,
                start=start,
                sentinel=None,
                turns_per_hop=CHAT_TURNS_PER_HOP,
                max_turns=CHAT_MAX_TURNS,
                timeout_seconds=CHAT_TIMEOUT_SECONDS,
                total_turns=pi_runner.count_turn_groups(existing + new_turns),
                persist_turn=persist,
                hop=hop,
            )
            if reason != "hop_cap":
                break
            message = "Continue your response."

        state = paths.read_chat_state(chat_id)
        state["phase"] = "idle"
        if reason == "empty":
            state["error"] = "model returned empty responses"
            state["error_detail"] = ""
        paths.write_chat_state(chat_id, state)
        logger.info("chats: exchange %d done for %s (reason=%s)", idx, chat_id, reason)
    except Exception as e:
        logger.exception("unscripted chat failed for %s", chat_id)
        state = paths.read_chat_state(chat_id)
        state["phase"] = "idle"
        state["error"] = str(e)
        state["error_detail"] = getattr(e, "detail", "")
        paths.write_chat_state(chat_id, state)
