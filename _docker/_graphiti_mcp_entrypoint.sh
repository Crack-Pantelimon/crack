#!/bin/bash
# Entrypoint for zepai/knowledge-graph-mcp:standalone on crack-docker-net.
# Waits for Ollama models, applies Ollama/network patches, then starts MCP.
set -euo pipefail

OLLAMA_HOST="${OPENAI_API_URL:-http://ollama:11434/v1}"
OLLAMA_BASE="${OLLAMA_HOST%/v1}"
LLM_MODEL="${MODEL_NAME:-qwen3.5:0.8b}"
EMBED_MODEL="${GRAPHITI_EMBEDDING_MODEL:-all-minilm}"

wait_ollama() {
  echo "[graphiti-mcp] waiting for ollama at ${OLLAMA_BASE} ..."
  for _ in $(seq 1 60); do
    if curl -sf "${OLLAMA_BASE}/api/tags" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  echo "[graphiti-mcp] WARNING: ollama not reachable; continuing anyway" >&2
  return 1
}

ensure_model() {
  local model="$1"
  if python3 - "$OLLAMA_BASE" "$model" <<'PY'
import json, sys, urllib.request
base, want = sys.argv[1], sys.argv[2]
want_base = want.split(":", 1)[0]
try:
    with urllib.request.urlopen(f"{base}/api/tags", timeout=5) as r:
        tags = json.load(r).get("models", [])
except Exception:
    sys.exit(1)
for m in tags:
    name = m.get("name") or ""
    if name == want or name.split(":", 1)[0] == want_base:
        sys.exit(0)
sys.exit(1)
PY
  then
    echo "[graphiti-mcp] model present: ${model}"
    return 0
  fi
  echo "[graphiti-mcp] pulling model: ${model}"
  curl -sf "${OLLAMA_BASE}/api/pull" \
    -H 'Content-Type: application/json' \
    -d "{\"name\":\"${model}\"}" >/dev/null \
    || echo "[graphiti-mcp] WARNING: pull failed for ${model}" >&2
}

wait_ollama || true
ensure_model "${LLM_MODEL}" || true
ensure_model "${EMBED_MODEL}" || true

python3 /patch_ollama.py
exec uv run --no-sync main.py
