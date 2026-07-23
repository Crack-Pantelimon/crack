"""First-hop RAG injection tests."""

from __future__ import annotations

from unittest.mock import AsyncMock

import pytest

from crack_server import rag_inject
from crack_server.settings import _RAG_DEFAULTS, rag_config


@pytest.fixture
def rag_cfg(tmp_path, monkeypatch):
    harness = tmp_path / "harness"
    harness.mkdir()
    monkeypatch.setattr("crack_server.settings.harness_dir", lambda: harness)
    return harness / "rag.json"


def _hit(score: float, snippet: str = "chunk text") -> dict:
    return {"score": score, "source": "file:///dep/lib.rs", "snippet": snippet}


@pytest.mark.anyio
async def test_below_threshold_unchanged(monkeypatch, rag_cfg):
    rag_cfg.write_text('{"first_hop_min_score": 0.5}', encoding="utf-8")
    monkeypatch.setattr(
        rag_inject.rag, "search_docs", AsyncMock(return_value=[_hit(0.03)])
    )
    msg = "fix the sandbox"
    assert await rag_inject.maybe_prepend_first_hop(query=msg, message=msg) == msg


@pytest.mark.anyio
async def test_above_threshold_prepends_context(monkeypatch, rag_cfg):
    rag_cfg.write_text('{"first_hop_min_score": 0.02}', encoding="utf-8")
    monkeypatch.setattr(
        rag_inject.rag,
        "search_docs",
        AsyncMock(return_value=[_hit(0.04, "overlay mount logic")]),
    )
    msg = "fix the sandbox"
    out = await rag_inject.maybe_prepend_first_hop(query=msg, message=msg)
    assert out.startswith("<rag-context>")
    assert "overlay mount logic" in out
    assert out.endswith(msg)


@pytest.mark.anyio
async def test_disabled_via_config(monkeypatch, rag_cfg):
    rag_cfg.write_text('{"first_hop_enabled": false}', encoding="utf-8")
    search = AsyncMock(return_value=[_hit(0.9)])
    monkeypatch.setattr(rag_inject.rag, "search_docs", search)
    msg = "hello"
    assert await rag_inject.maybe_prepend_first_hop(query=msg, message=msg) == msg
    search.assert_not_called()


@pytest.mark.anyio
async def test_fail_open_when_search_empty(monkeypatch, rag_cfg):
    monkeypatch.setattr(rag_inject.rag, "search_docs", AsyncMock(return_value=[]))
    msg = "query deps"
    assert await rag_inject.maybe_prepend_first_hop(query=msg, message=msg) == msg


@pytest.mark.anyio
async def test_empty_query_skips_search(monkeypatch, rag_cfg):
    search = AsyncMock()
    monkeypatch.setattr(rag_inject.rag, "search_docs", search)
    assert await rag_inject.maybe_prepend_first_hop(query="  ", message="x") == "x"
    search.assert_not_called()


def test_rag_config_defaults():
    assert rag_config() == _RAG_DEFAULTS


def test_rag_config_legacy_first_turn_keys(tmp_path, monkeypatch):
    harness = tmp_path / "harness"
    harness.mkdir()
    monkeypatch.setattr("crack_server.settings.harness_dir", lambda: harness)
    (harness / "rag.json").write_text(
        '{"first_turn_enabled": false, "first_turn_top_k": 3}', encoding="utf-8"
    )
    cfg = rag_config()
    assert cfg["first_hop_enabled"] is False
    assert cfg["first_hop_top_k"] == 3
