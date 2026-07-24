# Review: merge of 3 pending chats

**Date:** 2026-07-24  
**Base before merge:** `0f6175dd` (`fix chat` — removed the self-mod test gate that blocked Commit)  
**Chats:**

| Chat ID | Title | URL |
|--------|-------|-----|
| `1784904773767` | Fix async blocking in sandbox creation with to_thread | http://localhost:9847/chats/1784904773767 |
| `1784909509904` | Implement Rolling Summarizer Compaction w/ Trajectory | http://localhost:9847/chats/1784909509904 |
| `1784910041469` | Integrate Graphiti, FalkorDB, Ollama LLM; add Search Page | http://localhost:9847/chats/1784910041469 |

Patches were copied out of `/crack-harness-data/unscripted_chats/<id>/patch.diff` via `docker exec` / `docker cp`, then applied on the host with `git apply` (3-way where needed).

---

## Resulting commits

| Commit | Source | Notes |
|--------|--------|-------|
| `7c48cb51` | chat `1784909509904` | Already on master before this merge (`✓ Committed` in traj). Re-verified: reverse-apply of the chat patch checks clean — content fully present. **No new commit.** |
| `a80c372c` | chat `1784904773767` | Applied with `--3way`. Conflict in `patch.py` resolved by **keeping the gate removed** (the hunk only async-ified `container_exists` inside the deleted self-mod gate). |
| `b84db4eb` | chat `1784910041469` | Applied cleanly. |
| `14a9cda3` | post-merge fix | Graphiti → Zep Ollama guide (`deepseek-r1:7b` + `OpenAIGenericClient`), embeddings → `all-minilm@384`, standalone MCP + mounted config. |
| `1085da7` | post-merge cleanup | Drop unused `context_guard` import in `chats.py` left by compaction. |

Working copies of the three patches + prompts: `_slop/merge3-patches/` (untracked helper dump).

---

## Docker / runtime

`_docker/run.sh` brought up new services. After Ollama model pull + compose fixes:

| Service | Status | Notes |
|---------|--------|-------|
| `falkordb` | Up | Accepts connections on `127.0.0.1:6379` |
| `falkordb-browser` | Up | UI on `127.0.0.1:3000` |
| `graphiti-mcp` | Up | **standalone** image; LLM `deepseek-r1:7b`, embedder `all-minilm`; MCP on `127.0.0.1:8000/mcp/` |
| `ollama` | Up | Models: `deepseek-r1:7b` (4.7 GB), `all-minilm:latest` (45 MB), `nomic-embed-text` (unused by Graphiti now) |
| `crack-dev` | Up | Poetry installed `graphiti-core`; `/graph` and `/graph/search` return 200; status `available: true` |

**Boot fixes applied after first bring-up:**

1. Native Graphiti needed `OPENAI_API_KEY=ollama` (SDK credential check) even when config passed `api_key`.
2. MCP image ignored `OPENAI_BASE_URL` / `MODEL_NAME` — needs `OPENAI_API_URL` and a real config yaml for model/dim overrides.
3. Combined MCP image also spawned embedded FalkorDB + browser; switched to `:standalone` + sibling `falkordb` / `falkordb-browser`.
4. Tutorial LLM is `deepseek-r1:7b` via `OpenAIGenericClient` (not `OpenAIClient` / `llama3.2`). Embeddings use RAG’s `all-minilm` @ 384, not `nomic-embed-text`.

Tests in container: `tests/test_compaction.py` + `tests/test_graph_routes.py` + `tests/test_wait_join.py` → **21 passed**.

---

## Per-chat review vs prompt

### 1) Async sandbox (`1784904773767`)

**Prompt intent:** Offload blocking sandbox/git I/O from the asyncio loop (`to_thread`, timeouts, materialise semaphore); stop freeze during `ensure_sandbox`.

**Delivered in tree:**

- Core plan already landed in `sandbox.py` before this chat’s residual patch (`to_thread` on snapshot/materialise, `_SANDBOX_MAT_SEM`, `_GIT_SUB_TIMEOUT`).
- Residual patch added: async `container_exists`, vision route async resolve, `pi_rpc` session kill via `to_thread`, `git_utils` `timeout=` on checkpoint commits.
- Conflict hunk targeting the self-mod gate was correctly dropped (gate removed in `0f6175dd`).

**Issues / leftovers:**

- `kill_pid_file` in `pi_proc.py` still calls `kill_session_sync` synchronously. That helper is sync by design and is used from stop paths; not part of this chat’s residual diff, but still a possible loop stall if invoked from the event-loop thread under load.
- Prompt’s “status: already implemented” for Edits A–D matches the codebase; this merge only finished the leftover call sites.

**Verdict:** Acceptable. Main freeze path is covered; residual sync kill is a minor follow-up.

---

### 2) Rolling summarizer compaction (`1784909509904`)

**Prompt intent:** 75% trigger; top-level + sub-agents; orange trajectory note with tokens/msgs/duration; new pi session after compact; report in `_slop/report-1-compaction.md`; research Amp/Codex patterns.

**Delivered:**

- `compaction.py` with `COMPACTION_THRESHOLD = 0.75`, structured summary prompt, session re-seed, traj note type `compaction`.
- Wired in `chat_engine.py` and `sub_agents/base.py` via `compact_if_needed` before hops.
- UI: `.traj-note--compaction` orange border (`#e67e22`), render shows `tokens A→B`, `msgs`, duration.
- Tests + report present. Commit already on master.

**Issues / inconsistencies:**

1. **`context_guard.FORCE_STOP_THRESHOLD` still 0.75** with force-stop helpers and tests, but production chat/sub-agent paths no longer call `force_stop_*` — compaction owns the threshold. Latent dual meaning of “75%” (compact vs kill) is confusing; force-stop is effectively dead runtime code.
2. **`chats.py` imported `context_guard` unused** — removed in `1085da7`.
3. `needs_compaction()` in `context_guard` is a thin wrapper around `compaction.should_compact` and appears unused outside that module.

**Verdict:** Feature matches the prompt. Cleanup of the old force-stop story is recommended later (raise force-stop threshold or delete it).

---

### 3) Graphiti / FalkorDB / Graph Search (`1784910041469`)

**Prompt intent:** Graphiti + Ollama + FalkorDB; telemetry off; Poetry dep; Graph Search page; FalkorDB UI; example prompts; MCP in `.mcp.json`; don’t re-run docker from the sandbox.

**Delivered (after post-merge fixes):**

- Poetry `graphiti-core[falkordb]`, `graphiti.py` (fail-open), routes `/graph`, `/graph/search`, `/graph/expand`, Cytoscape client, sidebar link, CSS.
- Compose: `falkordb`, `falkordb-browser`, `graphiti-mcp` (standalone) + crack-dev env for Ollama/FalkorDB/telemetry.
- `.mcp.json` graphiti → `mcp-remote http://graphiti-mcp:8000/mcp/`.
- `_slop/example-prompts-graphiti.md` updated with model notes.
- Native client follows [Zep Ollama (Local LLMs)](https://help.getzep.com/graphiti/configuration/llm-configuration): `OpenAIGenericClient` + `deepseek-r1:7b` + reranker; embeddings **`all-minilm` / 384** (same as RAG), not tutorial’s `nomic-embed-text`.

**Issues / watch items:**

1. Original chat patch used `OpenAIClient` + `llama3.2` + `nomic-embed-text` — incorrect vs Ollama guide and vs RAG embedder. Fixed in `14a9cda3`.
2. MCP server log still says “Creating OpenAI client” (image factory). Config points at Ollama URL/models; if MCP episode ingestion fails on `/v1/responses`, the image may need a Generic client path — monitor first real MCP write.
3. Empty graph search returns empty nodes (expected until episodes are seeded). Status reports models correctly.
4. CDN Cytoscape dependency (`cdn.jsdelivr.net`) — fine for local use; offline would break the explorer.

**Verdict:** Prompt requirements met after the Ollama/`all-minilm` fix. Stack boots cleanly.

---

## Cross-cutting

- Prior Commit failures were from the **self-mod pytest gate** (removed in `0f6175dd`). That unblocked human Commit; these merges were done offline with `git apply` as requested.
- Chat `1784909509904` traj shows both `✓ Committed` and later `✗ Test gate failed` / sandbox-gone errors — consistent with retries after sandbox destruction while the gate still existed.
- Compaction and Graphiti both touch `app.css`; applied without conflict because compaction was already merged first.

---

## Suggested follow-ups (not done)

1. Retire or raise `FORCE_STOP_THRESHOLD` so it cannot fight compaction if someone rewires force-stop into the hop loop.
2. Exercise Graphiti end-to-end: seed episodes via MCP or Python, then `/graph` expand.
3. Confirm MCP episode add works with `deepseek-r1:7b` structured output (7B models can be flaky on JSON schemas — Zep docs warn about this).
4. Optionally wrap `kill_session_sync` in `to_thread` where called from async stop paths.
5. Delete or `.gitignore` `_slop/merge3-patches/` when no longer needed.
