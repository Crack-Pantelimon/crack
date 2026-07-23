#!/usr/bin/env node
import {
  Context,
  MilvusVectorDatabase,
  OllamaEmbedding,
} from "@zilliz/claude-context-core";

const codebasePath = process.argv[2];
if (!codebasePath) {
  console.error("usage: node index.mjs <codebasePath>");
  process.exit(1);
}

function buildContext() {
  const milvusAddress =
    process.env.MILVUS_ADDRESS || "milvus-standalone:19530";
  // Embedding model for indexing + query. Kept small/fast (all-minilm, 384-dim).
  // For higher retrieval quality switch BOTH the model and dimension together and
  // re-index (the dim is baked into the Milvus collection):
  //   nomic-embed-text  -> EMBEDDING_DIMENSION=768   (stronger, ~137M)
  //   mxbai-embed-large -> EMBEDDING_DIMENSION=1024  (strongest common ollama embed)
  // `ollama pull <model>` happens automatically at boot (claude_context_ensure_embed_model).
  const embeddingModel = process.env.EMBEDDING_MODEL || "all-minilm";
  const ollamaHost = process.env.OLLAMA_HOST || "http://ollama:11434";
  const embeddingDimension = parseInt(
    process.env.EMBEDDING_DIMENSION || "384",
    10,
  );
  const customIgnore = (process.env.CUSTOM_IGNORE_PATTERNS || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

  const vectorDatabase = new MilvusVectorDatabase({ address: milvusAddress });
  const embedding = new OllamaEmbedding({
    model: embeddingModel,
    host: ollamaHost,
    dimension: embeddingDimension,
  });

  return new Context({
    embedding,
    vectorDatabase,
    customIgnorePatterns: customIgnore,
  });
}

const onProgress = (progress) => {
  console.error(
    `[claude-context] ${progress.phase} ${progress.percentage}% (${progress.current}/${progress.total})`,
  );
};

try {
  const context = buildContext();
  const hasIndex = await context.hasIndex(codebasePath);
  const stats = hasIndex
    ? await context.reindexByChange(codebasePath, onProgress)
    : await context.indexCodebase(codebasePath, onProgress);
  console.error("[claude-context] index done", JSON.stringify(stats));
} catch (err) {
  console.error("[claude-context] index failed:", err);
  process.exit(1);
}
