# KB + Auto-compaction components — candidate report

Scope: **C (both)** — chat/session trajectory compaction **plus** improved RAG / KB ingestion, chunking, indexing, and enrichment pipelines for the current stack (`FastAPI + Milvus (claude-context) + Ollama`).

Filtering criteria used here, before writing any edit:

- **Must play nice with Python/Poetry** (we can’t fight the venv layout in `.pi/crack/server`).
- **Should be installable without replacing the existing Milvus/claude-context index** — we prefer drop-in adapters or parallel pipelines that can feed the same index.
- **Genuinely useful**, not just a name-drop from a blog.
- **Ordered by probable GitHub popularity / community health**, not by how cool the idea sounds. Where there’s real ambiguity, I say so and give a “popularity band” instead of a fake rank.

---

## 1. Context-window compaction (trajectory)

What these components do: they turn old turns/context into shorter summaries or sparse representations so long sessions stay inside a model’s context window without dropping important facts.

### 1.1 — Rust-side tooling: Amp / Codex CLI / OpenCode context compaction
**Popularity band: very high (authoritative reference implementations)**

> Reference: <https://gist.github.com/badlogic/cd2ef65b0697c4dbe2d13fbecb0a0a5f>

These are the *implementations that actually shipped context compaction* at scale (Anthropic’s Claude Code, OpenAI’s Codex CLI, OpenCode, Amp). They are **not Python libraries you pip-install** — they’re inspiration and a specification, not a drop-in.

**Why it’s still the top entry:** every later project below basically re-implements the same idea (rolling summarization + token-window pruning + selective key-fact retention). Reading the gist is the highest-leverage research to do before you write your own compaction step in `chats.py` / `pi_runner.py`.

**Fit for this repo:** **HIGH value, ZERO integration cost** — it’s reading material, then you implement a compressor inside the existing `chats.py` / `prewalk.py` flow.

---

### 1.2 — Summarization-pydantic-ai (`vstorm-co/summarization-pydantic-ai`)
**Popularity band: low–moderate (niche, very relevant)**

Context-management processor for Pydantic AI agents. Provides:

- LLM-powered rolling summarization of conversation history.
- Zero-cost sliding-window trimming (drop the oldest turns without an LLM call).
- Flexible triggers (token budget, hop count, turn age) and safe cutoffs.

**Fit for this repo:** Moderate direct value only if you’re willing to migrate agent orchestration to Pydantic AI. **Unlikely fit as-is** — your stack uses a hand-rolled `prewalk` state machine and `chats.py`, not Pydantic AI. The **patterns and prompts** it encourages are worth stealing (safe-cutoff behaviour, turning old turns into a single summary message rather than hard deletion). The code itself is probably not a drop-in.

**Verdict:** **Steal the prompt patterns; skip the library.**

---

### 1.3 — Adaptive Memory Compressor (`berkdurmus/adaptive-llm-memory-compressor`)
**Popularity band: low (academic / research repo)**

Automatically compresses conversation history to keep memory under a token limit without hurting answer quality. Uses LLM-centric adaptive compression (not a fixed budget, but a learned-ish compression ratio).

**Verdict:** Interesting as a paper-turned-repo, but low integration fit. The approach is heavier than you want for per-hop compaction inside `pi_runner`. Better as “future option” than “install now.”

---

### 1.4 — Agent-Memory-Compressor (`dakshjain-1616/Agent-Memory-Compressor`)
**Popularity band: very low**

Summarization + embedding-based retrieval with tunable compression ratios. Lightweight code.

**Verdict:** Tiny repo, no write-up indicating it’s production-ready. Conceptually close to what we want (summarize the tail, keep the head, use embeddings to surface just-in-time relevant old state), but not worth installing until there’s more maintenance signal. **Skip for now; read if you want a reference implementation.**

---

### 1.5 — Compact Memory (`scottfalconer/compact-memory`)
**Popularity band: low**

Open-source toolkit for developing and sharing context compression strategies. The README promises a framework/strategy catalogue.

**Verdict:** Scanned; looks like a research/strategy registry, not a pipeline. Low maintenance signal. **Skip; use as a strategy vocabulary list.**

---

## 2. Knowledge-base building and indexing

What these components do: better ingestion, chunking, metadata enrichment, incremental indexing, and retrieval setup on top of your Milvus + Ollama base.

### 2.1 — LlamaIndex (Python, `run-llama/llama_index`)
**Popularity band: very high**

The leading Python framework for document indexing and RAG. The scoreboard reasons:

- Strong incremental-index story (can do `VectorStoreIndex.from_documents` + `insert` / `refresh` per file).
- Rich ingestion toolkit: sentence-window chunking, markdown/HTML parsers, metadata extraction.
- Milvus is a first-class vector store (official `llama-index-integrations` package).
- Ollama is natively supported as the embedding provider.
- Built-in evaluators for retrieval quality.

**Fit for this repo:** **HIGH**. Your `rag.py` already shells out to `claude-context search.mjs` against a Milvus collection. A cleaner path is to keep `claude-context` as the query path but pipe ingest/compaction through LlamaIndex to the same Milvus collection.

> Probable route: a new async worker (or a FastAPI `POST /rag/index`) that watches for file changes (`watchfiles` is already a dep), chunks via LlamaIndex, embeds via Ollama, and upserts to Milvus. The existing `search.mjs` query path keeps working.

**Main risk:** LlamaIndex is large. Add it as a dev-group or a dedicated `rag-tools` extra so it doesn’t bloat `crack-server` itself.

---

### 2.2 — Graphiti (Zep, `getzep/graphiti`) + Zep memory
**Popularity band: moderate to high**

Real-time knowledge graph builder for AI agents. Zep also includes memory primitives (short/long-term memory, entity extraction, summarization).

**Fit for this repo:** Dual value:

1. **KB enrichment:** ingest a file, extract entities + relationships, store in a graph (Milvus is Ok for this, or add a lightweight graph store — Zep uses SQLite by default).
2. **Memory/compaction:** Zep’s memory layer is a drop-in Python API that gives you compaction and episodic recall without rewriting your whole chat engine.

**Caveat:** Graphiti runs its own graph builder + optional MCP server. It’s not a Milvus-native pipeline out of the box. You’d treat it as a **sidecar enrichment step** (graph extraction in addition to, not replacing, vector search).

**Verdict:** Strong candidate for **KB enrichment layer**; moderate candidate for **trajectory compaction** only if you accept an extra SQLite store. Recommend as a second-phase install, not a first-day drop-in.

---

### 3. “Build your own” — recommended custom components

Often the right answer with these systems is to write two focused components inside `.pi/crack/server` using the existing infrastructure, instead of adding a heavy external dependency.

### 3.1 — Rolling summarizer for chat trajectories
**Where it lives:** a new `compaction.py` module under `src/crack_server/`.

**Shape:** on every Nth turn (or when a `context_stats` token budget is exceeded), compress the oldest K turns into a single summary message using the existing Ollama-backed model selector in `models.py`. Persist the summary as a synthetic turn so it stays in the `turns` list and is rendered by `render.py`.

**Why do this instead of installing a library:** the exact rules for compaction belong to your trajectory schema (`prewalk` phase, model tags, `reason` field from the existing code). A generic library won’t respect those rules.

**Fit:** highest-value, highest-integrity piece you can ship next.

---

### 3.2 — RAG ingest worker with incremental indexing
**Where it lives:** a new async worker or a small FastAPI route (`/rag/index` or a queue job in `worker.py`).

**Shape:** use `watchfiles` (already a dep) to watch the codebase + docs. On change, chunk via a simple recursive markdown/heading splitter (or via LlamaIndex if you adopt it), embed via Ollama (`all-minilm` or `mxbai-embed-large` per `rag.py` constants), and upsert into the same Milvus collection that `claude-context` already queries. Because `rag.py` already expects Milvus shape, `search.mjs` keeps working.

**Why do this instead of replacing claude-context:** `claude-context` search.mjs *only* reads/query — it doesn’t index. You’re not in conflict; you’re adding a complementary writer. This also gives you **incremental, file-granular** reindexing (something `claude-context` builds as a batch), which is exactly the compaction tool the backlog asked for.

---

## 4. Priority / effort table

| Candidate                      | Category                    | Effort  | Value | Popularity band         | Install or Study |
|--------------------------------|-----------------------------|---------|-------|------------------------|------------------|
| Amp/Codex compaction internals | Compaction (reference only) | Low     | High  | Very high              | Study            |
| Rolling summarizer (in-house)  | Compaction                  | Medium  | High  | —                      | Build            |
| LlamaIndex                     | RAG/KB ingest               | Medium  | High  | Very high              | Install (optional extra) |
| Graphiti / Zep memory          | KB enrichment + memory      | Medium  | Medium| Moderate-to-high       | Phase 2 install  |
| Summarization-pydantic-ai      | Compaction                  | Low     | Low   | Low–moderate           | Skip / Steal prompts |
| Adaptive Memory Compressor     | Compaction                  | Low     | Low   | Low                    | Skip / Read      |
| Agent-Memory-Compressor        | Compaction                  | Low     | Low   | Very low               | Skip / Read      |
| Compact Memory                 | Compaction (strategy list)  | Very low| Low   | Low                    | Skip / Read      |

---

## 5. Recommended next action

1. **Read the Amp/Codex compaction write-up** (the gist link above) — 30 minutes, highest-leverage research for the semantics of what to keep/drop/compress.
2. **Start the in-house rolling summarizer** (`compaction.py`) shaped around your existing `turns` schema, triggered by `context_stats` budgets or a fixed hop cadence. This is the highest-value, lowest-friction real deliverable.
3. **Adopt LlamaIndex for ingest** so you get incremental vector indexing for free. Add it as an optional Poetry group or a separate `rag-tools` venv layer; don’t pollute `crack-server`’s main runtime unless you actually use it.
4. **Defer Graphiti/Zep** to a phase-2 spike after you have a working ingest loop.
