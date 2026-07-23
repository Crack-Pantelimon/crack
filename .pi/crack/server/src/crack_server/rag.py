"""Shared access to the self-hosted RAG index (claude-context + Milvus).

Both the ``/rag`` debug page (``routes_rag``) and first-turn injection
(``rag_inject``) retrieve through :func:`search_docs`, which shells out to the
claude-context search CLI against the shared Milvus collection — the SAME index
the stdio ``code-search`` MCP tool queries.

Everything fails **open**: if claude-context isn't built, Milvus/Ollama is down,
or the query is empty, we return ``[]`` and let the caller proceed without RAG.
Retrieval is never load-bearing.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
from pathlib import Path

logger = logging.getLogger("uvicorn.error")

# Mirror _docker/_claude_context_setup.sh. Overridable for tests / host dev.
CLAUDE_CONTEXT_DIR = os.environ.get(
    "CLAUDE_CONTEXT_DIR", "/crack-harness-data/tools/claude-context"
)
CODEBASE_PATH = os.environ.get("CODEBASE_PATH", "/workspace")
CODEBASE_LABEL = os.environ.get("RAG_CODEBASE_LABEL", "crack-repo")
MILVUS_ADDRESS = os.environ.get("MILVUS_ADDRESS", "milvus-standalone:19530")
OLLAMA_HOST = os.environ.get("OLLAMA_HOST", "http://ollama:11434")
# Embedding model for indexing + query. Kept small/fast (all-minilm, 384-dim).
# For higher retrieval quality switch BOTH the model and dimension together and
# re-index (the dim is baked into the Milvus collection):
#   nomic-embed-text  -> EMBEDDING_DIMENSION=768   (stronger, ~137M)
#   mxbai-embed-large -> EMBEDDING_DIMENSION=1024  (strongest common ollama embed)
# `ollama pull <model>` happens automatically at boot (claude_context_ensure_embed_model).
EMBEDDING_MODEL = os.environ.get("EMBEDDING_MODEL", "all-minilm")
EMBEDDING_DIMENSION = os.environ.get("EMBEDDING_DIMENSION", "384")
# Ollama embedding over a large index routinely exceeds 20s; keep fail-open.
SEARCH_TIMEOUT = float(os.environ.get("RAG_SEARCH_TIMEOUT", "120"))


def _claude_context_env() -> dict[str, str]:
    env = dict(os.environ)
    env.update(
        {
            "MILVUS_ADDRESS": MILVUS_ADDRESS,
            "EMBEDDING_PROVIDER": "Ollama",
            "OLLAMA_HOST": OLLAMA_HOST,
            "EMBEDDING_MODEL": EMBEDDING_MODEL,
            "EMBEDDING_DIMENSION": EMBEDDING_DIMENSION,
            "CODEBASE_PATH": CODEBASE_PATH,
            "NODE_ENV": "production",
            "CUSTOM_IGNORE_PATTERNS": os.environ.get(
                "CUSTOM_IGNORE_PATTERNS",
                "target/**,node_modules/**,_slop/**,.playwright-mcp/**,"
                "venv/**,**/.venv/**,**/site-packages/**",
            ),
        }
    )
    return env


def search_script_path() -> Path:
    return Path(CLAUDE_CONTEXT_DIR) / "search.mjs"


def available() -> bool:
    """True when the claude-context search script is present."""
    return search_script_path().is_file()


def normalize_hit(hit: dict) -> dict:
    """Map a claude-context result to ``{score, source, snippet}``."""
    rel = str(hit.get("relativePath") or hit.get("source") or hit.get("url") or "")
    start = hit.get("startLine")
    end = hit.get("endLine")
    if rel and start is not None and end is not None:
        source = f"{rel}:{start}-{end}"
    else:
        source = rel
    score = hit.get("score")
    return {
        "score": float(score) if isinstance(score, (int, float)) else 0.0,
        "source": source,
        "snippet": str(hit.get("content") or hit.get("text") or "").strip(),
    }


async def search_docs(
    query: str,
    *,
    limit: int = 8,
    library: str | None = None,
    timeout: float | None = None,
) -> list[dict]:
    """Return up to ``limit`` normalized hits for ``query`` (``[]`` on any failure).

    Shells out to ``node search.mjs <query> --limit <n>`` with the Ollama/Milvus
    env. The ``library`` kwarg is accepted for call-site compatibility but
    ignored (claude-context searches ``CODEBASE_PATH``). Never raises.
    """
    _ = library  # docs-mcp library id; unused with claude-context
    query = (query or "").strip()
    if not query:
        return []
    script = search_script_path()
    if not script.is_file():
        logger.warning("rag: claude-context not built at %s — skipping search", script)
        return []
    if timeout is None:
        timeout = SEARCH_TIMEOUT
    try:
        proc = await asyncio.create_subprocess_exec(
            "node",
            str(script),
            query,
            "--limit",
            str(limit),
            env=_claude_context_env(),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
    except OSError as e:
        logger.warning("rag: could not launch claude-context search: %s", e)
        return []
    try:
        out, err = await asyncio.wait_for(proc.communicate(), timeout=timeout)
    except asyncio.TimeoutError:
        proc.kill()
        await proc.wait()
        logger.warning("rag: search timed out after %ss (query=%r)", timeout, query[:80])
        return []
    if proc.returncode != 0:
        logger.warning(
            "rag: search rc=%s (query=%r): %s",
            proc.returncode,
            query[:80],
            (err.decode(errors="replace") or "").strip()[:300],
        )
        return []
    try:
        data = json.loads(out.decode(errors="replace") or "[]")
    except json.JSONDecodeError:
        logger.warning("rag: non-JSON search output (query=%r)", query[:80])
        return []
    if not isinstance(data, list):
        return []
    return [normalize_hit(h) for h in data if isinstance(h, dict)]
