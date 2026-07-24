"""Graph Search route contract tests."""

from unittest.mock import AsyncMock

import pytest
from fastapi.responses import JSONResponse

from crack_server import graphiti
from crack_server.routes_graph import graph_expand, graph_page, graph_search


@pytest.mark.anyio
async def test_graph_search_is_fail_open_for_blank_query(monkeypatch):
    monkeypatch.setattr(graphiti, "get_graph", lambda: None)
    response = await graph_search("")
    assert isinstance(response, JSONResponse)
    assert response.body.decode().startswith('{"nodes":[],"edges":[],"status":')


@pytest.mark.anyio
async def test_graph_search_returns_graph(monkeypatch):
    monkeypatch.setattr(
        graphiti,
        "search",
        AsyncMock(return_value={"nodes": [{"id": "n1"}], "edges": [], "status": {"available": True}}),
    )
    response = await graph_search("ollama")
    assert response.status_code == 200
    assert '"n1"' in response.body.decode()


@pytest.mark.anyio
async def test_graph_expand_delegates_to_graphiti(monkeypatch):
    monkeypatch.setattr(
        graphiti,
        "expand",
        AsyncMock(return_value={"nodes": [], "edges": [], "status": {"available": True}}),
    )
    response = await graph_expand("node-1")
    assert response.status_code == 200
    graphiti.expand.assert_awaited_once_with("node-1")


def test_graph_page_has_search_and_expand_client():
    body = graph_page().body.decode()
    assert "Graph Search" in body
    assert "/static/graph.js" in body
    assert "cytoscape" in body