#!/bin/bash
# Shared claude-context bootstrap helpers (crack-dev boot indexer + search CLI).
# Sourced by _cont_start.sh and _claude_context_index.sh.

CLAUDE_CONTEXT_VERSION="${CLAUDE_CONTEXT_VERSION:-0.1.15}"
CLAUDE_CONTEXT_DIR="${CLAUDE_CONTEXT_DIR:-${CRACK_HARNESS_DATA_DIR}/tools/claude-context}"
CLAUDE_CONTEXT_TEMPLATES="${CLAUDE_CONTEXT_TEMPLATES:-/workspace/_docker/claude-context}"

export MILVUS_ADDRESS="${MILVUS_ADDRESS:-milvus-standalone:19530}"
export EMBEDDING_PROVIDER="${EMBEDDING_PROVIDER:-Ollama}"
export OLLAMA_HOST="${OLLAMA_HOST:-http://ollama:11434}"
# Embedding model for indexing + query. Kept small/fast (all-minilm, 384-dim).
# For higher retrieval quality switch BOTH the model and dimension together and
# re-index (the dim is baked into the Milvus collection):
#   nomic-embed-text  -> EMBEDDING_DIMENSION=768   (stronger, ~137M)
#   mxbai-embed-large -> EMBEDDING_DIMENSION=1024  (strongest common ollama embed)
# `ollama pull <model>` happens automatically at boot (claude_context_ensure_embed_model).
export EMBEDDING_MODEL="${EMBEDDING_MODEL:-all-minilm}"
export EMBEDDING_DIMENSION="${EMBEDDING_DIMENSION:-384}"
export CODEBASE_PATH="${CODEBASE_PATH:-/workspace}"
export NODE_ENV="${NODE_ENV:-production}"
# claude-context honors tracked .gitignore files, but the repo-root venv/ is NOT
# gitignored, so exclude it (and any site-packages) here or the index fills with
# vendored third-party code (pip's urllib3, bs4, ...) instead of repo code.
export CUSTOM_IGNORE_PATTERNS="${CUSTOM_IGNORE_PATTERNS:-target/**,node_modules/**,_slop/**,.playwright-mcp/**,venv/**,**/.venv/**,**/site-packages/**}"

claude_context_env() {
    export MILVUS_ADDRESS EMBEDDING_PROVIDER OLLAMA_HOST EMBEDDING_MODEL
    export EMBEDDING_DIMENSION CODEBASE_PATH NODE_ENV CUSTOM_IGNORE_PATTERNS
}

claude_context_ensure_built() {
    mkdir -p "$CLAUDE_CONTEXT_DIR"
    cp -f "$CLAUDE_CONTEXT_TEMPLATES/package.json" "$CLAUDE_CONTEXT_DIR/package.json"
    cp -f "$CLAUDE_CONTEXT_TEMPLATES/index.mjs" "$CLAUDE_CONTEXT_DIR/index.mjs"
    cp -f "$CLAUDE_CONTEXT_TEMPLATES/search.mjs" "$CLAUDE_CONTEXT_DIR/search.mjs"
    if [ ! -d "$CLAUDE_CONTEXT_DIR/node_modules/@zilliz/claude-context-core" ] || \
       [ "$(cat "$CLAUDE_CONTEXT_DIR/.built-at" 2>/dev/null || true)" != "$CLAUDE_CONTEXT_VERSION" ]; then
        echo "[claude-context] npm install @zilliz/claude-context-core@${CLAUDE_CONTEXT_VERSION}"
        (
            cd "$CLAUDE_CONTEXT_DIR" &&
            npm install --omit=dev
        )
        echo "$CLAUDE_CONTEXT_VERSION" >"$CLAUDE_CONTEXT_DIR/.built-at"
    fi
}

# Poll Milvus standalone health (9091) until ready or timeout.
claude_context_wait_milvus() {
    local deadline=$(( SECONDS + ${1:-180} ))
    local host="${MILVUS_ADDRESS%%:*}"
    while (( SECONDS < deadline )); do
        if curl -sf "http://${host}:9091/healthz" >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
    done
    echo "[claude-context] milvus not reachable at ${host}:9091" >&2
    return 1
}

# Block until the Ollama HTTP API answers.
claude_context_wait_ollama() {
    local deadline=$(( SECONDS + ${1:-120} ))
    while (( SECONDS < deadline )); do
        if curl -sf "${OLLAMA_HOST}/api/version" >/dev/null 2>&1; then
            return 0
        fi
        sleep 2
    done
    echo "[claude-context] ollama not reachable at ${OLLAMA_HOST}" >&2
    return 1
}

claude_context_ensure_embed_model() {
    claude_context_wait_ollama || return 1
    if ! curl -sf "${OLLAMA_HOST}/api/pull" \
            -d "{\"name\":\"${EMBEDDING_MODEL}\",\"stream\":false}" \
            >/dev/null; then
        echo "[claude-context] failed to pull ${EMBEDDING_MODEL} from ollama" >&2
        return 1
    fi
}
