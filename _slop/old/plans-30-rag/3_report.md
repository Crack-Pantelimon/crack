Plan 30 RAG leftovers are done. Summary:

A. /rag/search bug — fixed
Root cause: 20s timeout, not env/PATH. Ollama embed+search over the index takes ~12s warm / ~70–90s cold. Logs showed rag: search timed out after 20.0s.

Default timeout raised to 120s via RAG_SEARCH_TIMEOUT in rag.py
Verified live: curl /rag/search?q=sandbox+overlay returns 8 hits (~12s)
B. Tests — added
tests/test_rag_routes.py — empty/short query, hit rendering, debounce attrs
tests/test_rag_inject.py — threshold, prepend, disabled, fail-open, config
26 tests pass (test_sandbox.py + test_rag*.py)
C. Index pipeline — script fixes
Concurrent docs-mcp server caused SQLITE_CONSTRAINT_FOREIGNKEY on --clean scrapes (exit code 0 but failed internally).

_docs_mcp_index.sh: stop server before scrape, detect sqlite errors in log, require ≥1 dep lib before stamping
_cont_start.sh: run index before respawning docs-mcp server
Re-index is running in background (DEP_MAX_DEPTH=1). Monitor with tail -f /tmp/rag-reindex.log. Expect .index-stamp + dep libs in docs_mcp_cli list when it finishes.

D. Part 5 — first-hop injection — implemented
File	Change
rag_inject.py
maybe_prepend_first_hop() — <rag-context> block, fail-open
settings.py
rag_config() + defaults (first_hop_min_score=0.02 calibrated to live scores ~0.03)
harness/rag.json
default config
chats.py
inject before run_exchange, every exchange hop 1
sub_agents/base.py
inject when hop_n == 1 only
E. Plan docs — updated
0_research_code_mcp_tools.md, 1_impl/deferred_demo_and_first_turn.md, and 2_left.md now reflect v1 locked decisions and landed code.

Not done (out of scope): D5 sigmap teardown, depth-4 indexing, depth slider, multi-library demo search (needs dep libs indexed first).

Next step for you: let the re-index finish, then confirm docs_mcp_cli list shows py-* / crate libs and .index-stamp exists. After that, a chat smoke test with a symbol only in a dep crate would validate injection end-to-end.

why is the mcp tool indexing our git ignored data ? it should ignore all the git ignore data. I see it indexing json files under data, but they are git ignored ! How can we fix this ? 

Why it happens: docs-mcp-server has no .gitignore support. It walks the filesystem from file:///workspace and only skips paths you pass via --exclude-pattern. The index script had a small hardcoded regex list (target, node_modules, .git, etc.) and did not include nested rules like:


.gitignore
Lines 1-3
*.bytes
json_decoded/
*.tmp
So json_decoded/ under _data/3d_data_v2/data_cache/ was scraped and embedded even though git ignores it.

Fix: Generate exclude globs from every tracked .gitignore and pass them to scrape.

File	Change
_docker/_docs_mcp_gitignore_patterns.py
new — reads all tracked .gitignore files, converts rules to minimatch globs docs-mcp understands
_docker/_docs_mcp_index.sh
passes ~54 --exclude-pattern args instead of one hardcoded regex; fingerprint includes gitignore hash so gitignore edits trigger re-index
Key patterns now emitted:

_data/3d_data_v2/data_cache/**/json_decoded/**
_data/3d_data_v2/data_cache/**/*.bytes
_data/3d_data_v2/data_out/**
**/.context/**
To apply: the existing index must be rebuilt (old embeddings stay until then):

# inside crack-dev — long run
rm -f /workspace/target/rag/.index-stamp
DEP_MAX_DEPTH=1 bash /workspace/_docker/_docs_mcp_index.sh
Or restart crack-dev so bringup re-indexes (stamp won't match after the fingerprint change).

Caveat: negation rules (!foo in .gitignore) aren't supported — docs-mcp only has excludes. That's fine for cache/generated dirs; the one case with !pedestrian/*.glb under _data/3d_data/3d_slop_models_clean/ may over-exclude if that tree is otherwise ignored. Say if you want include-pattern handling for that case.

The background re-index did not finish — it exited with code 137 (SIGKILL, usually OOM) after ~3.7 minutes, stuck on the repo scrape. There is still no .index-stamp, and only crack-repo is in the index.

To retry with the new gitignore excludes (and avoid indexing _data cache JSON):

docker exec crack-dev bash -c 'rm -f /workspace/target/rag/.index-stamp && DEP_MAX_DEPTH=1 bash /workspace/_docker/_docs_mcp_index.sh'
That run is long and memory-heavy; if it dies again at 137, give crack-dev more RAM or run it when the box is less loaded.