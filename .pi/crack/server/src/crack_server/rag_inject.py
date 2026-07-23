"""First-hop RAG injection: prepend retrieved docs to hop-1 agent input only.

Runs once per user exchange (and once per sub-agent run on hop 1). Fails open —
if claude-context is unavailable or scores are below threshold, the message is
unchanged. The injected block is ephemeral (not persisted as a user turn).
"""

from __future__ import annotations

import logging

from crack_server import rag
from crack_server.settings import rag_config

logger = logging.getLogger("uvicorn.error")

_CONTEXT_TAG = "rag-context"


async def maybe_prepend_first_hop(*, query: str, message: str) -> str:
    """Return ``message``, or ``message`` with a fenced RAG block prepended."""
    cfg = rag_config()
    if not cfg.get("first_hop_enabled", True):
        return message
    q = (query or "").strip()
    if not q:
        return message

    top_k = int(cfg.get("first_hop_top_k", 5))
    min_score = float(cfg.get("first_hop_min_score", 0.02))
    max_chars = int(cfg.get("first_hop_max_chars", 12000))

    hits = await rag.search_docs(q, limit=top_k)
    if not hits:
        return message

    kept = [h for h in hits if float(h.get("score", 0)) >= min_score]
    if not kept:
        logger.info(
            "rag_inject: %d hit(s) below threshold %.3f (top=%.3f)",
            len(hits),
            min_score,
            float(hits[0].get("score", 0)),
        )
        return message

    lines = [
        f"The following documentation excerpts may be relevant "
        f"(retrieval score ≥ {min_score:.2f}):",
        "",
    ]
    used = 0
    for h in kept:
        snippet = str(h.get("snippet") or "").strip()
        source = str(h.get("source") or "")
        score = float(h.get("score", 0))
        block = f"### {source} (score={score:.3f})\n{snippet}\n"
        if used and used + len(block) > max_chars:
            break
        lines.append(block)
        used += len(block)

    if used == 0:
        return message

    block = f"<{_CONTEXT_TAG}>\n" + "\n".join(lines).rstrip() + f"\n</{_CONTEXT_TAG}>\n\n"
    return block + message
