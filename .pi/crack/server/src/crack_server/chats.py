"""Unscripted chats: free-form pi chat sessions with recursive sub-agents.

Chats live under ``.pi/crack/unscripted_chats/<chat_id>/`` (``info.json``,
``chat.json``, ``sessions/``) with a ms-epoch id. The web process only writes
state and enqueues ``CHAT_JOB_SLUG`` jobs; the worker runs the agent here via
``run_chat`` with *all* pi tools enabled (``tools=None``), resuming the chat's
own pi session across messages.
"""

from __future__ import annotations

import logging
import re
import shutil
import time

from fastapi import HTTPException
from fastapi.responses import HTMLResponse, RedirectResponse

from crack_server import ui as _ui
from crack_server import attachments, chat_engine, context_stats, git_utils
from crack_server import paths, patch, pi_runner, queue, sandbox, settings as _settings, titles
from crack_server.chat_engine import MAX_CHAT_HOPS
from crack_server.state import chat_state_mtime

logger = logging.getLogger("uvicorn.error")

# Pseudo-stage slug for the non-stage chat job on the queue (see worker.py).
CHAT_JOB_SLUG = "__chat__"

DEFAULT_CHAT_MODEL = "nvidia/z-ai/glm-5.2"

CHAT_TIMEOUT_SECONDS = 3600

RECENT_CHATS = 5

# Pseudo-slug used for msg/tail ids (mirrors Stage.slug).
CHAT_SLUG = "chat"


def _render():
    """Lazy import to avoid app ↔ chats ↔ render circular imports at load time."""
    from crack_server import render as render_mod

    return render_mod


def check_chat_id(chat_id: str) -> None:
    """404 on malformed or unknown chat ids (mirrors app._check_task_id)."""
    try:
        directory = paths.chat_dir(chat_id)
    except ValueError:
        raise HTTPException(status_code=404, detail="chat not found") from None
    if not directory.is_dir():
        raise HTTPException(status_code=404, detail="chat not found")


def _agent_pid_file(chat_id: str):
    """Where the worker publishes the running pi subprocess's pid so the web
    STOP handler can kill it (see pi_runner.run_agent_hop / kill_pid_file)."""
    return paths.chat_dir(chat_id) / "agent.pid"


# -- home-page section --------------------------------------------------------


def list_chat_links() -> list[tuple[str, str]]:
    """``(chat_id, title)`` pairs for the persistent sidebar nav."""
    links: list[tuple[str, str]] = []
    for cid in paths.list_chat_ids():
        info = paths.chat_info_state(cid).read()
        title = info.get("title") or f"Chat {cid}"
        links.append((cid, str(title)))
    return links


def _tool_status_from_block(block: dict) -> str:
    if block.get("is_error") is True:
        return "err"
    if block.get("is_error") is False or block.get("output") not in (None, ""):
        return "ok"
    return "pending"


def chat_status_dot(chat_id: str) -> dict:
    """``{"phase": chatting|awaiting|idle|error, "tool": ok|err|pending|none}``."""
    state = paths.chat_state(chat_id).read()
    phase_raw = state.get("phase") or "idle"
    if phase_raw == "chatting":
        phase = "chatting"
    elif phase_raw == "error" or state.get("error"):
        phase = "error"
    elif state.get("pending_question") or state.get("waiting_on"):
        phase = "awaiting"
    else:
        # Any active sub-agent run counts as chatting for the outer dot.
        active_run = False
        awaiting_run = False
        for run_id in paths.list_run_ids(chat_id):
            rp = paths.run_state(chat_id, run_id).read().get("phase") or ""
            if rp in ("awaiting_user", "awaiting_answers"):
                awaiting_run = True
            elif rp not in ("done", "error", "stopped", ""):
                active_run = True
        if awaiting_run and not active_run and phase_raw != "chatting":
            phase = "awaiting"
        elif active_run:
            phase = "chatting"
        else:
            phase = "idle"

    tool = "none"
    exchanges = state.get("exchanges") or []
    if exchanges:
        turns = exchanges[-1].get("turns") or []
        for turn in reversed(turns):
            if turn.get("kind"):
                continue
            blocks = turn.get("tool_blocks") or []
            if blocks:
                tool = _tool_status_from_block(blocks[-1])
                break
    return {"phase": phase, "tool": tool}


def render_chat_dot(chat_id: str, status: dict | None = None) -> str:
    """Outer phase symbol + inner tool-colored dot for sidebar/home cards."""
    esc = _ui._esc
    status = status or chat_status_dot(chat_id)
    phase = status.get("phase") or "idle"
    tool = status.get("tool") or "none"
    return (
        f'<span class="chat-dot dot-{esc(phase)}" data-chat-id="{esc(chat_id)}" '
        f'title="{esc(phase)} / tool:{esc(tool)}">'
        f'<span class="chat-dot-inner tool-{esc(tool)}"></span></span>'
    )


def _render_chat_list(ids: list[str]) -> str:
    if not ids:
        return '<p class="muted">No chats yet.</p>'
    items = []
    for cid in ids:
        info = paths.chat_info_state(cid).read()
        title = info.get("title") or "(untitled chat)"
        created = info.get("created_at")
        when = _ui._format_time(created) if created else ""
        delete_btn = (
            f'<button class="contrast compact-btn" hx-delete="/api/chats/{_ui._esc(cid)}" '
            'hx-target="closest li" hx-swap="outerHTML" '
            'hx-confirm="Delete this chat permanently?">Delete</button>'
        )
        items.append(
            f'<li class="chat-list-item">'
            f'{render_chat_dot(cid)}'
            f'<a href="/chats/{_ui._esc(cid)}">{_ui._esc(title)}</a> '
            f'<small class="muted">({_ui._esc(cid)}{" · " + when if when else ""})</small>'
            f"{delete_btn}</li>"
        )
    return "<ul>" + "".join(items) + "</ul>"


def _plain_model_select(name: str, current: str) -> str:
    """A plain <select> of the model cache (no htmx save-on-change) for the
    new-chat form. The saved/default value is always kept as an option."""
    from crack_server import model_latency
    from crack_server import models as models_mod

    esc = _ui._esc
    options = models_mod.models_for_render()
    if current not in options:
        options = [current] + options
    avgs = model_latency.latencies()
    opt_bits: list[str] = []
    for m in options:
        selected = " selected" if m == current else ""
        label = esc(m)
        if m != current and m in avgs:
            label = f"{esc(m)}  ·  {avgs[m]:.1f}s"
        opt_bits.append(f'<option value="{esc(m)}"{selected}>{label}</option>')
    return f'<select name="{esc(name)}">{"".join(opt_bits)}</select>'


def render_new_chat_form() -> str:
    """New-chat button. Plan mode and the model choices are now picked *inside*
    the chat — editable until the first message is sent — so the home page just
    mints the chat with the global-settings defaults."""
    return """
      <form method="post" action="/api/chats" class="new-chat-form">
        <button type="submit">New Chat</button>
      </form>
    """


def render_chat_config_editor(info: dict) -> str:
    """Editable plan/model controls shown at the top of a brand-new chat, before
    its first message is sent (moved off the home page). Submits inside the send
    form; :func:`post_message` locks the choices onto the chat when the first
    message goes out."""
    plan_on = bool(info.get("plan", True))
    planner = _plain_model_select(
        "planner_model", info.get("planner_model") or _settings.plan_planner_model()
    )
    implementer = _plain_model_select(
        "implementer_model", info.get("implementer_model") or _settings.plan_implementer_model()
    )
    nonplan = _plain_model_select("model", info.get("model") or _settings.nonplan_model())
    return f"""
      <div class="chat-config" data-plan-form>
        <input type="hidden" name="config" value="1">
        <p class="chat-config-hint"><small class="muted">Pick the models before your
          first message — they lock once the chat starts.</small></p>
        <label class="new-chat-plan">
          <input type="checkbox" name="plan" value="1"{" checked" if plan_on else ""} data-plan-toggle>
          Plan mode (prewalk)
        </label>
        <div class="plan-fields" data-plan-fields{"" if plan_on else " hidden"}>
          <label>Planner model {planner}</label>
          <label>Implementer model {implementer}</label>
        </div>
        <div class="nonplan-fields" data-nonplan-fields{" hidden" if plan_on else ""}>
          <label>Model {nonplan}</label>
        </div>
      </div>
    """


def render_home_section() -> str:
    """Chats-only home body: New Chat + recent chats + links."""
    ids = paths.list_chat_ids()
    recent = _render_chat_list(ids[:RECENT_CHATS])
    rest = ""
    if len(ids) > RECENT_CHATS:
        rest = (
            f"<details><summary>All chats ({len(ids)})</summary>"
            f"{_render_chat_list(ids)}</details>"
        )
    return f"""
    <header>
      <h1>Crack</h1>
      <p class="muted">Unscripted chats and recursive sub-agents.</p>
    </header>
    <section id="unscripted-chats" class="section-spaced">
      <h2>Chats</h2>
      {render_new_chat_form()}
      {recent}
      {rest}
    </section>
    <p><a href="/sub_agents">Sub-agents</a> · <a href="/settings">Settings</a></p>
    """


def render_home_page() -> str:
    """Full HTML for ``GET /``."""
    return _ui._render_base("Crack", render_home_section())


# -- chat page ----------------------------------------------------------------


# A spawn tool's output text names the run it started ("Spawned coder run
# 1784616263980_de72c4dc.") — parse that so the sub-agent card renders inline
# right under the tool call, not pinned to the top of the chat.
_SPAWN_RUN_RE = re.compile(r"\brun\s+([0-9]+_[0-9a-f]+)")


def _spawn_run_ids(turn: dict) -> list[str]:
    """Run ids spawned by this turn's ``spawn_*`` tool calls, in call order."""
    ids: list[str] = []
    for block in turn.get("tool_blocks") or []:
        if not str(block.get("name", "")).startswith("spawn"):
            continue
        match = _SPAWN_RUN_RE.search(str(block.get("output", "")))
        if match:
            ids.append(match.group(1))
    return ids


def _append_inside_stage_msg(msg_html: str, extra: str) -> str:
    """Insert ``extra`` just before a stage-msg fragment's closing tag."""
    idx = msg_html.rfind("</div>")
    if idx == -1:
        return msg_html + extra
    return msg_html[:idx] + extra + msg_html[idx:]


def _chat_model_badge(info: dict) -> str:
    """Read-only summary of a chat's locked model choice(s)."""
    esc = _ui._esc
    if info.get("plan"):
        planner = esc(info.get("planner_model") or DEFAULT_CHAT_MODEL)
        impl = esc(info.get("implementer_model") or DEFAULT_CHAT_MODEL)
        return (
            f'<small class="muted">plan · planner <code>{planner}</code> '
            f'→ implementer <code>{impl}</code></small>'
        )
    return f'<small class="muted">model <code>{esc(info.get("model") or DEFAULT_CHAT_MODEL)}</code></small>'


def render_chat_form(chat_id: str, info: dict, state: dict | None = None) -> str:
    """Bottom form: multiline input + Send, plus the model control.

    Three shapes:

    - *before the first message* (no exchanges / nothing queued): the editable
      plan + model config (moved off the home page). The choices lock onto the
      chat when the first message is sent.
    - *a plan chat resolving its first message*: the pairing is a read-only badge.
    - *graduated / non-plan*: a model dropdown continues the chat on any model
      (default: the implementer model for a graduated plan chat, else the chat's
      non-plan model), with a muted note that plan mode is now locked."""
    safe_id = _ui._esc(chat_id)
    strip = attachments.render_strip(
        "chats", chat_id, paths.chat_attachments_state(chat_id), "chat-attachments"
    )
    plan = bool(info.get("plan"))
    graduated = bool(info.get("graduated"))
    if state is None:
        state = paths.chat_state(chat_id).read()
    pre_first = not (state.get("exchanges") or state.get("pending"))
    if pre_first:
        model_control = render_chat_config_editor(info)
    elif plan and not graduated:
        model_control = f'<div class="chat-model-badge">{_chat_model_badge(info)}</div>'
    else:
        default_model = (
            info.get("implementer_model") if plan else info.get("model")
        ) or info.get("model") or DEFAULT_CHAT_MODEL
        lock_note = (
            '<small class="muted chat-plan-locked">Plan mode is locked for this '
            "chat — start a new chat to plan again.</small>"
            if plan else ""
        )
        model_control = (
            '<label class="chat-model-pick">Model '
            f"{_plain_model_select('model', default_model)}</label>{lock_note}"
        )
    return f"""
    <form class="chat-form" hx-post="/api/chats/{safe_id}/messages"
          hx-target="#chat-content" hx-swap="outerHTML">
      {model_control}
      {strip}
      <label>Message
        <textarea name="msg" rows="4" required placeholder="Type a message…"></textarea>
      </label>
      <button type="submit">Send</button>
    </form>
    """


def render_user_question_form(chat_id: str, run_id: str, question: dict) -> str:
    """The ask_user Q&A form for a suspended run (run tree + run page)."""
    esc = _ui._esc
    choices = question.get("choices") or []
    if choices:
        field = "".join(
            f'<label class="choice-label">'
            f'<input type="radio" name="answer" value="{esc(c)}" required> {esc(c)}</label>'
            for c in choices
        )
    else:
        field = '<textarea name="answer" rows="3" required placeholder="Your answer…"></textarea>'
    return f"""
    <form class="ask-user-form" hx-post="/api/chats/{esc(chat_id)}/sub_agents/runs/{esc(run_id)}/user_answer"
          hx-target="closest .subagent-run-region" hx-swap="outerHTML">
      <p><strong>The agent asks:</strong> {esc(question.get("question", ""))}</p>
      {field}
      <button type="submit">Answer</button>
    </form>
    """


def render_chat_question_form(chat_id: str, question: dict) -> str:
    """Interactive ask_user form for a chat parent: radios for choices (else a
    textarea), submitted to the dedicated ask_answer endpoint."""
    esc = _ui._esc
    choices = question.get("choices") or []
    if choices:
        field = "".join(
            f'<label class="choice-label">'
            f'<input type="radio" name="answer" value="{esc(c)}" required> {esc(c)}</label>'
            for c in choices
        )
    else:
        field = '<textarea name="answer" rows="3" required placeholder="Your answer…"></textarea>'
    return f"""
    <form class="ask-user-form chat-ask" hx-post="/api/chats/{esc(chat_id)}/ask_answer"
          hx-target="#chat-content" hx-swap="outerHTML">
      <div class="aq-q"><strong>Clanker asks:</strong> {_ui._render_markdown(question.get("question", ""))}</div>
      {field}
      <button type="submit">Answer</button>
    </form>
    """


def render_answered_question(qa: dict) -> str:
    """Read-only mirror of an answered ask_user form (shown in the transcript
    exactly as the user submitted it)."""
    esc = _ui._esc
    question = str(qa.get("question", ""))
    choices = qa.get("choices") or []
    answer = qa.get("answer", "")
    if choices:
        field = "".join(
            f'<label class="choice-label">'
            f'<input type="radio" disabled{" checked" if c == answer else ""}> {esc(str(c))}</label>'
            for c in choices
        )
    else:
        field = f'<textarea rows="3" disabled>{esc(str(answer))}</textarea>'
    return (
        '<div class="stage-msg answered-question">'
        f'<p class="aq-q"><strong>Clanker asked:</strong> {esc(question)}</p>'
        f'<div class="aq-form">{field}</div>'
        "</div>"
    )


def _run_phase_class(phase: str) -> str:
    if phase in ("running", "resuming", "revising", "awaiting_answers", "writing"):
        return "running"
    if phase == "done":
        return "done"
    if phase == "awaiting_user":
        return "awaiting"
    if phase == "error":
        return "error"
    if phase == "stopped":
        return "stopped"
    return phase or "idle"


_RUN_TERMINAL = ("done", "error", "stopped")


def _run_label(state: dict) -> str:
    return str(state.get("title") or state.get("persona") or "?")


def _run_turns(state: dict) -> int:
    return int(state.get("hops_completed", 0) or 0)


def _run_alive_str(state: dict) -> str:
    start = state.get("created_at")
    if not start:
        return ""
    end = state.get("finished_at") if state.get("phase") in _RUN_TERMINAL else time.time()
    mins = max(0, int((float(end) - float(start)) // 60))
    running = state.get("phase") not in _RUN_TERMINAL
    verb = "running for" if running else "ran for"
    return f"{verb} {mins} min"


def _sidebar_run_order(
    roots: list[str], cmap: dict[str, list[str]]
) -> dict[str, int]:
    """``run_id ->`` 1-based spawn-order index (oldest first, DFS)."""
    order: dict[str, int] = {}
    idx = 0

    def dfs(run_id: str) -> None:
        nonlocal idx
        idx += 1
        order[run_id] = idx
        for child in cmap.get(run_id, []):
            dfs(child)

    for root in roots:
        dfs(root)
    return order


def _persona_model(persona_slug: str) -> str:
    """The model a persona currently runs on (for card/sidebar display)."""
    from crack_server.sub_agents import registry

    persona = registry.get(persona_slug)
    return persona.model_for() if persona is not None else ""


def _run_display_model(state: dict) -> str:
    """The model this run's *next* hop uses — planner while planning, implementer
    after the prewalk swap, or the single model in non-plan mode. Keeps the card
    and right-tree badges in sync with what actually runs (a persona's config
    default is only the fallback)."""
    from crack_server import prewalk

    model = prewalk.model_for_phase(state, state.get("turns") or [])
    if model and model != prewalk.DEFAULT_MODEL:
        return model
    # Non-plan run with no explicit model: fall back to the persona's config.
    return model or _persona_model(str(state.get("persona") or ""))


def _chat_display_model(info: dict, state: dict) -> str:
    """The chat root's current effective model, mirroring what its next hop
    would run on: planner→implementer while a plan chat is still resolving its
    first message, else the continuation default."""
    from crack_server import prewalk

    # Graduation caches the display model on info (see run_chat) — a direct read
    # that skips the model_for_phase list-copy on every 2s poll.
    cached = info.get("display_model")
    if cached:
        return cached
    plan = bool(info.get("plan"))
    if plan and not info.get("graduated"):
        exchanges = state.get("exchanges", [])
        turns = exchanges[-1].get("turns", []) if exchanges else []
        synth = {
            "plan": True,
            "planner_model": info.get("planner_model") or info.get("model"),
            "implementer_model": info.get("implementer_model") or info.get("model"),
            "model": info.get("model"),
        }
        return prewalk.model_for_phase(synth, turns) or DEFAULT_CHAT_MODEL
    default = info.get("implementer_model") if plan else info.get("model")
    return default or info.get("model") or DEFAULT_CHAT_MODEL


def _children_map(chat_id: str) -> dict[str, list[str]]:
    """``parent_run_id -> [child_run_id...]`` for run-parented runs (oldest first)."""
    run_ids = paths.list_run_ids(chat_id)
    cmap: dict[str, list[str]] = {}
    for run_id in run_ids:
        state = paths.run_state(chat_id, run_id).read()
        if state.get("parent_kind") == "run" and state.get("parent_id") in run_ids:
            cmap.setdefault(str(state["parent_id"]), []).append(run_id)
    for kids in cmap.values():
        kids.sort()
    return cmap


def _root_run_ids(chat_id: str) -> list[str]:
    """Runs parented directly by the chat (oldest first)."""
    run_ids = set(paths.list_run_ids(chat_id))
    roots: list[str] = []
    for run_id in run_ids:
        state = paths.run_state(chat_id, run_id).read()
        if not (state.get("parent_kind") == "run" and state.get("parent_id") in run_ids):
            roots.append(run_id)
    roots.sort()
    return roots


def root_run_id(chat_id: str, run_id: str) -> str:
    """Walk parent links up to the root run parented by the chat (the run whose
    inline region wraps ``run_id``). Falls back to ``run_id`` on any gap."""
    run_ids = set(paths.list_run_ids(chat_id))
    current = run_id
    seen: set[str] = set()
    while current in run_ids and current not in seen:
        seen.add(current)
        state = paths.run_state(chat_id, current).read()
        if state.get("parent_kind") == "run" and state.get("parent_id") in run_ids:
            current = str(state["parent_id"])
        else:
            break
    return current


def _subtree_active(chat_id: str, run_id: str, cmap: dict[str, list[str]]) -> bool:
    """True when this run or any descendant is not in a terminal phase."""
    state = paths.run_state(chat_id, run_id).read()
    if state.get("phase") not in _RUN_TERMINAL:
        return True
    return any(_subtree_active(chat_id, c, cmap) for c in cmap.get(run_id, []))


def _render_run_card(chat_id: str, run_id: str, children_by_parent: dict[str, list[str]]) -> str:
    """One bordered sub-agent card: collapsible header + full transcript +
    context meter + nested children. Open while active, collapsed when done."""
    esc = _ui._esc
    render = _render()
    state = paths.run_state(chat_id, run_id).read()
    phase = state.get("phase") or "?"
    persona = state.get("persona", "?")
    label = _run_label(state)
    hop_count = _run_turns(state)
    alive = _run_alive_str(state)
    depth = state.get("depth", "?")
    safe_run = esc(run_id)
    phase_cls = _run_phase_class(str(phase))
    model = _run_display_model(state)

    actions = ""
    if phase not in _RUN_TERMINAL:
        actions += (
            f'<button class="contrast compact-btn" '
            f'hx-post="/api/chats/{esc(chat_id)}/sub_agents/runs/{safe_run}/stop" '
            f'hx-target="closest .subagent-run-region" hx-swap="outerHTML">Stop</button>'
        )
    if phase in ("error", "stopped"):
        actions += (
            f'<button class="secondary compact-btn" '
            f'hx-post="/api/chats/{esc(chat_id)}/sub_agents/runs/{safe_run}/retry" '
            f'hx-target="closest .subagent-run-region" hx-swap="outerHTML">Retry</button>'
        )
    error = ""
    if phase == "error" and state.get("error"):
        error = f'<p class="error"><small>{esc(str(state["error"]))}</small></p>'

    form_html = ""
    if phase == "awaiting_user" and state.get("pending_question"):
        form_html = render_user_question_form(chat_id, run_id, state["pending_question"])

    turns = state.get("turns") or []
    errors = state.get("errors") or []
    transcript = "".join(render.render_turn_msgs(
        turns, errors=errors, include_text=True, model_state=render.new_model_state()
    ))
    if not transcript:
        transcript = '<p class="muted"><small>No turns yet.</small></p>'

    ctx_line = context_stats.render_context_line(
        paths.run_sessions_dir(chat_id, run_id), model
    )

    child_html = "".join(
        _render_run_card(chat_id, child_id, children_by_parent)
        for child_id in children_by_parent.get(run_id, [])
    )
    status_dot = f'<span class="run-status-dot phase-{esc(phase_cls)}" aria-hidden="true"></span>'
    model_badge = f'<code class="run-model">{esc(model)}</code>' if model else ""
    open_attr = " open" if phase not in _RUN_TERMINAL else ""
    metrics = f" · {hop_count} turns"
    if alive:
        metrics += f" · {esc(alive)}"
    header = (
        f'<summary class="subagent-card-header">'
        f"{status_dot}"
        f'<strong title="{esc(str(persona))}">{esc(label)}</strong> '
        f'<small class="muted">depth {esc(str(depth))} · <code>{esc(phase)}</code>'
        f"{metrics}</small> "
        f"{model_badge}"
        f'<a class="run-link" href="/sub_agents/runs/{safe_run}">{safe_run}</a>'
        f"</summary>"
    )
    body = (
        f'<div class="subagent-body">'
        f'<div class="subagent-actions">{actions}</div>'
        f"{error}{form_html}"
        f'<div class="subagent-transcript">{transcript}</div>'
        f"{ctx_line}"
        f"{child_html}</div>"
    )
    return (
        f'<div class="subagent-card phase-{esc(phase_cls)}" data-run-id="{safe_run}">'
        f'<details class="subagent-details" id="subagent-body-{safe_run}"{open_attr}>'
        f"{header}{body}</details></div>"
    )


def render_inline_run_region(chat_id: str, run_id: str) -> str:
    """A root sub-agent card as a self-polling region (embedded inline under the
    spawn tool call). Polls its own fragment every 2s while the subtree is active."""
    esc = _ui._esc
    cmap = _children_map(chat_id)
    card = _render_run_card(chat_id, run_id, cmap)
    poll = ""
    if _subtree_active(chat_id, run_id, cmap):
        poll = (
            f' hx-get="/chats/{esc(chat_id)}/run/{esc(run_id)}" '
            f'hx-trigger="every 2s" hx-swap="outerHTML"'
        )
    return (
        f'<div id="subagent-run-{esc(run_id)}" class="subagent-run-region"{poll}>'
        f"{card}</div>"
    )


def _render_sidebar_node(
    chat_id: str, run_id: str, cmap: dict[str, list[str]], order: dict[str, int]
) -> str:
    """Compact control-tree node for one run (right sidebar)."""
    esc = _ui._esc
    state = paths.run_state(chat_id, run_id).read()
    phase = str(state.get("phase") or "?")
    persona = str(state.get("persona") or "?")
    label = _run_label(state)
    hop_count = _run_turns(state)
    alive = _run_alive_str(state)
    run_idx = order.get(run_id, 0)
    phase_cls = _run_phase_class(phase)
    model = _run_display_model(state)
    dot = f'<span class="run-status-dot phase-{esc(phase_cls)}" aria-hidden="true"></span>'
    stop = ""
    if phase not in _RUN_TERMINAL:
        # swap=none: the action fires; both the sidebar tree and the inline
        # region self-poll (every 2s) to reflect the new phase.
        stop = (
            f'<button class="contrast compact-btn tree-stop" '
            f'hx-post="/api/chats/{esc(chat_id)}/sub_agents/runs/{esc(run_id)}/stop" '
            f'hx-swap="none">Stop</button>'
        )
    model_badge = f'<small class="muted">{esc(model)}</small>' if model else ""
    meta_bits = [f"#{run_idx}", f"{hop_count} turns"]
    if alive:
        meta_bits.append(alive)
    meta_line = " · ".join(meta_bits)
    kids = "".join(
        _render_sidebar_node(chat_id, child, cmap, order)
        for child in cmap.get(run_id, [])
    )
    return (
        f'<li class="tree-node phase-{esc(phase_cls)}">'
        f'<div class="tree-row">{dot}'
        f'<a href="#subagent-run-{esc(run_id)}" class="tree-label" '
        f'title="{esc(persona)}">{esc(label)}</a>'
        f'<small class="muted tree-phase">{esc(phase)}</small>{stop}</div>'
        f'<div class="tree-meta"><small class="muted">{esc(meta_line)}</small> '
        f"{model_badge}</div>"
        f'{f"<ul>{kids}</ul>" if kids else ""}'
        f"</li>"
    )


def render_sidebar_tree(chat_id: str) -> str:
    """Right-rail control tree: root = the chat (Stop = kill everything), children
    = sub-agent runs recursively (persona, phase, model, per-run Stop)."""
    esc = _ui._esc
    info = paths.chat_info_state(chat_id).read()
    title = info.get("title") or f"Chat {chat_id}"
    state = paths.chat_state(chat_id).read()
    chat_phase = str(state.get("phase") or "idle")
    cmap = _children_map(chat_id)
    roots = _root_run_ids(chat_id)

    chat_running = chat_phase == "chatting"
    any_run_active = any(_subtree_active(chat_id, rid, cmap) for rid in roots)
    chat_pending = bool(state.get("pending") or state.get("child_inbox"))
    active = chat_running or any_run_active or chat_pending

    root_dot_cls = (
        "running" if active and chat_phase != "error"
        else ("error" if chat_phase == "error" else "done")
    )
    chat_stop = (
        f'<button class="contrast compact-btn tree-stop" '
        f'hx-post="/api/chats/{esc(chat_id)}/stop" '
        f'hx-swap="none">Stop all</button>'
    )
    chat_model = _chat_display_model(info, state)
    run_order = _sidebar_run_order(roots, cmap)
    nodes = "".join(
        _render_sidebar_node(chat_id, rid, cmap, run_order) for rid in roots
    )
    tree = f"<ul>{nodes}</ul>" if nodes else '<p class="muted"><small>No sub-agents yet.</small></p>'
    # Always poll — perf budget in Plan 3 (threadpool routes; cheap run.json reads).
    poll_attrs = (
        f' hx-get="/chats/{esc(chat_id)}/sidebar-tree" hx-trigger="every 2s" '
        f'hx-swap="outerHTML"'
    )
    return (
        f'<div id="subagent-sidebar-tree" class="subagent-sidebar-tree"{poll_attrs}>'
        f"<h6>Agent tree</h6>"
        f'<div class="tree-root phase-{esc(root_dot_cls)}">'
        f'<div class="tree-row">'
        f'<span class="run-status-dot phase-{esc(root_dot_cls)}" aria-hidden="true"></span>'
        f'<span class="tree-label"><strong>{esc(title)}</strong></span>'
        f'<small class="muted tree-phase">{esc(chat_phase)}</small>{chat_stop}</div>'
        f'<div class="tree-meta"><small class="muted">{esc(chat_model)}</small></div>'
        f"</div>"
        f"{tree}</div>"
    )


def _tag_chat_msg(index: int, html: str, oob: bool = False) -> str:
    esc = _ui._esc
    msg_id = f"{CHAT_SLUG}-msg-{index}"
    # An out-of-band copy replaces the same-id element already in the DOM (used to
    # refresh a turn that was rendered mid-flight, so its tool dots settle once the
    # result lands rather than staying stuck "running" until a full reload).
    attrs = f'id="{esc(msg_id)}" ' + ('hx-swap-oob="true" ' if oob else "")
    for needle in (
        '<div class="stage-msg', "<div class='stage-msg",
        '<details class="stage-msg', "<details class='stage-msg",
    ):
        if needle in html[:120]:
            tag = needle.split(" ", 1)[0]
            return html.replace(tag + " ", f"{tag} {attrs}", 1)
    return f'<div {attrs}class="stage-msg">{html}</div>'


def _exchange_duration(exchange: dict | None) -> float | None:
    """Wall duration for an exchange from started_at/finished_at, else turn span."""
    if not exchange:
        return None
    started = exchange.get("started_at")
    finished = exchange.get("finished_at")
    try:
        if started is not None and finished is not None:
            return max(0.0, float(finished) - float(started))
    except (TypeError, ValueError):
        pass
    ats = []
    for t in exchange.get("turns") or []:
        at = t.get("at")
        if at is not None:
            try:
                ats.append(float(at))
            except (TypeError, ValueError):
                pass
    if len(ats) >= 2:
        return max(0.0, ats[-1] - ats[0])
    return None


def render_chat_msgs(chat_id: str) -> list[str]:
    """Render the chat trajectory from pi session ndjson (faithful projection).

    The session ndjson is the single source of truth for the message stream:
    user prompts included. Exchange sidecars only *enrich* what the trajectory
    already contains (ask_user Q&A, recorded errors, compiled-prompt / media
    metadata) — they never inject a message the session file lacks. That keeps
    the append-by-index poll stable: a just-sent prompt appears exactly once,
    when pi records it, rather than being echoed optimistically at the top and
    then again at its real position once annotations push it down.
    """
    from crack_server import trajectory_view

    render = _render()
    state = paths.chat_state(chat_id).read()
    model_state = render.new_model_state()
    sessions_dir = paths.chat_sessions_dir(chat_id)
    projected = trajectory_view.project_sessions_dir(
        sessions_dir,
        media_dir=paths.chat_dir(chat_id) / "media",
        media_url_prefix=f"/chats/{chat_id}/media",
        conv_id=chat_id,
    )
    exchanges = state.get("exchanges") or []
    rows = trajectory_view.merge_exchange_sidecars(projected, exchanges)
    out: list[str] = []
    # UI-only prep timings (sandbox / first byte / …) — not part of chat history.
    for entry in state.get("ui_prep") or []:
        out.append(render.render_prep_timing_row(entry))
    known_runs = set(paths.list_run_ids(chat_id))
    for row in rows:
        kind = row.get("kind")
        if kind == "turn" or not kind:
            msgs = render.render_turn_msgs([row], include_text=True, model_state=model_state)
            run_ids = [rid for rid in _spawn_run_ids(row) if rid in known_runs]
            if run_ids and msgs:
                cards = "".join(render_inline_run_region(chat_id, rid) for rid in run_ids)
                msgs[-1] = _append_inside_stage_msg(msgs[-1], cards)
            out.extend(msgs)
        else:
            out.extend(render.render_turn_msgs([row], include_text=True, model_state=model_state))

    # Errored chats have no terminal_reason row (phase resets to idle); show a
    # red runtime line just above the #chat-msgs border.
    last_ex = exchanges[-1] if exchanges else None
    stop_reason = (last_ex or {}).get("stop_reason")
    if state.get("error") or stop_reason == "empty":
        out.append(render.render_error_stop_row(_exchange_duration(last_ex)))
    return out


def render_chat_tail(
    chat_id: str, *, gate_error_html: str | None = None
) -> str:
    render = _render()
    info = paths.chat_info_state(chat_id).read()
    state = paths.chat_state(chat_id).read()
    phase = state.get("phase")
    parts: list[str] = []

    if gate_error_html:
        parts.append(gate_error_html)

    pending_n = len(state.get("pending") or [])
    if pending_n:
        parts.append(
            f'<p class="chat-pending"><small>{pending_n} message(s) queued…</small></p>'
        )

    pending_question = state.get("pending_question") or {}
    if pending_question.get("question"):
        parts.append(render_chat_question_form(chat_id, pending_question))

    if phase != "chatting" and state.get("error"):
        parts.append(render.render_fatal_error_banner(state))
        parts.append(render.render_error_msg(state.get("error", ""), state.get("error_detail", "")))

    if phase == "chatting":
        safe_id = _ui._esc(chat_id)
        parts.append(
            '<div class="stage-running">'
            f"{render.render_spinner('Thinking…')}"
            f'<button class="contrast" hx-post="/api/chats/{safe_id}/stop" '
            'hx-target="#chat-content" hx-swap="outerHTML">Stop</button></div>'
        )

    # Context meter pinned to the bottom of the chat frame (pi-CLI status line).
    ctx_line = context_stats.render_context_line(
        paths.chat_sessions_dir(chat_id), info.get("model") or DEFAULT_CHAT_MODEL
    )
    if ctx_line:
        parts.append(ctx_line)

    parts.append(render_chat_form(chat_id, info, state))
    return "".join(parts)


def wrap_chat_content(chat_id: str, msgs: list[str], tail: str, after: int | None = None) -> str:
    esc = _ui._esc
    tagged = [_tag_chat_msg(i, m) for i, m in enumerate(msgs)]
    msg_count = len(tagged)
    mtime = chat_state_mtime(chat_id)
    state = paths.chat_state(chat_id).read()
    phase = state.get("phase") or "idle"
    exchanges = state.get("exchanges") or []
    last_ex = exchanges[-1] if exchanges else {}
    stop_reason = last_ex.get("stop_reason")
    if phase == "chatting":
        status = "running"
    elif state.get("error") or stop_reason == "empty":
        status = "error"
    elif stop_reason == "stopped":
        status = "stopped"
    elif stop_reason in ("agent_end", "sentinel"):
        status = "done"
    else:
        status = "idle"

    if after is not None:
        # Re-send the boundary message (the last one the client already has) as an
        # out-of-band swap: a turn rendered while its tools were still running is
        # frozen at its index and the append-only delta (i > after) never revisits
        # it, so its dots would stay blue until a full reload. The OOB copy replaces
        # it in place; strictly-new messages (i > after) are appended as before.
        boundary = (
            _tag_chat_msg(after, msgs[after], oob=True)
            if 0 <= after < msg_count
            else ""
        )
        new_msgs = "".join(tagged[i] for i in range(len(tagged)) if i > after)
        return (
            boundary
            + new_msgs
            + f'<div id="chat-tail" hx-swap-oob="outerHTML">{tail}</div>'
            + '<span id="chat-status-meta" hx-swap-oob="outerHTML"'
            + f' data-stage-status="{esc(status)}" data-msg-count="{msg_count}"'
            + f' data-state-mtime="{mtime}" hidden></span>'
        )

    return (
        f'<div id="chat-content" class="stage-content chat-content"'
        f' data-chat-id="{esc(chat_id)}" data-stage-status="{esc(status)}"'
        f' data-msg-count="{msg_count}" data-state-mtime="{mtime}"'
        f' data-stage-slug="{CHAT_SLUG}">'
        f'<div id="chat-msgs">{"".join(tagged)}</div>'
        f'<div id="chat-tail">{tail}</div>'
        f'<span id="chat-status-meta" hidden'
        f' data-stage-status="{esc(status)}" data-msg-count="{msg_count}"'
        f' data-state-mtime="{mtime}"></span>'
        f"</div>"
    )


def render_chat_content(
    chat_id: str, after: int | None = None, *, gate_error_html: str | None = None
) -> str:
    """Chat exchanges + status + form (msgs/tail; supports ``?after=`` deltas)."""
    return wrap_chat_content(
        chat_id,
        render_chat_msgs(chat_id),
        render_chat_tail(chat_id, gate_error_html=gate_error_html),
        after=after,
    )


def render_chat_page_body(chat_id: str) -> str:
    info = paths.chat_info_state(chat_id).read()
    title = info.get("title") or f"Chat {chat_id}"
    return f"""
    <header>
      <p><a href="/">← Home</a> · <a href="/sub_agents">Sub-agents</a></p>
      <h1>{_ui._esc(title)}</h1>
      <p><small class="muted">id {_ui._esc(chat_id)} · all tools enabled</small></p>
    </header>
    {render_chat_content(chat_id)}
    """


# -- route handlers (registered in app.py) -------------------------------------


def create_chat(
    plan: bool = True,
    planner_model: str = "",
    implementer_model: str = "",
    model: str = "",
) -> RedirectResponse:
    """POST /api/chats: create a prewalk-coder chat with its locked model
    choices, then redirect into its page."""
    chat_id = paths.generate_chat_id()
    paths.create_chat(
        chat_id,
        model or _settings.nonplan_model(),
        plan=plan,
        planner_model=planner_model or _settings.plan_planner_model(),
        implementer_model=implementer_model or _settings.plan_implementer_model(),
    )
    logger.info("chats: created %s (plan=%s)", chat_id, plan)
    return RedirectResponse(url=f"/chats/{chat_id}", status_code=303)


def post_message(
    chat_id: str,
    msg: str,
    model: str | None,
    *,
    plan: bool | None = None,
    planner_model: str = "",
    implementer_model: str = "",
) -> HTMLResponse:
    """POST /api/chats/{id}/messages: queue the agent for a new user message.

    Always appends to ``pending`` and enqueues; human messages and child-report
    resumes serialize via the exclusive chat job (no B2 refuse-while-chatting).

    Before the first message the send form carries the editable plan + model
    config (``plan``/``planner_model``/``implementer_model``/``model``); those
    lock onto the chat's ``info`` here. Plan mode governs only the first
    message's resolution; once the chat has graduated (see :func:`run_chat`) a
    human message runs unrestrained on the ``model`` picked in the continuation
    dropdown (recorded on the exchange).
    """
    check_chat_id(chat_id)
    msg = msg.strip()
    st = paths.chat_state(chat_id).read()
    pre_first = not (st.get("exchanges") or st.get("pending"))
    if pre_first and plan is not None:
        # Lock the in-chat model/plan choices onto the chat before it starts.
        # This runs *before* the clean-git gate so that a refused (dirty-tree)
        # send still persists the user's plan/model selection — otherwise the
        # gate re-render falls back to config defaults and silently flips the
        # plan checkbox back on.
        def _lock_config(info: dict) -> dict:
            info["plan"] = bool(plan)
            if planner_model:
                info["planner_model"] = planner_model
            if implementer_model:
                info["implementer_model"] = implementer_model
            if model:
                info["model"] = model
            return info

        paths.chat_info_state(chat_id).update(_lock_config)
        # The nonplan model is now stored on the chat; don't also stamp it as a
        # per-exchange continuation switch.
        model = None
    # Hard clean-git gate: refuse the first message of a top-level sandboxed
    # chat until the host worktree is clean (prerequisite for frozen bases).
    if (
        pre_first
        and sandbox.sandbox_enabled()
        and git_utils.host_worktree_dirty()
    ):
        status_raw = git_utils.host_status_colored(limit=10)
        status_html = git_utils.ansi_to_html(status_raw)
        gate = (
            '<div class="chat-git-gate error" role="alert">'
            "<p><strong>Host worktree is dirty</strong> — clean it (commit, "
            "stash, or discard) before starting a sandboxed chat.</p>"
            f'<pre class="chat-git-status">{status_html}</pre>'
            "</div>"
        )
        return HTMLResponse(render_chat_content(chat_id, gate_error_html=gate))
    # One-shot attachments staged via paste/drop: weave them into this message,
    # then clear the manifest so they aren't resent on the next message. The
    # uploaded files stay on disk under attachments/ for history. A media list
    # rides along on the exchange so the sent-message bubble can render the
    # thumbnails (the woven prompt text itself stays text-only).
    staged = attachments.list_attachments(paths.chat_attachments_state(chat_id))
    media: list[dict] = []
    if staged:
        block = attachments.format_block(staged)
        msg = (block + "\n\n" + msg) if msg else block
        media = [
            {
                "url": f"/chats/{chat_id}/attachments/{e.get('id', '')}",
                "src": str(e.get("saved_path", "")),
                "description": str(e.get("description", "")),
            }
            for e in staged
        ]
        attachments.clear(paths.chat_attachments_state(chat_id))
    if msg:
        def _begin(state: dict) -> dict:
            item: dict = {"user": msg, "source": "human"}
            if model:
                item["model"] = model
            if media:
                item["media"] = media
            state.setdefault("pending", []).append(item)
            state["phase"] = "chatting"
            state["stop_requested"] = False
            # A human message answers any outstanding ask_user question.
            state.pop("pending_question", None)
            state.pop("error", None)
            state.pop("error_detail", None)
            # Fresh prep strip for the new exchange.
            state["ui_prep"] = []
            return state

        paths.chat_state(chat_id).update(_begin)
        queue.enqueue_exclusive(chat_id, CHAT_JOB_SLUG, "chat")
    return HTMLResponse(render_chat_content(chat_id))


def answer_chat_question(chat_id: str, answer: str) -> HTMLResponse:
    """POST /api/chats/{id}/ask_answer: the human's answer to a chat ask_user
    question. Records the Q&A (for the read-only log mirror) and resumes the
    agent with the answer as a fresh exchange."""
    check_chat_id(chat_id)
    answer = (answer or "").strip()
    if not answer:
        raise HTTPException(status_code=400, detail="answer is required")
    chat = paths.chat_state(chat_id)
    pending_q = chat.read().get("pending_question") or {}
    if not pending_q.get("question"):
        raise HTTPException(status_code=409, detail="no pending question")
    question = str(pending_q.get("question", ""))
    choices = list(pending_q.get("choices") or [])
    qa = {"question": question, "choices": choices, "answer": answer, "at": time.time()}
    message = f"You asked: {question}\n\nThe user answered: {answer}"

    def _record(state: dict) -> dict:
        state.setdefault("pending", []).append({
            "user": message,
            "source": "ask_answer",
            "qa": qa,
        })
        state["phase"] = "chatting"
        state["stop_requested"] = False
        state.pop("pending_question", None)
        state.pop("error", None)
        state.pop("error_detail", None)
        return state

    chat.update(_record)
    queue.enqueue_exclusive(chat_id, CHAT_JOB_SLUG, "chat")
    return HTMLResponse(render_chat_content(chat_id))


def _stop_all_runs(chat_id: str, *, cascade_finish: bool = False) -> None:
    """Stop every sub-agent run under this chat (kill pid, phase stopped)."""
    from crack_server.sub_agents import registry

    for run_id in paths.list_run_ids(chat_id):
        state = paths.run_state(chat_id, run_id).read()
        persona = registry.get(state.get("persona", ""))
        if persona is None:
            continue
        # Cascade skips parent resume — chat-wide stop should not re-enqueue drains.
        persona.request_stop(run_id, cascade=True)


def stop_chat(chat_id: str) -> HTMLResponse:
    """POST /api/chats/{id}/stop: halt the chat agent and all sub-agent runs."""
    check_chat_id(chat_id)
    chat = paths.chat_state(chat_id)

    def _flag_stop(state: dict) -> dict:
        state["stop_requested"] = True
        return state

    chat.update(_flag_stop)
    killed = pi_runner.kill_pid_file(_agent_pid_file(chat_id))
    logger.info("chats: stop requested for %s (killed=%s)", chat_id, killed)
    _stop_all_runs(chat_id)

    def _halt(state: dict) -> dict:
        if state.get("phase") == "chatting":
            state["phase"] = "idle"
        state["pending"] = []
        # Stamp a terminal reason on the open exchange immediately so the Stop
        # response (and the next poll) show "Stopped by user" without waiting
        # for the worker to finish tearing down the hop.
        exs = state.get("exchanges") or []
        if exs and not exs[-1].get("stop_reason"):
            exs[-1]["stop_reason"] = "stopped"
        return state

    chat.update(_halt)
    return HTMLResponse(render_chat_content(chat_id))


def delete_chat(chat_id: str) -> HTMLResponse:
    """DELETE /api/chats/{id}: kill agents (incl. sub-runs), then remove the dir."""
    check_chat_id(chat_id)
    pi_runner.kill_pid_file(_agent_pid_file(chat_id))
    if sandbox.sandbox_enabled():
        sandbox.destroy_sandbox_sync(chat_id)
    _stop_all_runs(chat_id)
    try:
        shutil.rmtree(paths.chat_dir(chat_id))
    except OSError as e:
        logger.warning("chats: failed to delete %s: %s", chat_id, e)
    logger.info("chats: deleted %s", chat_id)
    return HTMLResponse("")


# -- worker entry point ---------------------------------------------------------


async def _maybe_generate_title(chat_id: str, first_message: str) -> None:
    """Summarize the first user message into a short chat title via the nano
    title model (the same one used for task-prompt titles). Best-effort: a
    failure leaves the title empty ("(untitled chat)") rather than breaking the
    chat run."""
    info = paths.chat_info_state(chat_id).read()
    if info.get("title"):
        return
    message = (first_message or "").strip()
    if not message:
        return
    try:
        title = await titles.agenerate_title(message, log_prefix="chat-title")
        if title:
            def _set_title(info: dict) -> dict:
                info["title"] = title
                return info

            paths.chat_info_state(chat_id).update(_set_title)
            logger.info("chats: titled %s -> %r", chat_id, title)
    except Exception:
        logger.exception("chats: title generation failed for %s", chat_id)


def _merge_child_inbox(chat_id: str) -> int:
    """Move chat.json child_inbox entries into pending as child_report messages."""
    from crack_server.sub_agents import runner

    entries: list[dict] = []

    def _take(state: dict) -> dict:
        entries.extend(state.get("child_inbox") or [])
        state["child_inbox"] = []
        return state

    paths.chat_state(chat_id).update(_take)
    if not entries:
        return 0

    def _enqueue(state: dict) -> dict:
        pending = list(state.get("pending") or [])
        for entry in entries:
            pending.append({
                "user": (
                    "Your spawned sub-agent(s) have reported back:\n\n"
                    + runner.format_child_result(entry)
                ),
                "source": "child_report",
                "run_id": entry.get("run_id"),
            })
        state["pending"] = pending
        state["phase"] = "chatting"
        return state

    paths.chat_state(chat_id).update(_enqueue)
    return len(entries)


def _has_active_runs(chat_id: str) -> bool:
    """True while any sub-agent run under this chat is non-terminal. Used to hold
    the chat sandbox open so children's finish-time patches still have a parent
    overlay to apply into."""
    for run_id in paths.list_run_ids(chat_id):
        if paths.run_state(chat_id, run_id).read().get("phase") not in _RUN_TERMINAL:
            return True
    return False


def _pop_pending(chat_id: str) -> dict | None:
    """Pop the next pending message, or None if the queue is empty / stop flagged."""
    taken: dict | None = None

    def _pop(state: dict) -> dict:
        nonlocal taken
        if state.get("stop_requested"):
            state["pending"] = []
            return state
        pending = list(state.get("pending") or [])
        if not pending:
            return state
        taken = pending.pop(0)
        state["pending"] = pending
        state.setdefault("exchanges", []).append({
            "user": taken.get("user", ""),
            "turns": [],
            "source": taken.get("source", "human"),
            **({"run_id": taken["run_id"]} if taken.get("run_id") else {}),
            **({"media": taken["media"]} if taken.get("media") else {}),
            **({"qa": taken["qa"]} if taken.get("qa") else {}),
            **({"model": taken["model"]} if taken.get("model") else {}),
        })
        state["phase"] = "chatting"
        return state

    paths.chat_state(chat_id).update(_pop)
    return taken


def _clear_ui_prep(chat_id: str) -> None:
    """Drop previous prep timings so a new exchange starts a fresh debug strip."""

    def _clear(state: dict) -> dict:
        state["ui_prep"] = []
        return state

    paths.chat_state(chat_id).update(_clear)


def _append_ui_prep(chat_id: str, stage_id: str, label: str, elapsed: float) -> None:
    """Append one completed prep stage (UI-only; bumps chat.json mtime for poll)."""

    def _append(state: dict) -> dict:
        rows = list(state.get("ui_prep") or [])
        # Replace an existing stage with the same id (idempotent re-entry).
        rows = [r for r in rows if r.get("id") != stage_id]
        rows.append({
            "id": stage_id,
            "label": label,
            "elapsed": round(float(elapsed), 3),
            "at": time.time(),
        })
        state["ui_prep"] = rows
        return state

    paths.chat_state(chat_id).update(_append)


async def run_chat(chat_id: str) -> None:
    """Worker side of a CHAT_JOB_SLUG job: drain child reports, then process
    pending exchanges FIFO until the queue is empty."""
    chat = paths.chat_state(chat_id)
    sandbox_name: str | None = None
    if sandbox.sandbox_enabled():
        t0 = time.monotonic()
        sandbox_name = await sandbox.ensure_sandbox(chat_id)
        _append_ui_prep(
            chat_id, "sandbox",
            "sandbox ready (frozen git tree + overlay)",
            time.monotonic() - t0,
        )
        await patch.capture_baseline(sandbox_name, paths.chat_dir(chat_id))

    def stop_check() -> bool:
        return bool(chat.read().get("stop_requested"))

    while True:
        _merge_child_inbox(chat_id)
        item = _pop_pending(chat_id)
        if item is None:
            def _idle(state: dict) -> dict:
                if state.get("pending") or state.get("child_inbox"):
                    return state
                state["phase"] = "idle"
                return state

            chat.update(_idle)
            _merge_child_inbox(chat_id)
            if chat.read().get("pending"):
                continue
            if sandbox.sandbox_enabled() and _has_active_runs(chat_id):
                # Sub-agents still running (the chat ended its turn without
                # wait_join): keep the chat sandbox alive so their finish-time
                # patches can still apply into it. drain_children re-enqueues this
                # job when they finish, and we finalize on a later idle pass.
                return
            if sandbox.sandbox_enabled() and sandbox_name:
                if await patch.finalize_chat_sandbox(chat_id, sandbox_name):
                    t0 = time.monotonic()
                    sandbox_name = await sandbox.ensure_sandbox(chat_id)
                    _append_ui_prep(
                        chat_id, "sandbox",
                        "sandbox ready (frozen git tree + overlay)",
                        time.monotonic() - t0,
                    )
                    await patch.capture_baseline(sandbox_name, paths.chat_dir(chat_id))
                    continue
            elif sandbox.sandbox_enabled():
                await sandbox.destroy_sandbox(chat_id)
            return

        # Fresh prep strip per exchange (sandbox timings from above stay if this
        # is the first hop of the job; otherwise we only add pi_first_byte).
        if not (chat.read().get("ui_prep") or []):
            pass  # sandbox timings already recorded at job start
        else:
            # Keep sandbox/baseline lines; drop a previous hop's first-byte line.
            def _trim(state: dict) -> dict:
                state["ui_prep"] = [
                    r for r in (state.get("ui_prep") or [])
                    if r.get("id") != "pi_first_byte"
                ]
                return state

            chat.update(_trim)

        info = paths.chat_info_state(chat_id).read()
        planner_model = info.get("planner_model") or info.get("model") or DEFAULT_CHAT_MODEL
        implementer_model = info.get("implementer_model") or info.get("model") or DEFAULT_CHAT_MODEL
        plan_active = bool(info.get("plan")) and not bool(info.get("graduated"))
        exchanges = chat.read().get("exchanges", [])
        is_first = len(exchanges) == 1
        cur_exchange = exchanges[-1] if exchanges else {}
        if plan_active:
            model = planner_model
        else:
            # Non-plan: locked chat model wins. ``implementer_model`` is only a
            # fallback after a plan chat has graduated (it defaults to composer
            # and must not shadow the locked non-plan model on exchange 0).
            model = (
                cur_exchange.get("model")
                or (info.get("implementer_model") if info.get("graduated") else None)
                or info.get("model")
                or DEFAULT_CHAT_MODEL
            )

        def _on_first_byte(elapsed: float) -> None:
            _append_ui_prep(
                chat_id, "pi_first_byte",
                "pi spawned · first byte",
                elapsed,
            )

        await chat_engine.run_exchange(
            state=chat,
            ident=chat_id,
            message_builder=lambda user_msg: user_msg,
            record_template="",
            log_prefix="unscripted-chat",
            model=model,
            plan=plan_active,
            planner_model=planner_model,
            implementer_model=implementer_model,
            max_hops=MAX_CHAT_HOPS,
            session_id=f"unscripted-{chat_id}",
            sessions_dir=paths.chat_sessions_dir(chat_id),
            tools=None,
            timeout_seconds=CHAT_TIMEOUT_SECONDS,
            hop_kwargs={
                "pid_file": _agent_pid_file(chat_id),
                "stop_check": stop_check,
                "waiting_check": lambda: bool(chat.read().get("waiting_on")),
                "sandbox": sandbox_name,
                "on_first_byte": _on_first_byte,
            },
            pre_stop_check=stop_check,
            on_first_exchange=(
                (lambda user_msg: _maybe_generate_title(chat_id, user_msg))
                if is_first and item.get("source") == "human"
                else None
            ),
            env_extra={
                "CRACK_SUBAGENT_CTX": "1",
                "CRACK_SUBAGENT_DEPTH": "0",
                "CRACK_CHAT_ID": chat_id,
                "CRACK_PARENT_KIND": "chat",
                "CRACK_PARENT_ID": chat_id,
            },
            media_dir=paths.chat_dir(chat_id) / "media",
            media_url_prefix=f"/chats/{chat_id}/media",
        )

        # If the agent finished while children are still running, surface that as
        # an implicit wait_join rather than a plain "agent finished" row.
        def _maybe_waiting(state: dict) -> dict:
            exs = state.get("exchanges") or []
            if not exs:
                return state
            last = exs[-1]
            if (
                last.get("stop_reason") in ("agent_end", "sentinel")
                and _has_active_runs(chat_id)
            ):
                last["stop_reason"] = "waiting_children"
            return state

        chat.update(_maybe_waiting)

        if plan_active and not chat.read().get("pending_question"):
            paths.chat_info_state(chat_id).update(
                lambda s: {**s, "graduated": True, "display_model": implementer_model}
            )
        if stop_check():
            if sandbox.sandbox_enabled() and sandbox_name:
                await patch.finalize_chat_sandbox(chat_id, sandbox_name, forceful=True)
            def _halt(state: dict) -> dict:
                state["phase"] = "idle"
                state["pending"] = []
                return state

            chat.update(_halt)
            return

