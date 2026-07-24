# Plan 30 RAG — leftover work (credits cut mid-Part-4)

**Status:** **complete** for Parts 4–5. Index pipeline fix landed in scripts; re-index on next boot or manual `DEP_MAX_DEPTH=1 bash /workspace/_docker/_docs_mcp_index.sh`.

---

## Locked decisions (from grill — supersede old plan text)

| # | Decision | Implication |
|---|----------|-------------|
| 1 | Index scope = **repo + direct deps** (`DEP_MAX_DEPTH=1`) | Raise depth later after quality check |
| 2 | Child sandboxes see **shared base** target volume | Fixed in `sandbox.py` |
| 3 | **Defer D5** (sigmap teardown) | Out of this pass |
| 4 | Injection = **first hop of each user turn** | Wired in `chats.py` + `sub_agents/base.py` hop 1 |

---

## Done

### Infra / compose / docs-mcp boot ✅
(all items from original cut point — unchanged)

### Part 4 ✅
| Item | Status |
|------|--------|
| `rag.py`, `routes_rag.py`, app/ui wiring | done |
| `/rag/search` timeout bug | **fixed** — `RAG_SEARCH_TIMEOUT` default 120s |
| `tests/test_rag_routes.py` | **5 tests pass** |

### Part 5 ✅
| Item | Status |
|------|--------|
| `rag_inject.py` | done |
| `settings.rag_config()` + `harness/rag.json` | done |
| `chats.py` message_builder wrap | done |
| `sub_agents/base.py` hop_n==1 inject | done |
| `tests/test_rag_inject.py` | **7 tests pass** |

### Index pipeline ✅ (script fixes)
| Fix | Detail |
|-----|--------|
| Stop server before scrape | `docs_mcp_stop_server` in `_docs_mcp_index.sh` — avoids SQLITE FK on concurrent `--clean` |
| Index before server in bringup | `_cont_start.sh` runs `_docs_mcp_index.sh` before `respawn docs-mcp` |
| Scrape error detection | grep scrape log for SqliteError/ConnectionError |
| Stamp guard | requires repo ok **and** ≥1 dep library scraped |

**Manual re-index** (long): `docker exec crack-dev bash -c 'DEP_MAX_DEPTH=1 bash /workspace/_docker/_docs_mcp_index.sh'`

### Plan docs ✅
- `0_research_code_mcp_tools.md` — v1 revisions recorded
- `1_impl/deferred_demo_and_first_turn.md` — rewritten to match landed code

---

## Out of scope (unchanged)

- D5 sigmap / AGENTS signature teardown
- Depth-4 all-deps indexing
- Parent→child live target-upper chaining
- Depth slider / `depth_map.json` post-filter
- Pipeline-stage RAG injection
- Multi-library search on demo page (blocked until dep libs in `list`)

---

## Quick reference

```bash
# search contract
docker exec crack-dev curl -s 'http://127.0.0.1:9847/rag/search?q=sandbox+overlay' | head

# host tests
cd .pi/crack/server && poetry run python -m pytest tests/test_sandbox.py tests/test_rag*.py -q
```
