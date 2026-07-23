"""RAG debug page: type a query, see the EXACT hits the model would retrieve.

This is the retrieval-quality harness for the self-hosted claude-context index.
It renders what :func:`crack_server.rag.search_docs` returns — same Milvus
collection, same normalization the first-turn injection uses — so tuning
decisions (scope, thresholds) are made against ground truth, not guesses.

    GET /rag              → full page (debounced search box + results region)
    GET /rag/search?q=... → htmx partial (results table only)
"""

from __future__ import annotations

import os

from fastapi import APIRouter, Query
from fastapi.responses import HTMLResponse

from crack_server import rag
from crack_server import ui as _ui

router = APIRouter()

RAG_DEMO_LIMIT = int(os.environ.get("RAG_DEMO_LIMIT", "8"))
RAG_DEMO_MIN_QUERY_LEN = int(os.environ.get("RAG_DEMO_MIN_QUERY_LEN", "2"))

_SNIPPET_MAX = 800


def _render_hits(hits: list[dict]) -> str:
    if not hits:
        return "<p><em>No hits.</em></p>"
    rows = []
    for h in hits:
        snippet = h["snippet"][:_SNIPPET_MAX]
        rows.append(
            "<tr>"
            f'<td><code>{h["score"]:.3f}</code></td>'
            f'<td class="rag-src"><small>{_ui._esc(h["source"])}</small></td>'
            f'<td><pre class="rag-snippet">{_ui._esc(snippet)}</pre></td>'
            "</tr>"
        )
    return (
        f"<p class='muted'><small>{len(hits)} hit(s)</small></p>"
        '<table><thead><tr><th>Score</th><th>Source</th><th>Snippet</th></tr></thead>'
        f"<tbody>{''.join(rows)}</tbody></table>"
    )


@router.get("/rag/search", response_class=HTMLResponse)
async def rag_search(q: str = Query(default="")) -> HTMLResponse:
    """htmx partial: the results table for ``q`` (blank/too-short → empty)."""
    query = (q or "").strip()
    if len(query) < RAG_DEMO_MIN_QUERY_LEN:
        return HTMLResponse("")
    hits = await rag.search_docs(query, limit=RAG_DEMO_LIMIT)
    return HTMLResponse(_render_hits(hits))


@router.get("/rag", response_class=HTMLResponse)
def rag_page() -> HTMLResponse:
    status = (
        "index ready"
        if rag.available()
        else "claude-context not built yet — boot still indexing"
    )
    body = f"""
    <header>
      <h1>RAG search</h1>
      <p><a href="/">← Home</a> · <small class="muted">codebase <code>{_ui._esc(rag.CODEBASE_LABEL)}</code> · {status}</small></p>
    </header>
    <section>
      <p class="muted">Exactly what the model retrieves from the self-hosted claude-context index — gitignore-aware repo search. Use it to judge retrieval quality.</p>
      <input
        type="search"
        name="q"
        placeholder="Search repo…"
        hx-get="/rag/search"
        hx-trigger="keyup changed delay:300ms, search"
        hx-target="#rag-results"
        hx-swap="innerHTML"
        autocomplete="off"
        autofocus
      />
      <div id="rag-results"></div>
    </section>
    """
    return HTMLResponse(_ui._render_base("RAG search", body))
