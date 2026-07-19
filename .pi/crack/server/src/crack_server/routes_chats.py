"""Unscripted-chat routes (logic in chats.py; worker dispatch via
chats.CHAT_JOB_SLUG)."""

from __future__ import annotations

import asyncio
import time

from fastapi import APIRouter, Form, Query, Response
from fastapi.responses import HTMLResponse, JSONResponse

from crack_server import chats, paths
from crack_server.state import chat_state_mtime
from crack_server.ui import _esc, _render_base

router = APIRouter()


@router.post("/api/chats")
def api_create_chat() -> Response:
    """Create a new unscripted chat and redirect (303) into its chat page."""
    return chats.create_chat()


@router.get("/chats/{chat_id}", response_class=HTMLResponse)
def chat_page(chat_id: str) -> HTMLResponse:
    chats.check_chat_id(chat_id)
    info = paths.chat_info_state(chat_id).read()
    title = info.get("title") or f"Chat {chat_id}"
    return HTMLResponse(_render_base(f"Crack Chat: {_esc(title)}", chats.render_chat_page_body(chat_id)))


@router.get("/chats/{chat_id}/status", response_class=HTMLResponse)
def chat_status(
    chat_id: str,
    after: int | None = Query(default=None),
) -> HTMLResponse:
    """Status fragment (full or ``?after=`` delta) for the chat long-poll watch."""
    chats.check_chat_id(chat_id)
    return HTMLResponse(chats.render_chat_content(chat_id, after=after))


@router.get("/chats/{chat_id}/wait")
async def chat_wait(
    chat_id: str,
    since: float = Query(default=0.0),
) -> JSONResponse:
    """Long-poll until the chat's state file mtime advances (up to 25s)."""
    chats.check_chat_id(chat_id)
    deadline = time.monotonic() + 25.0
    while True:
        mtime = chat_state_mtime(chat_id)
        if mtime > since:
            return JSONResponse({"since": mtime, "changed": True})
        if time.monotonic() >= deadline:
            return JSONResponse({"since": since, "changed": False})
        await asyncio.sleep(0.3)


@router.post("/api/chats/{chat_id}/messages", response_class=HTMLResponse)
def api_chat_message(
    chat_id: str,
    msg: str = Form(default=""),
    model: str = Form(default=""),
) -> HTMLResponse:
    """Append a user message, enqueue the agent, return the updated chat fragment."""
    return chats.post_message(chat_id, msg, model or None)


@router.post("/api/chats/{chat_id}/model", response_class=HTMLResponse)
def api_chat_model(chat_id: str, model: str = Form(...)) -> HTMLResponse:
    """Persist the chat's model selection (dropdown saves on change)."""
    return chats.set_model(chat_id, model)


@router.post("/api/chats/{chat_id}/stop", response_class=HTMLResponse)
def api_chat_stop(chat_id: str) -> HTMLResponse:
    """Stop the running agent for this chat and return the updated fragment."""
    return chats.stop_chat(chat_id)


@router.delete("/api/chats/{chat_id}", response_class=HTMLResponse)
def api_chat_delete(chat_id: str) -> HTMLResponse:
    """Delete a chat directory; empty fragment removes its home-page list item."""
    return chats.delete_chat(chat_id)
