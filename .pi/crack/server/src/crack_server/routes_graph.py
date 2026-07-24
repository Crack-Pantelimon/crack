"""Graphiti search and graph-explorer page."""

from __future__ import annotations

from fastapi import APIRouter, Query
from fastapi.responses import HTMLResponse, JSONResponse

from crack_server import graphiti
from crack_server import ui as _ui

router = APIRouter()


@router.get("/graph/search")
async def graph_search(q: str = Query(default="")) -> JSONResponse:
    return JSONResponse(await graphiti.search(q))


@router.get("/graph/expand")
async def graph_expand(uuid: str = Query(default="")) -> JSONResponse:
    return JSONResponse(await graphiti.expand(uuid))


@router.get("/graph", response_class=HTMLResponse)
def graph_page() -> HTMLResponse:
    body = """
    <header>
      <h1>Graph Search</h1>
      <p><a href="/">← Home</a> · <small class="muted">Graphiti + FalkorDB · telemetry disabled</small></p>
    </header>
    <section>
      <p class="muted">Search the local knowledge graph, then click a node to expand its neighbors.</p>
      <input id="graph-query" type="search" placeholder="Search the knowledge graph…" autocomplete="off" autofocus>
      <p id="graph-status" class="muted">Enter a query to begin.</p>
      <div id="graph-canvas" class="graph-canvas" aria-label="Knowledge graph"></div>
      <details id="graph-details" class="graph-details" hidden>
        <summary>Selected node</summary>
        <pre id="graph-node-details"></pre>
      </details>
    </section>
    <script src="https://cdn.jsdelivr.net/npm/cytoscape@3.32.0/dist/cytoscape.min.js"></script>
    <script src="/static/graph.js"></script>
    """
    return HTMLResponse(_ui._render_base("Graph Search", body))