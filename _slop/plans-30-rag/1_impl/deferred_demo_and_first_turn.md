# Plan 30 RAG — deferred work: demo page + first-hop injection

**Status:** **implemented** (Parts 4–5). Parts 1–3 landed earlier (compose, docs-mcp + Ollama, chained `target` overlay).

This doc is the execution plan for the two deferred features.

---

## Part 4 — Debug / demo search page (crack-server) ✅

### Goal

A FastAPI + htmx page that queries the shared docs-mcp SQLite store via the CLI (same path as injection) and renders the **exact** result payload the model would receive.

### Landed files

| File | Status |
|------|--------|
| `src/crack_server/rag.py` | shared `search_docs` / `normalize_hit` (CLI, fail-open, `RAG_SEARCH_TIMEOUT` default 120s) |
| `src/crack_server/routes_rag.py` | `GET /rag`, `GET /rag/search` (htmx partial) |
| `src/crack_server/app.py` | router included |
| `src/crack_server/ui.py` | sidebar "RAG" link |
| `tests/test_rag_routes.py` | unit tests |

### Routes

```
GET  /rag                 → full page (search box + results region)
GET  /rag/search?q=...    → htmx partial (results only)
```

### Notes

- **Bug fixed:** 20s subprocess timeout caused silent empty results; Ollama embed+search over a large index needs ~30–90s cold, ~10–15s warm. Default timeout raised to 120s (`RAG_SEARCH_TIMEOUT` env).
- No depth slider in v1 (deferred with depth-4 indexing).
- Search hardcodes `DOCS_MCP_LIBRARY=crack-repo` until dep libs appear in `docs_mcp_cli list`.

### Test plan (Part 4) ✅

1. `test_rag_search_empty_query_returns_blank_partial`
2. `test_rag_search_renders_hit_rows`
3. `test_rag_page_includes_debounce_attributes`
4. Manual: `curl http://localhost:9847/rag/search?q=sandbox+overlay`

---

## Part 5 — First-hop gated RAG injection ✅

### Goal

Before **hop 1 of each user exchange** (every new message, not only chat message 0), embed the user prompt, search docs-mcp, and prepend top-k chunks **only if** score ≥ threshold. Later hops in the same exchange: tool-only (`docs-search` MCP). Never blanket-prepend (sigmap failure mode).

### Config (`.pi/crack/harness/rag.json` + `settings.rag_config()`)

| Key | Default | Description |
|-----|---------|-------------|
| `first_hop_enabled` | `true` | master switch |
| `first_hop_top_k` | `5` | max chunks to inject |
| `first_hop_min_score` | `0.02` | drop hits below this (live scores ~0.02–0.05 on nomic-embed-text) |
| `first_hop_max_chars` | `12000` | cap injected block size |

Legacy `first_turn_*` keys in `rag.json` are accepted as aliases.

### Hook points (actual code — do not use invented APIs)

| Call site | When | Change |
|-----------|------|--------|
| `chats.py` → chat job worker | before `run_exchange` | `first_hop_message = await rag_inject.maybe_prepend_first_hop(query=user_msg, message=user_msg)` then `message_builder=lambda _: first_hop_message` |
| `sub_agents/base.py` → `_run_hop` | `hop_n == 1` only | `message = await rag_inject.maybe_prepend_first_hop(...)` before `arun_agent_hop` |
| `chat_engine.py` → `run_exchange` | **do not** inject here | `message_builder` is already the right seam for chats |

**Module:** `src/crack_server/rag_inject.py` — `async def maybe_prepend_first_hop(*, query, message) -> str`

### Injection format

```markdown
<rag-context>
The following documentation excerpts may be relevant (retrieval score ≥ {threshold}):

### {source} (score={score:.3f})
{snippet}
</rag-context>

{original_message}
```

Ephemeral to hop input only — not persisted as a separate user turn.

### Test plan (Part 5) ✅

| Test | File | Status |
|------|------|--------|
| Below threshold → unchanged | `tests/test_rag_inject.py` | ✅ |
| Above threshold → `<rag-context>` prepended | same | ✅ |
| Disabled via config | same | ✅ |
| Fail-open when search empty | same | ✅ |
| Hop > 1 does not call inject | wired in `base.py` (`hop_n==1` guard) | ✅ (code review) |

---

## Cross-cutting notes

- **Ollama dependency:** semantic search embeds the query → Ollama must be up; on failure, log warning and pass message through unchanged (fail open).
- **Index bringup:** `_docs_mcp_index.sh` stops any live docs-mcp server before scraping (FK constraint on concurrent `--clean`); `_cont_start.sh` runs index **before** respawning the server. Stamp requires repo scrape success **and** at least one dep library indexed.
- **No depth filter** in v1: demo page and injection search the full index (repo + direct deps once indexed).

---

## Execution order (completed)

1. Part 4 demo page + `/rag/search` timeout fix
2. `rag_inject.py` + `settings.rag_config()` + `harness/rag.json`
3. Wire `chats.py` + `sub_agents/base.py`
4. Tests (`test_rag_routes.py`, `test_rag_inject.py`)
