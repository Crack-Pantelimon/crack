"""Unscripted chats: free-form pi chat sessions outside the task pipeline.

Chats live under ``.pi/crack/unscripted_chats/<chat_id>/`` (``info.json``,
``chat.json``, ``sessions/``) with a ms-epoch id. The web process only writes
state and enqueues ``CHAT_JOB_SLUG`` jobs; the worker runs the agent here via
``run_chat`` with *all* pi tools enabled (``tools=None``), resuming the chat's
own pi session across messages.
"""

from __future__ import annotations

import logging
import shutil

from fastapi import HTTPException
from fastapi.responses import HTMLResponse, RedirectResponse

from crack_server import ui as _ui
from crack_server import chat_engine
from crack_server import paths, pi_runner, queue, titles
from crack_server.state import chat_state_mtime

logger = logging.getLogger("uvicorn.error")

# Pseudo-stage slug for the non-stage chat job on the queue (see worker.py).
CHAT_JOB_SLUG = "__chat__"

DEFAULT_CHAT_MODEL = "nvidia/z-ai/glm-5.2"

CHAT_TIMEOUT_SECONDS = 1800

RECENT_CHATS = 5

# Pseudo-slug used for msg/tail ids (mirrors Stage.slug).
CHAT_SLUG = "chat"


def _render():
    """Lazy import to avoid app ↔ chats ↔ stages circular imports at load time."""
    from crack_server.stages import render as render_mod

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


def _render_chat_list(ids: list[str]) -> str:
    if not ids:
        return '<p style="color: #888;">No chats yet.</p>'
    items = []
    for cid in ids:
        info = paths.chat_info_state(cid).read()
        title = info.get("title") or "(untitled chat)"
        created = info.get("created_at")
        when = _ui._format_time(created) if created else ""
        delete_btn = (
            f'<button hx-delete="/api/chats/{_ui._esc(cid)}" '
            'hx-target="closest li" hx-swap="outerHTML" '
            'hx-confirm="Delete this chat permanently?" '
            'style="background:#c0392b;border-color:#c0392b;color:#fff;'
            'padding:0.1rem 0.5rem;font-size:0.8rem;margin-left:0.5rem;">Delete</button>'
        )
        items.append(
            f'<li style="display:flex;align-items:center;gap:0.25rem;">'
            f'<a href="/chats/{_ui._esc(cid)}">{_ui._esc(title)}</a> '
            f'<small style="color: #666;">({_ui._esc(cid)}{" · " + when if when else ""})</small>'
            f"{delete_btn}</li>"
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


def render_chat_answer(turns: list[dict]) -> list[str]:
    """One exchange's agent output as a list of msg fragments."""
    render = _render()
    parts: list[str] = []
    agent_turns = [t for t in turns if t.get("kind") != "user_prompt"]
    parts.extend(render.render_turn_msgs(agent_turns, include_text=False))
    answer = "\n\n".join(
        cleaned
        for turn in agent_turns
        if (cleaned := render._clean_turn_text(turn.get("text", "")))
    )
    if answer:
        parts.append(
            '<div class="stage-msg chat-assistant"><strong>Assistant:</strong>'
            f"{_ui._render_markdown(answer)}</div>"
        )
    return parts


def render_chat_form(chat_id: str, info: dict) -> str:
    """Bottom form: cached-model dropdown (saves on change) + multiline input + Send."""
    current = info.get("model") or DEFAULT_CHAT_MODEL
    safe_id = _ui._esc(chat_id)
    select = _render().model_select(
        "model", current, f"/api/chats/{chat_id}/model", swap="none", indent=" " * 8
    )
    return f"""
    <form class="chat-form" hx-post="/api/chats/{safe_id}/messages"
          hx-target="#chat-content" hx-swap="outerHTML">
      <label>Model
{select}
      </label>
      <label>Message
        <textarea name="msg" rows="4" required placeholder="Type a message…"></textarea>
      </label>
      <button type="submit">Send</button>
    </form>
    """


def render_run_tree(chat_id: str) -> str:
    """Live fragment listing sub-agent runs for this chat (statuses, planner forms)."""
    from crack_server.stages.qa import render_questions_form

    esc = _ui._esc
    run_ids = paths.list_run_ids(chat_id)
    if not run_ids:
        return (
            f'<div id="subagent-run-tree" class="subagent-run-tree" '
            f'data-chat-id="{esc(chat_id)}"></div>'
        )

    rows: list[str] = []
    active = False
    for run_id in sorted(run_ids, reverse=True):
        state = paths.run_state(chat_id, run_id).read()
        phase = state.get("phase") or "?"
        if phase not in ("done", "error", "stopped"):
            active = True
        persona = state.get("persona", "?")
        depth = state.get("depth", "?")
        safe_run = esc(run_id)
        link = f'<a href="/sub_agents/runs/{safe_run}">{safe_run}</a>'
        actions = ""
        if phase not in ("done", "error", "stopped"):
            actions += (
                f' <button hx-post="/api/chats/{esc(chat_id)}/sub_agents/runs/{safe_run}/stop" '
                f'hx-target="#subagent-run-tree" hx-swap="outerHTML" '
                f'style="padding:0.1rem 0.4rem;font-size:0.8rem;">Stop</button>'
            )
        if phase in ("error", "stopped"):
            actions += (
                f' <button hx-post="/api/chats/{esc(chat_id)}/sub_agents/runs/{safe_run}/retry" '
                f'hx-target="#subagent-run-tree" hx-swap="outerHTML" '
                f'style="padding:0.1rem 0.4rem;font-size:0.8rem;">Retry</button>'
            )
        error = ""
        if phase == "error" and state.get("error"):
            error = f'<br><small class="error">{esc(str(state["error"]))}</small>'
        form_html = ""
        if phase == "awaiting_answers" and state.get("pending_questions"):
            form_html = render_questions_form(
                f"/api/chats/{chat_id}/sub_agents/runs/{run_id}/answers",
                "#subagent-run-tree",
                int(state.get("round", 1)),
                None,
                state["pending_questions"],
                meta=f"Planner round {state.get('round', 1)} — answer to continue:",
            )
            form_html += (
                f'<form hx-post="/api/chats/{esc(chat_id)}/sub_agents/runs/{safe_run}/continue" '
                f'hx-target="#subagent-run-tree" hx-swap="outerHTML" style="margin-top:0.5rem;">'
                f'<button type="submit">Continue to plan (skip more questions)</button></form>'
            )
        rows.append(
            f'<li class="subagent-run phase-{esc(phase)}">'
            f'<strong>{esc(persona)}</strong> depth {esc(str(depth))} · '
            f'<code>{esc(phase)}</code> · {link}{actions}{error}{form_html}</li>'
        )

    poll_attrs = ""
    if active:
        poll_attrs = (
            f' hx-get="/chats/{esc(chat_id)}/run-tree" hx-trigger="every 2s" '
            f'hx-swap="outerHTML"'
        )
    return (
        f'<div id="subagent-run-tree" class="subagent-run-tree" '
        f'data-chat-id="{esc(chat_id)}"{poll_attrs}>'
        f'<h3>Sub-agent runs</h3><ul>{"".join(rows)}</ul></div>'
    )


def _tag_chat_msg(index: int, html: str) -> str:
    esc = _ui._esc
    msg_id = f"{CHAT_SLUG}-msg-{index}"
    for needle in (
        '<div class="stage-msg', "<div class='stage-msg",
        '<details class="stage-msg', "<details class='stage-msg",
    ):
        if needle in html[:120]:
            tag = needle.split(" ", 1)[0]
            return html.replace(tag + " ", f'{tag} id="{esc(msg_id)}" ', 1)
    return f'<div id="{esc(msg_id)}" class="stage-msg">{html}</div>'


def render_chat_msgs(chat_id: str) -> list[str]:
    render = _render()
    state = paths.chat_state(chat_id).read()
    return render.render_exchanges(state.get("exchanges", []), render_chat_answer)


def render_chat_tail(chat_id: str) -> str:
    render = _render()
    info = paths.chat_info_state(chat_id).read()
    state = paths.chat_state(chat_id).read()
    phase = state.get("phase")
    parts: list[str] = []

    pending_n = len(state.get("pending") or [])
    if pending_n:
        parts.append(
            f'<p class="chat-pending"><small>{pending_n} message(s) queued…</small></p>'
        )

    if phase != "chatting" and state.get("error"):
        parts.append(render.render_error_msg(state.get("error", ""), state.get("error_detail", "")))

    if phase == "chatting":
        safe_id = _ui._esc(chat_id)
        parts.append(
            '<div class="stage-running">'
            f"{render.render_spinner('Thinking…')}"
            f'<button class="chat-stop stop-btn" hx-post="/api/chats/{safe_id}/stop" '
            'hx-target="#chat-content" hx-swap="outerHTML">Stop</button></div>'
        )

    parts.append(render_chat_form(chat_id, info))
    return "".join(parts)


def wrap_chat_content(chat_id: str, msgs: list[str], tail: str, after: int | None = None) -> str:
    esc = _ui._esc
    tagged = [_tag_chat_msg(i, m) for i, m in enumerate(msgs)]
    msg_count = len(tagged)
    mtime = chat_state_mtime(chat_id)
    phase = paths.chat_state(chat_id).read().get("phase") or "idle"
    status = "running" if phase == "chatting" else ("error" if phase == "error" else "idle")

    if after is not None:
        new_msgs = "".join(tagged[i] for i in range(len(tagged)) if i > after)
        return (
            new_msgs
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


def render_chat_content(chat_id: str, after: int | None = None) -> str:
    """Chat exchanges + status + form (msgs/tail; supports ``?after=`` deltas)."""
    return wrap_chat_content(
        chat_id,
        render_chat_msgs(chat_id),
        render_chat_tail(chat_id),
        after=after,
    )


def render_chat_page_body(chat_id: str) -> str:
    info = paths.chat_info_state(chat_id).read()
    title = info.get("title") or f"Chat {chat_id}"
    return f"""
    <header style="margin-bottom: 1rem;">
      <p><a href="/">← Home</a> · <a href="/sub_agents">Sub-agents</a></p>
      <h1>{_ui._esc(title)}</h1>
      <p><small style="color: #666;">id {_ui._esc(chat_id)} · all tools enabled</small></p>
    </header>
    {render_run_tree(chat_id)}
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
    """POST /api/chats/{id}/messages: queue the agent for a new user message.

    Always appends to ``pending`` and enqueues; human messages and child-report
    resumes serialize via the exclusive chat job (no B2 refuse-while-chatting).
    """
    check_chat_id(chat_id)
    if model:
        def _set_model(info: dict) -> dict:
            info["model"] = model
            return info

        paths.chat_info_state(chat_id).update(_set_model)
    msg = msg.strip()
    if msg:
        def _begin(state: dict) -> dict:
            state.setdefault("pending", []).append({"user": msg, "source": "human"})
            state["phase"] = "chatting"
            state["stop_requested"] = False
            state.pop("error", None)
            state.pop("error_detail", None)
            return state

        paths.chat_state(chat_id).update(_begin)
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
        return state

    chat.update(_halt)
    return HTMLResponse(render_chat_content(chat_id))


def delete_chat(chat_id: str) -> HTMLResponse:
    """DELETE /api/chats/{id}: kill agents (incl. sub-runs), then remove the dir."""
    check_chat_id(chat_id)
    pi_runner.kill_pid_file(_agent_pid_file(chat_id))
    _stop_all_runs(chat_id)
    try:
        shutil.rmtree(paths.chat_dir(chat_id))
    except OSError as e:
        logger.warning("chats: failed to delete %s: %s", chat_id, e)
    logger.info("chats: deleted %s", chat_id)
    return HTMLResponse("")


def set_model(chat_id: str, model: str) -> HTMLResponse:
    """POST /api/chats/{id}/model: persist the dropdown selection."""
    check_chat_id(chat_id)

    def _set(info: dict) -> dict:
        info["model"] = model
        return info

    paths.chat_info_state(chat_id).update(_set)
    return HTMLResponse("")


# -- worker entry point ---------------------------------------------------------


def _maybe_generate_title(chat_id: str, first_message: str) -> None:
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
        title = titles.generate_title(message, log_prefix="chat-title")
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
        state["stop_requested"] = False
        return state

    paths.chat_state(chat_id).update(_enqueue)
    return len(entries)


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
        })
        state["phase"] = "chatting"
        return state

    paths.chat_state(chat_id).update(_pop)
    return taken


def run_chat(chat_id: str) -> None:
    """Worker side of a CHAT_JOB_SLUG job: drain child reports, then process
    pending exchanges FIFO until the queue is empty."""
    chat = paths.chat_state(chat_id)

    def stop_check() -> bool:
        return bool(chat.read().get("stop_requested"))

    while True:
        _merge_child_inbox(chat_id)
        item = _pop_pending(chat_id)
        if item is None:
            def _idle(state: dict) -> dict:
                # Only idle if nothing new arrived while we were checking.
                if state.get("pending") or state.get("child_inbox"):
                    return state
                state["phase"] = "idle"
                return state

            chat.update(_idle)
            # Re-check once: a finish() may have raced the idle write.
            _merge_child_inbox(chat_id)
            if chat.read().get("pending"):
                continue
            return

        model = paths.chat_info_state(chat_id).read().get("model") or DEFAULT_CHAT_MODEL
        is_first = len(chat.read().get("exchanges", [])) == 1
        chat_engine.run_exchange(
            state=chat,
            ident=chat_id,
            message_builder=lambda user_msg: user_msg,
            record_template="",
            log_prefix="unscripted-chat",
            model=model,
            session_id=f"unscripted-{chat_id}",
            sessions_dir=paths.chat_sessions_dir(chat_id),
            tools=None,
            timeout_seconds=CHAT_TIMEOUT_SECONDS,
            hop_kwargs={"pid_file": _agent_pid_file(chat_id), "stop_check": stop_check},
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
        )
        if stop_check():
            def _halt(state: dict) -> dict:
                state["phase"] = "idle"
                state["pending"] = []
                return state

            chat.update(_halt)
            return

