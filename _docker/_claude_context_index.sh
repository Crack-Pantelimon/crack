#!/bin/bash
# One-shot (cached) index of /workspace via claude-context → Milvus.
# claude-context is gitignore-aware; CUSTOM_IGNORE_PATTERNS covers extra dirs.
set -uo pipefail

source /workspace/_docker/_claude_context_setup.sh

STAMP="$CLAUDE_CONTEXT_DIR/.index-stamp"
mkdir -p "$CLAUDE_CONTEXT_DIR"

index_fingerprint() {
    {
        git -C /workspace rev-parse HEAD 2>/dev/null || echo no-git
        sha256sum /workspace/Cargo.lock 2>/dev/null || true
        sha256sum /workspace/.pi/crack/server/poetry.lock 2>/dev/null || true
        echo "claude-context@${CLAUDE_CONTEXT_VERSION}"
        echo "ignore=${CUSTOM_IGNORE_PATTERNS}"
        # Model+dim are baked into the Milvus collection; must reindex on change.
        echo "embed=${EMBEDDING_MODEL}:${EMBEDDING_DIMENSION}"
    } | sha256sum | awk '{print $1}'
}

FP="$(index_fingerprint)"
if [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$FP" ]; then
    echo "[claude-context] index stamp matches ($FP) — skipping"
    exit 0
fi

if ! claude_context_wait_milvus; then
    echo "[claude-context] milvus unavailable — skipping index (will retry next boot)" >&2
    exit 1
fi

if ! claude_context_ensure_embed_model; then
    echo "[claude-context] embeddings unavailable — skipping index (will retry next boot)" >&2
    exit 1
fi

claude_context_ensure_built
claude_context_env

echo "[claude-context] indexing ${CODEBASE_PATH}"
if ! node "$CLAUDE_CONTEXT_DIR/index.mjs" "$CODEBASE_PATH"; then
    echo "[claude-context] index failed — NOT stamping (will retry next boot)" >&2
    exit 1
fi

echo "$FP" >"$STAMP"
echo "[claude-context] index complete ($FP)"
