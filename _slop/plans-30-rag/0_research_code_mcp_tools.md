Done. The research + plan-of-record is at _slop/plans-30-rag/0_research_code_mcp_tools.md. Nothing built — this is the design we grilled out.

Summary
The landscape (§1–3): the leaked Claude Code deliberately dropped RAG for agentic grep because grep won "by a lot" — but that was on frontier models. Our weak nemotron workers are exactly the population that benefits from handed-to-them retrieval, so RAG-as-a-tool is defensible as long as we don't blanket-inject it (the sigmap mistake). Two distinct needs: churning internal code (grep stays primary, RAG is recall booster) vs. static pinned deps (the real "Context7-but-self-hosted" win).

Tool ranking: arabold/docs-mcp-server (1.6k★, self-contained SQLite, Ollama-local embeddings, indexes repo + Rust/Python deps) beat zilliztech/claude-context (12.2k★, AST-aware but drags in a 3-container Milvus stack) purely on the least-infra axis.

Decisions locked (§7) and turned into architecture (§8) + a small build plan (§9):

docs-mcp-server, cloned-at-pinned-commit into the data dir, node not uv (it's Node 22+ — corrected your assumption).
Index DB at /workspace/target/rag/ → your :O-overlay instinct was exactly right; that path is already per-sandbox overlaid, so re-index isolation is free.
Per-sandbox server for agent isolation + one crack-dev instance for host tools & the demo page.
Ollama as a separate nvidia-GPU container wired from run.sh.
Depth-4 all deps, tagged by depth via a side name@version→depth map; demo page gets the debounced search box + max-depth slider.
First-turn-only gated injection, MCP tool otherwise.
Two things I answered that you asked: indexing is manual/triggered, not auto (docs-mcp doesn't watch files — deps index once, repo re-indexes on boot/on-demand); and the sub-agent caveat — a sub-agent gets a fresh target overlay, so it sees the base index, not the parent's mid-run re-index (§8, accepted as out-of-scope for v1).

Open risks I flagged for build time (§10): pi-mcp-adapter is stdio-only so a supergateway --sse bridge is mandatory; a monolithic SQLite over overlayfs copies-up whole on first write (may need sharding); and the depth-4 blast radius (300–600+ crates) needs a real look at index time / volume size / embedding throughput.

Want me to spike any single piece next — e.g. the dependency-walker (cargo metadata + depth map) to size the real crate count, or a throwaway docs-mcp + ollama compose to sanity-check retrieval quality before we commit the harness wiring?


# Research: RAG code/doc search MCP tools for crack-dev workers

**Goal.** Give our pi agents + sub-agents (and host coding tools) a semantic
retrieval surface over (a) our own git tree, (b) our Rust + Python direct and
transitive dependencies (depth 4), indexed once and persisted on the
`crack-harness-data` volume, exposed as MCP tools — while **writing as little
code as possible** (install-and-run an existing project, don't rewrite
Context7) and **retiring the sigmap/AGENTS.md signature dump**.

Status: **Parts 1–5 landed** (compose, docs-mcp, demo page, first-hop injection). See `1_impl/deferred_demo_and_first_turn.md` for execution detail and `2_left.md` for cutover notes.

**v1 scope revisions (supersede §7–9 where they conflict):**

| Topic | v1 choice |
|-------|-----------|
| Dep scope | **`DEP_MAX_DEPTH=1`** (repo + direct deps only). Depth-4 + depth slider deferred until retrieval quality is validated. |
| Child sandboxes | Share the **base** `target` volume lower (not parent's `target-upper`). Fixed in `sandbox.py`. |
| D5 sigmap teardown | **Deferred** — signatures still in AGENTS.md. |
| Injection timing | **First hop of every user exchange** (each new message turn), not only chat message 0, not every hop inside an exchange. |
| Score threshold | Live nomic-embed scores cluster ~**0.02–0.05**; default `first_hop_min_score=0.02` (not 0.35). |
| Demo page | Debounced search box only — **no depth slider** in v1. |

---

## 1. What the leaked coding tools actually do (and the RAG irony)

The April 2026 Claude Code source-map leak (~512k lines of TS in a public npm
sourcemap) got picked apart publicly. The retrieval-relevant findings:

- **Claude Code deliberately does *not* use RAG / a vector index.** Retrieval is
  a stack of live filesystem tools: `Glob` (file discovery) → `Grep`
  (ripgrep content search) → `Read` (precise reads) → `LSPTool` → `FileIndex`.
  No persistent vector DB for code content by default.
- Anthropic's stated rationale (Boris Cherny, Latent Space / HN): early Claude
  Code *did* ship a local vector DB + RAG; **agentic grep-search outperformed it
  "by a lot"**, and avoids index-lag (edit a file, ask 100 ms later, it reads
  the new bytes — no re-embed).
- Other leaked bits: a three-layer memory (persistent `memory.md` + grep layer +
  an unshipped "Chyros" background daemon), and unreleased `KAIROS`/`PROACTIVE`
  autonomous-daemon flags. Not retrieval; noted for completeness.

**Counter-camp.** Milvus/Zilliz published "Why I'm Against Claude Code's
Grep-Only Retrieval" — grep loops burn tokens and stall on large repos; a
precomputed semantic index is cheaper per query and catches *meaning* (e.g.
"where do we throttle uploads" when the code says `rate_limit`).

**Why RAG is defensible *for us* even though Anthropic dropped it.** Anthropic's
finding was on *frontier* models that are excellent at driving an iterative grep
loop themselves. Our workers run cheap/weak models (`nvidia/nemotron-*`, the
whole reason `.pi/SYSTEM.md` spoon-feeds tool JSON — see
[[pi-system-md-tool-guidance]]). Weak models are exactly the population that
benefits most from *handed-to-them* retrieval instead of a self-directed search
loop. So: RAG-as-an-extra-tool = good bet; **RAG-as-always-on-prompt-injection =
repeats the sigmap mistake** (tokens spent on irrelevant hits). More on that in §5.

Sources:
- <https://www.lowcode.agency/blog/claude-code-source-code-leaked>
- <https://vadim.blog/claude-code-no-indexing/>
- <https://rust-trends.com/posts/ripgrep-claude-code/>
- <https://milvus.io/blog/why-im-against-claude-codes-grep-only-retrieval-it-just-burns-too-many-tokens.md>
- <https://zerofilter.medium.com/why-claude-code-is-special-for-not-doing-rag-vector-search-agent-search-tool-calling-versus-41b9a6c0f4d9>

---

## 2. Two distinct needs (don't conflate them)

The ask is really two retrieval problems with different best tools:

| Need | Corpus | Churn | Best chunking |
|------|--------|-------|---------------|
| **A. Internal code search** | our git tree | high (edits constantly) | code/AST-aware |
| **B. Dependency knowledge** | Rust+Python deps, depth 4 | ~never (pinned versions) | doc + source |

A wants incremental re-index and code-aware chunking. B is a big, static,
index-once corpus — and is exactly the "Context7 but self-hosted for *your*
pinned versions" problem. A single tool *can* cover both, at some quality cost.

---

## 3. Candidate MCP servers (ranked by fit to OUR goals)

Ordered by my preference for: least infra in one Docker container · native
HTTP-MCP reachable by both in-container agents and host tools · one on-volume
store · self-hostable embeddings · maturity.

### #1 — `arabold/docs-mcp-server` ("Grounded Docs MCP") — **my top pick**
- **Popularity:** ~1.6k★. Explicitly markets itself as the open-source
  Context7 / Ref.tools / Nia replacement.
- **Self-host:** *fully* self-contained. **SQLite** store (no external vector
  DB), embeddings **optional** and pluggable (Ollama = local/offline, or
  OpenAI/Gemini/Azure). Runs entirely on-box; "your code never leaves your
  network." One container.
- **Sources it indexes:** websites, GitHub repos, **npm, PyPI**, local
  files/folders, ZIPs — plus 90+ file types incl. **Rust, Python, Go, C/C++**.
  So it can index both our repo dir *and* dependency packages by name+version.
- **MCP:** native **HTTP/SSE** (`:6280/sse`) + a built-in web UI and a job queue
  for managing/refreshing indexed libraries.
- **Why it wins for us:** slots into the existing harness with near-zero new
  code. It serves HTTP MCP natively (like `@playwright/mcp` — no supergateway
  bridge needed), persists to a mounted dir on `crack-harness-data`, and is one
  process. The web UI + job queue is the "index these 200 deps" surface we'd
  otherwise have to build. Directly kills sigmap.
- **Weakness:** generic text/markup chunking, **not AST-aware** — code-symbol
  search quality is below a purpose-built code indexer. Best at *docs/prose*;
  merely-OK at "find the function that does X" over raw source.
- Repo: <https://github.com/arabold/docs-mcp-server>

### #2 — `zilliztech/claude-context` (was CodeIndexer) — best *code* quality, heaviest infra
- **Popularity:** ~12.2k★ — by far the most adopted; purpose-built code search
  MCP for coding agents.
- **Strengths:** **AST-based chunking**, hybrid **BM25 + dense** search,
  **incremental re-index via Merkle tree** (only changed files) — ideal for
  need A / our churning tree. Rust + Python + ~15 langs. Embeddings via
  **Ollama** (local) / OpenAI / Voyage / Gemini.
- **The catch:** vector store is **Milvus** — self-hosting means a
  docker-compose of **etcd + minio + milvus** (3 services), or **Zilliz Cloud**
  (external SaaS = violates "store on our volume / self-host"). No embedded
  SQLite/Milvus-Lite path in this Node MCP. That's a real infra tax against the
  "least code/infra, one container" goal.
- **When it's worth it:** if internal-code semantic quality is the priority and
  we accept running Milvus. Could pair with #1 (claude-context for code, docs-
  mcp-server for deps) at the cost of two stacks.
- Repo: <https://github.com/zilliztech/claude-context>

### #3 — `rakuv3r/open-context7` — literal Context7 clone
- Full-stack Context7 replacement: backend API + web UI + vector DB via
  docker-compose. Heavier and much less proven than #1/#2; more moving parts to
  own. Only compelling if we want the exact Context7 UX. Low maturity signal.
- Repo: <https://github.com/rakuv3r/open-context7>

### #4 — `salfatigroup/mcp-code-search` — zero-infra, but unproven
- **Local-first done right on paper:** **SQLite-vec** (no external service),
  local `multilingual-e5-large-instruct` embeddings (~1.2 GB, offline after
  download), AST + call-graph tools, FastMCP (7 tools). Python 3.13 + PyTorch.
- **Why not higher:** **~0★, no Docker guidance, unproven.** Architecturally it's
  the "ideal" (single process, no vector-DB server, AST-aware) but adopting a
  0-star PyTorch-heavy project is a maintenance bet I wouldn't lead with.
  Worth a second look *only* if we want AST quality without Milvus and are
  willing to harden it ourselves.
- Repo: <https://github.com/salfatigroup/mcp-code-search>

**Also-rans:** `giuseppeferretti/sqlite-rag-mcp` (Ollama + sqlite, lexical
fallback — tiny), `felixscherz/mcp-rag`, `Docfork` (MIT, cloud-lib docs). None
beat #1 on the install-and-forget axis.

---

## 4. Recommended shape (least-code path)

**Lead with `docs-mcp-server` as the single retrieval service; treat
`claude-context`+Milvus as a possible upgrade for need A only if code-search
quality proves insufficient.**

Concretely, mirroring how the harness already runs HTTP MCPs (`_cont_start.sh`
`respawn` + fixed published ports in `run.sh`, store under
`$CRACK_HARNESS_DATA_DIR`):

1. **Run one `docs-mcp-server` container/process** in `crack-dev`, DB dir on
   `crack-harness-data` (e.g. `$CRACK_HARNESS_DATA_DIR/rag/docs-mcp/`). It
   serves HTTP/SSE natively → publish a fixed port (next in the 993x range,
   e.g. `9933`) via `run.sh`, exactly like firefox/blender. No supergateway.
2. **Embeddings:** run a small **Ollama** with a local embed model
   (`nomic-embed-text` or `qwen3-embedding`) — or start embeddings-off (lexical)
   and switch on later. (Decision D3.)
3. **Add it to `.mcp.json`** (the file synced to `/root/.config/mcp/mcp.json` in
   `_sandbox_common.sh`) so **both** in-container pi agents **and** sub-agents
   get the tools; the published port covers **host** coding tools. One entry.
4. **Indexing glue = one small script** (the only real code we write):
   - Index the **repo working tree** as a local-folder job (re-run on boot /
     on demand; docs-mcp handles refresh).
   - Enumerate **deps to depth 4**: `cargo metadata --format-version 1` for Rust
     (walk the dependency graph, dedup by name@version), and
     `uv pip list` / lockfile parse for Python. Feed each `name@version` (and/or
     its docs.rs / PyPI URL, or its vendored source path) into a docs-mcp index
     job. This is glue, not a rewrite.
5. **Delete the sigmap signature blocks from all `AGENTS.md`** and drop the
   `sigmap`/`sigmap --monorepo` calls from `sigmap.sh` / boot. The MCP search
   tool replaces "sigmap ask". (Decision D5 on how far to strip.)

New code footprint: ~1 respawn stanza, ~1 published port, ~1 `.mcp.json` entry,
**~1 dependency-walker script**, minus the sigmap deletions. That's it.

---

## 5. Sharp edges / things I'd push back on

- **Depth-4 transitive over *all* deps = possibly hundreds of libraries.**
  Rust `cargo metadata` alone can surface 300–600 crates for a Bevy-class tree.
  Embedding + storing all of that is real time/disk/compute, and most of it is
  noise (proc-macro crates, `windows-sys`, etc.). I'd want a scope cut (direct +
  1–2 levels, or an allowlist), not a blind depth-4 sweep. (Decision D2.)
- **"RAG into the initial user prompt" is the sigmap trap, again.** You're
  retiring sigmap-in-AGENTS.md *because* it spent tokens on unrelated queries.
  Auto-prepending top-k RAG hits to every prompt is the same failure mode with a
  fancier retriever. Strong recommendation: expose it as a **tool the model
  calls when it wants**, and only *optionally* prepend for the very first user
  turn behind a relevance threshold — not on every hop. (Decision D4.)
- **Index-lag on need A is real.** A vector index of our own churning tree goes
  stale between edits; claude-context's Merkle re-index mitigates it, docs-mcp's
  folder re-scan is coarser. For our own code, agentic grep (what we already
  have) stays the source of truth; RAG is the *recall booster*, not the primary.
- **Two-tool split doubles ops.** If we later add claude-context for code, we're
  running Milvus (3 containers) + docs-mcp + Ollama. Worth it only if #1's code
  recall measurably underperforms. Start with one.
- **Embeddings dependency.** Fully-offline semantic search needs a local embedder
  (Ollama + model, ~hundreds of MB–GB in the image/volume). Lexical-only avoids
  that but is barely better than grep. Pick your poison (D3).

---

## 6. Open decisions — see Grill

- **D1 — Tool:** docs-mcp-server only (recommended) · docs-mcp + claude-context/Milvus · claude-context only · other.
- **D2 — Dep scope:** true depth-4 all deps · direct+2 levels · allowlist of crates/pkgs that matter.
- **D3 — Embeddings:** local Ollama embedder (offline, +image weight) · lexical-only to start · external API.
- **D4 — Prompt injection:** tool-only (recommended) · first-turn-only behind threshold · always prepend.
- **D5 — sigmap teardown:** delete signature blocks + keep `sigmap.sh` as no-op stub · rip sigmap out entirely · leave sigmap, add RAG alongside.

---

## 7. Decisions LOCKED

| # | Decision | Choice |
|---|----------|--------|
| D1 | Tool | **`arabold/docs-mcp-server`** only (no Milvus/claude-context for now). Cloned at a pinned commit into the data dir *only if absent*, built + run with **node** (⚠️ not `uv` — it's Node 22+/TypeScript), pointed at `/workspace`. |
| D2 | Dep scope | **v1: direct deps only (`DEP_MAX_DEPTH=1`)**. Depth-4 + per-crate depth tagging deferred — validate retrieval on repo + directs first, then raise depth. |
| D3 | Embeddings | **Local Ollama** as a **separate GPU (nvidia) container** (`ollama/ollama` from Docker Hub), started from `_docker/run.sh`, joined to `crack-net`, reachable by crack-dev *and* sandboxes. |
| D4 | Prompt delivery | **First hop of each user exchange**, gated by relevance threshold; MCP search tool on later hops. Not blanket-prepend (sigmap failure). Not only the chat's first message forever. |
| D5 | sigmap teardown | **Deferred** for this pass. |
| D6 | Depth filter | **Deferred** with depth-4. Demo page searches full v1 index (no slider). |

Correction folded in: docs-mcp-server is **Node**, indexing is **manual/triggered** (no file-watch), transport is **HTTP/SSE only** (no stdio) — so pi's stdio `.mcp.json` needs a **stdio↔SSE bridge** (supergateway `--sse`, the mirror of the host bridges already in `_cont_start.sh`).

---

## 8. Final architecture

Storage & isolation (the crux):
- **Index DB lives at `/workspace/target/rag/index.db`** (the `crack-dev-target-dir`
  volume). That path is **already `:O`-overlaid per sandbox**
  (`sandbox.py:264`), so:
  - built **once in crack-dev** → lands in the real target volume (the shared
    "base index");
  - every sandbox inherits it **read-only via the overlay lower** — zero copy;
  - an agent that re-indexes (changed a dep) copies-up into **its own** overlay
    upper → automatic per-conversation isolation, no new mount wiring.
- **Caveat (accepted):** sub-agents get a *fresh* target overlay over the same
  volume lower (they share the parent's frozen `/workspace` base but not its
  target-upper — `sandbox.py:246-264`), so a sub-agent sees the **base index**,
  not the parent's *mid-run* re-index. Live parent→child re-index sharing is
  out of scope for v1.
- The **`name@version → depth` side table** (D6) lives next to the DB, e.g.
  `/workspace/target/rag/depth_map.json`.

Processes:
- **Ollama** — `ollama/ollama` GPU container on `crack-net` (`run.sh`), model
  (`nomic-embed-text` or `qwen3-embedding`) pulled once into an `ollama` volume.
  Both index-time and query-time embedding hit `http://ollama:11434`.
- **docs-mcp in crack-dev** — serves HTTP/SSE over the base index; published on
  a fixed 993x port (next free, e.g. `9933`) via `run.sh` for **host coding
  tools**, and queried by the **crack-server demo page**.
- **docs-mcp per sandbox** — lazily launched (à la `_blender_mcp_lazy.sh`),
  binds localhost inside the sandbox over that sandbox's overlay view; a
  **supergateway `--sse` stdio bridge** is the `.mcp.json` entry the pi-mcp-
  adapter launches, so **pi agents + sub-agents** get the search tools. Added to
  the `.mcp.json` synced in `_sandbox_common.sh` → one entry covers agents and
  sub-agents.

Retrieval delivery:
- **MCP search tool** — always available to the model on demand.
- **First-hop gate** — before hop 1 of **each user exchange**, embed the user
  prompt, run a docs-mcp search, and prepend the top-k **only if** score ≥
  threshold; nothing on later hops within the same exchange. Wired in
  `chats.py` (pre-`run_exchange` message) and `sub_agents/base.py` (`hop_n==1`).

Demo / debug page (crack-server, FastAPI + htmx):
- `GET /rag` + `GET /rag/search` with a search box (**300 ms debounce**);
  calls `rag.search_docs` (CLI against the shared store) and renders the
  *exact* hits the model would receive. No depth slider in v1.

## 9. Build plan (small surface)

1. **Ollama container** in `run.sh` (GPU flags, `crack-net`, volume, one-time
   `ollama pull <embed-model>`).
2. **docs-mcp bootstrap script** (data-dir clone at pinned commit if absent →
   `npm ci && build`), run in crack-dev + expose port in `run.sh`; add to
   `.mcp.json` via a supergateway `--sse` stdio bridge; lazy per-sandbox
   launcher.
3. **Dependency-walker script** (the one real chunk of logic): `cargo metadata`
   over every `Cargo.toml` + Python resolver over `pyproject.toml`/lock to
   depth 4; dedup `name@version`; compute BFS depth; feed each library's
   **local source** (cargo registry cache / site-packages — offline, exact
   version) into a docs-mcp index job; write `depth_map.json`. Index the repo
   tree as a local-folder job. Runs once/cached in crack-dev before sandboxes
   start.
4. **First-turn gate** in prewalk/chat_engine (embed prompt → search →
   threshold → prepend).
5. **Demo page** route + template (debounced search box + depth slider).
6. **sigmap teardown** (D5 still open in detail): strip the auto-generated
   signature blocks from all `AGENTS.md`, retire `sigmap.sh` from boot.

## 10. Open risks / to verify during build

- **pi-mcp-adapter ↔ HTTP/SSE**: confirm the adapter can only consume stdio (it
  launches `command`/`args`) → the supergateway `--sse` bridge is mandatory, not
  optional. Verify a bridged docs-mcp shows up as tools in a pi hop.
- **Monolithic sqlite over overlayfs**: first write copies-up the *whole* DB
  into the sandbox upper. If that's multi-GB, per-sandbox re-index is costly →
  consider **sharding the DB** (per-depth or per-library files) so only touched
  shards copy up.
- **Index-before-sandbox ordering**: ensure the base index exists in the target
  volume before any sandbox mounts it as a lower.
- **Query-time embedding dependency**: semantic search embeds the *query* too →
  sandboxes must reach `ollama:11434` (they're on `crack-net`, so OK) — but a
  slow/cold GPU model adds per-query latency; measure.
- **Depth-4 blast radius**: still 300–600+ crates; watch initial index time,
  volume size, and Ollama embedding throughput. The depth tags let us dial back
  default search scope later without re-indexing.
