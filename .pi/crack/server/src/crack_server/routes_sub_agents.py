"""Sub-agent HTTP API + control/run pages."""

from __future__ import annotations

import asyncio
import time

from fastapi import APIRouter, Form, HTTPException, Request
from fastapi.responses import HTMLResponse, JSONResponse

from crack_server import attachments, chats, paths, ui as _ui
from crack_server.render import model_select, new_model_state, render_turn_msgs
from crack_server.sub_agents import MAX_DEPTH, ask_user, registry, signals, wait
from crack_server.sub_agents import runner

router = APIRouter()

# Server-side cap for a single long-poll block: the extension re-issues polls
# in a loop, so this only bounds how long one HTTP request hangs.
MAX_BLOCK_SECONDS = 25.0


def _persona_or_404(slug: str):
    persona = registry.get(slug)
    if persona is None:
        raise HTTPException(status_code=404, detail=f"unknown persona: {slug}")
    return persona


def _run_or_404(run_id: str) -> dict:
    try:
        state = paths.run_state_by_id(run_id).read()
    except (ValueError, FileNotFoundError) as e:
        raise HTTPException(status_code=404, detail="run not found") from e
    if not state:
        raise HTTPException(status_code=404, detail="run not found")
    return state


def _run_public(state: dict) -> dict:
    return {
        "run_id": state.get("run_id"),
        "persona": state.get("persona"),
        "chat_id": state.get("chat_id"),
        "parent_kind": state.get("parent_kind"),
        "parent_id": state.get("parent_id"),
        "depth": state.get("depth"),
        "phase": state.get("phase"),
        "report_path": state.get("report_path"),
        "error": state.get("error") or "",
        "nudge_count": state.get("nudge_count", 0),
        "created_at": state.get("created_at"),
        "finished_at": state.get("finished_at"),
    }


# ---------------------------------------------------------------------------
# JSON API for the pi extension + UI
# ---------------------------------------------------------------------------


@router.get("/api/sub_agents")
def api_list_sub_agents() -> list[dict]:
    """Persona list for the crack_subagents pi extension."""
    out = []
    for persona in registry.list_personas():
        out.append({
            "slug": persona.slug,
            "name": persona.name,
            "tool_name": persona.tool_name(),
            "tool_description": persona.tool_description(),
            "tool_label": persona.tool_label(),
            "model": persona.model_for(),
        })
    return out


@router.post("/api/chats/{chat_id}/sub_agents/spawn")
async def api_spawn_sub_agent(chat_id: str, request: Request) -> JSONResponse:
    """Mint a run and enqueue it; returns immediately."""
    chats.check_chat_id(chat_id)
    try:
        body = await request.json()
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"invalid JSON body: {e}") from e
    if not isinstance(body, dict):
        raise HTTPException(status_code=400, detail="JSON object required")

    persona = str(body.get("persona") or "").strip()
    instructions = str(body.get("instructions") or "").strip()
    parent_kind = str(body.get("parent_kind") or "").strip()
    parent_id = str(body.get("parent_id") or "").strip()
    try:
        depth = int(body.get("depth", 0))
    except (TypeError, ValueError) as e:
        raise HTTPException(status_code=400, detail="depth must be an int") from e

    if not persona or not instructions:
        raise HTTPException(status_code=400, detail="persona and instructions are required")
    if parent_kind not in ("chat", "run") or not parent_id:
        raise HTTPException(status_code=400, detail="parent_kind and parent_id are required")
    plan = bool(body.get("plan", True))

    try:
        state = runner.spawn(
            chat_id=chat_id,
            persona_slug=persona,
            instructions=instructions,
            parent_kind=parent_kind,
            parent_id=parent_id,
            depth=depth,
            plan=plan,
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e)) from e

    return JSONResponse({
        "run_id": state["run_id"],
        "report_path": state["report_path"],
        "status": state.get("phase", "running"),
    })


@router.post("/api/chats/{chat_id}/sub_agents/wait")
async def api_wait_sub_agents(chat_id: str, request: Request) -> JSONResponse:
    """Long-poll for child results (the server side of the wait_join tool).

    Body: ``{parent_kind, parent_id, target?, run_ids?, rebuild?,
    block_seconds?}``. Runs one wait.poll(); while targets stay unresolved and
    ``block_seconds`` (capped at MAX_BLOCK_SECONDS) remains, stamps
    ``waiting_on`` into the parent state (orphan sweep skips it; the hop's
    watchdog credits the wait out of its timeout) and suspends on the parent's
    signal event — ``runner.finish()`` sets it after its inbox write, so a
    child landing wakes the poll immediately.
    """
    chats.check_chat_id(chat_id)
    try:
        body = await request.json()
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"invalid JSON body: {e}") from e
    if not isinstance(body, dict):
        raise HTTPException(status_code=400, detail="JSON object required")

    parent_kind = str(body.get("parent_kind") or "").strip()
    parent_id = str(body.get("parent_id") or "").strip()
    if parent_kind not in ("chat", "run") or not parent_id:
        raise HTTPException(status_code=400, detail="parent_kind and parent_id are required")
    if parent_kind == "run":
        _run_or_404(parent_id)
    target = body.get("target")
    run_ids = body.get("run_ids")
    rebuild = body.get("rebuild")
    try:
        block_seconds = min(float(body.get("block_seconds") or 0.0), MAX_BLOCK_SECONDS)
    except (TypeError, ValueError):
        block_seconds = 0.0

    def _poll() -> dict:
        return wait.poll(
            chat_id=chat_id,
            parent_kind=parent_kind,
            parent_id=parent_id,
            target=target,
            run_ids=run_ids,
            rebuild=rebuild,
        )

    result = _poll()
    if not result["pending"] or block_seconds <= 0:
        return JSONResponse(result)

    deadline = time.monotonic() + block_seconds
    event = signals.event_for(parent_kind, parent_id)
    try:
        while result["pending"]:
            wait.stamp_waiting(chat_id, parent_kind, parent_id, result["pending"])
            event.clear()
            # Re-poll after the clear so a finish() landing between the first
            # poll and the wait is not missed.
            result = _poll()
            if not result["pending"]:
                break
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            try:
                await asyncio.wait_for(event.wait(), timeout=remaining)
            except asyncio.TimeoutError:
                pass
            result = _poll()
    finally:
        wait.clear_waiting(chat_id, parent_kind, parent_id)
    return JSONResponse(result)


@router.post("/api/chats/{chat_id}/ask_user")
async def api_ask_user(chat_id: str, request: Request) -> JSONResponse:
    """Record a question for the human (the server side of the ask_user tool).

    Run parents suspend in ``awaiting_user`` (the hop ends cleanly; the answer
    arrives as a fresh resume hop). Chat parents just record the question —
    the chat's normal input is the answer channel.
    """
    chats.check_chat_id(chat_id)
    try:
        body = await request.json()
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"invalid JSON body: {e}") from e
    if not isinstance(body, dict):
        raise HTTPException(status_code=400, detail="JSON object required")

    parent_kind = str(body.get("parent_kind") or "").strip()
    parent_id = str(body.get("parent_id") or "").strip()
    question = str(body.get("question") or "").strip()
    choices = body.get("choices")
    if parent_kind not in ("chat", "run") or not parent_id:
        raise HTTPException(status_code=400, detail="parent_kind and parent_id are required")
    if not question:
        raise HTTPException(status_code=400, detail="question is required")
    if choices is not None and not (
        isinstance(choices, list) and all(isinstance(c, str) for c in choices)
    ):
        raise HTTPException(status_code=400, detail="choices must be a list of strings")

    try:
        status = ask_user.ask(
            chat_id=chat_id,
            parent_kind=parent_kind,
            parent_id=parent_id,
            question=question,
            choices=choices,
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e)) from e
    return JSONResponse({"status": status})


@router.post("/api/chats/{chat_id}/sub_agents/runs/{run_id}/user_answer")
async def api_run_user_answer(
    chat_id: str, run_id: str, request: Request, answer: str = Form(...)
) -> HTMLResponse:
    """The human's answer to an ask_user question: resume the run with it."""
    chats.check_chat_id(chat_id)
    answer = answer.strip()
    if not answer:
        raise HTTPException(status_code=400, detail="answer is required")
    if not ask_user.answer(chat_id, run_id, answer):
        raise HTTPException(status_code=409, detail="run is not awaiting an answer")
    if request.headers.get("hx-request"):
        return HTMLResponse(chats.render_inline_run_region(chat_id, chats.root_run_id(chat_id, run_id)))
    from fastapi.responses import RedirectResponse

    return RedirectResponse(url=f"/sub_agents/runs/{run_id}", status_code=303)


@router.get("/api/chats/{chat_id}/sub_agents/runs")
def api_list_runs(chat_id: str) -> dict:
    chats.check_chat_id(chat_id)
    runs = [_run_public(paths.run_state(chat_id, rid).read()) for rid in paths.list_run_ids(chat_id)]
    return {"runs": runs}


@router.get("/api/chats/{chat_id}/sub_agents/runs/{run_id}")
def api_get_run(chat_id: str, run_id: str) -> dict:
    chats.check_chat_id(chat_id)
    state = paths.run_state(chat_id, run_id).read()
    if not state:
        raise HTTPException(status_code=404, detail="run not found")
    return _run_public(state)


@router.get("/chats/{chat_id}/sub_agents/runs/{run_id}/media/{filename}")
def run_media(chat_id: str, run_id: str, filename: str):
    """Serve a persisted image copy from the run's media/ dir."""
    chats.check_chat_id(chat_id)
    try:
        directory = paths.run_media_dir(chat_id, run_id)
    except ValueError as e:
        raise HTTPException(status_code=404, detail="not found") from e
    return attachments.serve_file(directory, filename)


@router.post("/api/chats/{chat_id}/sub_agents/runs/{run_id}/stop", response_class=HTMLResponse)
def api_run_stop(chat_id: str, run_id: str) -> HTMLResponse:
    chats.check_chat_id(chat_id)
    state = paths.run_state(chat_id, run_id).read()
    if not state:
        raise HTTPException(status_code=404, detail="run not found")
    persona = _persona_or_404(state.get("persona", ""))
    persona.request_stop(run_id, cascade=False)
    return HTMLResponse(chats.render_inline_run_region(chat_id, chats.root_run_id(chat_id, run_id)))


@router.post("/api/chats/{chat_id}/sub_agents/runs/{run_id}/retry", response_class=HTMLResponse)
def api_run_retry(chat_id: str, run_id: str) -> HTMLResponse:
    chats.check_chat_id(chat_id)
    state = paths.run_state(chat_id, run_id).read()
    if not state:
        raise HTTPException(status_code=404, detail="run not found")
    persona = _persona_or_404(state.get("persona", ""))
    persona.retry(run_id)
    return HTMLResponse(chats.render_inline_run_region(chat_id, chats.root_run_id(chat_id, run_id)))


@router.post("/api/sub_agents/{slug}/model", response_class=HTMLResponse)
def api_set_persona_model(slug: str, model: str = Form(...)) -> HTMLResponse:
    persona = _persona_or_404(slug)
    persona.set_model(model)
    return HTMLResponse(_render_persona_row(persona))


@router.put("/api/sub_agents/{slug}/templates/{filename}", response_class=HTMLResponse)
def api_put_persona_template(
    slug: str, filename: str, content: str = Form(...)
) -> HTMLResponse:
    persona = _persona_or_404(slug)
    # Allow any basename under the persona dir that is a simple .md name.
    from pathlib import Path as P

    base = P(filename).name
    if not base.endswith(".md") or "/" in filename or "\\" in filename:
        raise HTTPException(status_code=400, detail="invalid template filename")
    path = persona.persona_dir() / base
    if not path.is_file() and base not in persona.templates:
        raise HTTPException(status_code=404, detail="unknown template")
    path.write_text(content, encoding="utf-8")
    return HTMLResponse(_render_persona_template_row(persona, base))


# ---------------------------------------------------------------------------
# HTML pages / fragments
# ---------------------------------------------------------------------------


def _render_persona_template_row(persona, filename: str, editing: bool = False) -> str:
    path = persona.persona_dir() / filename
    content = path.read_text(encoding="utf-8") if path.is_file() else ""
    try:
        meta = f"{path.stat().st_size} bytes • {_ui._format_time(path.stat().st_mtime)}"
    except OSError:
        meta = ""
    return _ui.render_file_row(
        f"/sub_agents/{persona.slug}/template-row/{filename}",
        f"/api/sub_agents/{persona.slug}/templates/{filename}",
        filename,
        content,
        meta,
        editing,
        indent=" " * 8,
    )


def _render_persona_row(persona) -> str:
    esc = _ui._esc
    select = model_select(
        "model",
        persona.model_for(),
        f"/api/sub_agents/{persona.slug}/model",
        target="closest .persona-row",
        swap="outerHTML",
        indent=" " * 10,
    )
    return f"""
    <div class="persona-row part-row">
      <span class="part-label">{esc(persona.name)}</span>
      <code>{esc(persona.slug)}</code>
      <small>tool <code>{esc(persona.tool_name())}</code></small>
{select}
    </div>
    """


@router.get("/sub_agents", response_class=HTMLResponse)
def sub_agents_page() -> HTMLResponse:
    esc = _ui._esc
    sections = []
    for persona in registry.list_personas():
        templates = "".join(
            _render_persona_template_row(persona, name)
            for name in persona.templates
        )
        sections.append(f"""
        <section>
          <h2>{esc(persona.name)}</h2>
          <p class="muted">{esc(persona.tool_description())}</p>
          {_render_persona_row(persona)}
          <h3>Templates</h3>
          {templates}
        </section>
        """)
    body = f"""
    <header>
      <h1>Sub-agents</h1>
      <p class="muted">Max depth {MAX_DEPTH}. Personas live in <code>.pi/crack/sub_agents/</code>.</p>
      <p><a href="/">← Home</a></p>
    </header>
    {"".join(sections)}
    """
    return HTMLResponse(_ui._render_base("Sub-agents", body))


@router.get("/sub_agents/{slug}/template-row/{filename}", response_class=HTMLResponse)
def persona_template_row(
    slug: str, filename: str, editing: bool = False
) -> HTMLResponse:
    persona = _persona_or_404(slug)
    return HTMLResponse(_render_persona_template_row(persona, filename, editing=editing))


@router.get("/sub_agents/runs/{run_id}", response_class=HTMLResponse)
def run_page(run_id: str) -> HTMLResponse:
    state = _run_or_404(run_id)
    esc = _ui._esc
    turns = state.get("turns") or []
    msgs = "".join(render_turn_msgs(
        turns, errors=state.get("errors", []), model_state=new_model_state()
    ))
    question_html = ""
    if state.get("phase") == "awaiting_user" and state.get("pending_question"):
        q = state["pending_question"]
        choices = q.get("choices") or []
        if choices:
            field = "".join(
                f'<label class="choice-label">'
                f'<input type="radio" name="answer" value="{esc(c)}" required> {esc(c)}</label>'
                for c in choices
            )
        else:
            field = '<textarea name="answer" rows="3" required placeholder="Your answer…"></textarea>'
        question_html = f"""
        <section class="ask-user-box">
          <h2>Question for you</h2>
          <form method="post" action="/api/chats/{esc(state.get('chat_id', ''))}/sub_agents/runs/{esc(run_id)}/user_answer">
            <p><strong>{esc(q.get('question', ''))}</strong></p>
            {field}
            <button type="submit">Answer</button>
          </form>
        </section>
        """
    report = ""
    report_path = state.get("report_path") or ""
    if report_path:
        from pathlib import Path

        p = Path(report_path)
        if p.is_file():
            try:
                report = _ui._render_markdown(p.read_text(encoding="utf-8"))
            except OSError:
                report = ""
    body = f"""
    <header>
      <p><a href="/chats/{esc(state.get('chat_id', ''))}">← Chat</a>
         · <a href="/sub_agents">Sub-agents</a></p>
      <h1>Run {esc(run_id)}</h1>
      <p><small class="muted">
        persona <code>{esc(state.get('persona', ''))}</code> ·
        phase <code>{esc(state.get('phase', ''))}</code> ·
        depth {esc(str(state.get('depth', '')))}
      </small></p>
    </header>
    {question_html}
    <section><h2>Trajectory</h2>{msgs or '<p class="muted">No turns yet.</p>'}</section>
    <section><h2>Report</h2>{report or '<p class="muted">No report.md yet.</p>'}</section>
    """
    return HTMLResponse(_ui._render_base(f"Run {run_id}", body))
