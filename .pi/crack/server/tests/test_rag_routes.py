"""RAG debug page routes (/rag, /rag/search)."""

from __future__ import annotations

from unittest.mock import AsyncMock

import pytest
from fastapi.responses import HTMLResponse

from crack_server import rag
from crack_server.routes_rag import rag_page, rag_search


@pytest.mark.anyio
async def test_rag_search_empty_query_returns_blank_partial():
    resp = await rag_search("")
    assert isinstance(resp, HTMLResponse)
    assert resp.body == b""


@pytest.mark.anyio
async def test_rag_search_short_query_returns_blank_partial(monkeypatch):
    monkeypatch.setattr(rag, "search_docs", AsyncMock())
    resp = await rag_search("a")
    assert resp.body == b""
    rag.search_docs.assert_not_called()


@pytest.mark.anyio
async def test_rag_search_renders_hit_rows(monkeypatch):
    monkeypatch.setattr(
        rag,
        "search_docs",
        AsyncMock(
            return_value=[
                {
                    "score": 0.042,
                    "source": "file:///workspace/foo.py",
                    "snippet": "def sandbox_overlay(): ...",
                }
            ]
        ),
    )
    resp = await rag_search("sandbox overlay")
    body = resp.body.decode()
    assert "0.042" in body
    assert "file:///workspace/foo.py" in body
    assert "sandbox_overlay" in body
    assert "1 hit(s)" in body


@pytest.mark.anyio
async def test_rag_search_no_hits_message(monkeypatch):
    monkeypatch.setattr(rag, "search_docs", AsyncMock(return_value=[]))
    resp = await rag_search("nothing here")
    assert "No hits." in resp.body.decode()


def test_rag_page_includes_debounce_attributes():
    resp = rag_page()
    body = resp.body.decode()
    assert "delay:300ms" in body
    assert 'hx-get="/rag/search"' in body
    assert "#rag-results" in body
