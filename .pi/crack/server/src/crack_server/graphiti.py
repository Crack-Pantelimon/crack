"""Small, fail-open Graphiti integration for the local FalkorDB graph.

Configured per Graphiti's "Ollama (Local LLMs)" guide:
https://help.getzep.com/graphiti/configuration/llm-configuration

Uses ``OpenAIGenericClient`` (chat/completions + response_format) because
Ollama does not implement ``/v1/responses``. Embeddings reuse the same small
``all-minilm`` model as the RAG indexer (384-dim) instead of nomic-embed-text.

Graphiti is initialized lazily so the rest of the server remains usable while
FalkorDB, Ollama, or the optional Poetry dependency is unavailable.
"""

from __future__ import annotations

import inspect
import logging
import os
from typing import Any

logger = logging.getLogger("uvicorn.error")

FALKORDB_HOST = os.environ.get("FALKORDB_HOST", "falkordb")
FALKORDB_PORT = int(os.environ.get("FALKORDB_PORT", "6379"))
FALKORDB_DATABASE = os.environ.get("FALKORDB_DATABASE", "graphiti")
# Tutorial model: deepseek-r1:7b. Override via GRAPHITI_LLM_MODEL if needed.
GRAPHITI_LLM_MODEL = os.environ.get("GRAPHITI_LLM_MODEL", "deepseek-r1:7b")
# Match RAG / code-search: all-minilm @ 384 dims (45 MB), not nomic-embed-text.
GRAPHITI_EMBEDDING_MODEL = os.environ.get(
    "GRAPHITI_EMBEDDING_MODEL", "all-minilm"
)
GRAPHITI_EMBEDDING_DIMENSION = int(
    os.environ.get("GRAPHITI_EMBEDDING_DIMENSION", "384")
)
OLLAMA_HOST = os.environ.get("OLLAMA_HOST", "http://ollama:11434")
SEARCH_LIMIT = int(os.environ.get("GRAPHITI_SEARCH_LIMIT", "20"))

# Graphiti's telemetry is opt-out. Set all known switches explicitly so a
# package upgrade cannot silently turn telemetry back on in this deployment.
os.environ.setdefault("GRAPHITI_TELEMETRY_ENABLED", "false")
os.environ.setdefault("TELEMETRY_ENABLED", "false")
os.environ.setdefault("DO_NOT_TRACK", "1")
# OpenAI SDK / Graphiti reranker still peek at these even when config.api_key
# is passed explicitly — keep them pointed at local Ollama.
os.environ.setdefault("OPENAI_API_KEY", "ollama")
os.environ.setdefault("OPENAI_BASE_URL", f"{OLLAMA_HOST.rstrip('/')}/v1")

_graph: Any | None = None
_init_error: str | None = None


def _build_graph() -> Any:
    from graphiti_core import Graphiti
    from graphiti_core.cross_encoder.openai_reranker_client import OpenAIRerankerClient
    from graphiti_core.driver.falkordb_driver import FalkorDriver
    from graphiti_core.embedder.openai import OpenAIEmbedder, OpenAIEmbedderConfig
    from graphiti_core.llm_client.config import LLMConfig
    from graphiti_core.llm_client.openai_generic_client import OpenAIGenericClient

    driver = FalkorDriver(
        host=FALKORDB_HOST,
        port=FALKORDB_PORT,
        database=FALKORDB_DATABASE,
    )
    ollama_api = f"{OLLAMA_HOST.rstrip('/')}/v1"
    llm_config = LLMConfig(
        api_key="ollama",
        base_url=ollama_api,
        model=GRAPHITI_LLM_MODEL,
        small_model=GRAPHITI_LLM_MODEL,
        temperature=0.0,
    )
    # OpenAIGenericClient → /v1/chat/completions (Ollama-compatible).
    # OpenAIClient would hit /v1/responses, which Ollama does not implement.
    llm_client = OpenAIGenericClient(config=llm_config)
    embedder = OpenAIEmbedder(
        config=OpenAIEmbedderConfig(
            api_key="ollama",
            base_url=ollama_api,
            embedding_model=GRAPHITI_EMBEDDING_MODEL,
            embedding_dim=GRAPHITI_EMBEDDING_DIMENSION,
        )
    )
    return Graphiti(
        graph_driver=driver,
        llm_client=llm_client,
        embedder=embedder,
        cross_encoder=OpenAIRerankerClient(client=llm_client, config=llm_config),
    )


def get_graph() -> Any | None:
    """Return the singleton Graphiti client, or ``None`` when unavailable."""
    global _graph, _init_error
    if _graph is not None:
        return _graph
    try:
        _graph = _build_graph()
        _init_error = None
    except Exception as exc:  # Graph search is deliberately fail-open.
        _init_error = str(exc)
        logger.warning("graphiti unavailable: %s", exc)
        return None
    return _graph


def status() -> dict[str, Any]:
    return {
        "available": get_graph() is not None,
        "telemetry": False,
        "error": _init_error,
        "database": FALKORDB_DATABASE,
        "llm_model": GRAPHITI_LLM_MODEL,
        "embedding_model": GRAPHITI_EMBEDDING_MODEL,
        "embedding_dim": GRAPHITI_EMBEDDING_DIMENSION,
    }


def _value(obj: Any, *names: str) -> Any:
    if isinstance(obj, dict):
        for name in names:
            if name in obj:
                return obj[name]
    for name in names:
        value = getattr(obj, name, None)
        if value is not None:
            return value
    return None


def _node(node: Any) -> dict[str, Any]:
    return {
        "id": str(_value(node, "uuid", "id") or ""),
        "label": str(_value(node, "name", "label") or "Unknown"),
        "summary": str(_value(node, "summary", "description") or ""),
        "type": "node",
    }


def _edge(edge: Any) -> dict[str, Any]:
    source = _value(edge, "source_node_uuid", "source", "source_id")
    target = _value(edge, "target_node_uuid", "target", "target_id")
    return {
        "id": str(_value(edge, "uuid", "id") or f"{source}:{target}"),
        "source": str(source or ""),
        "target": str(target or ""),
        "label": str(_value(edge, "name", "fact", "relation_type") or "related"),
        "fact": str(_value(edge, "fact", "summary") or ""),
        "type": "edge",
    }


async def search(query: str, limit: int = SEARCH_LIMIT) -> dict[str, Any]:
    graph = get_graph()
    if graph is None or not query.strip():
        return {"nodes": [], "edges": [], "status": status()}
    try:
        results = await graph.search(query=query.strip(), num_results=limit)
        edges = [_edge(item) for item in results]
        nodes: dict[str, dict[str, Any]] = {}
        for edge in edges:
            for key in ("source", "target"):
                node_id = edge[key]
                if node_id:
                    nodes.setdefault(node_id, {"id": node_id, "label": node_id, "type": "node"})
        return {"nodes": list(nodes.values()), "edges": edges, "status": status()}
    except Exception as exc:
        logger.warning("graphiti search failed: %s", exc)
        return {"nodes": [], "edges": [], "status": {**status(), "error": str(exc)}}


async def expand(node_id: str, limit: int = SEARCH_LIMIT) -> dict[str, Any]:
    graph = get_graph()
    if graph is None or not node_id.strip():
        return {"nodes": [], "edges": [], "status": status()}
    try:
        query = (
            "MATCH (n)-[r]-(m) WHERE n.uuid = $uuid "
            "RETURN n, r, m LIMIT $limit"
        )
        result = graph.driver.execute_query(query, uuid=node_id, limit=limit)
        if inspect.isawaitable(result):
            result = await result
        records = result[0] if isinstance(result, tuple) else result
        nodes: dict[str, dict[str, Any]] = {}
        edges = []
        for record in records or []:
            n = _value(record, "n") or (record[0] if isinstance(record, (list, tuple)) else None)
            r = _value(record, "r") or (record[1] if isinstance(record, (list, tuple)) else None)
            m = _value(record, "m") or (record[2] if isinstance(record, (list, tuple)) else None)
            for item in (n, m):
                if item:
                    parsed = _node(item)
                    if parsed["id"]:
                        nodes[parsed["id"]] = parsed
            if r:
                edges.append(_edge(r))
        return {"nodes": list(nodes.values()), "edges": edges, "status": status()}
    except Exception as exc:
        logger.warning("graphiti expansion failed: %s", exc)
        return {"nodes": [], "edges": [], "status": {**status(), "error": str(exc)}}
