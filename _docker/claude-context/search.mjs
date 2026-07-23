#!/usr/bin/env node
import {
  Context,
  MilvusVectorDatabase,
  OllamaEmbedding,
} from "@zilliz/claude-context-core";

// claude-context-core logs verbose progress via console.log (stdout). This CLI
// must emit ONLY the JSON result array on stdout (rag.py json.loads() the whole
// thing), so route all library noise to stderr and keep stdout pristine.
console.log = (...a) => console.error(...a);

const args = process.argv.slice(2);
let query = "";
let limit = 8;
const codebasePath = process.env.CODEBASE_PATH || "/workspace";
// semanticSearch's default threshold is 0.5, but hybrid RRF scores are tiny
// (~0.01-0.03) so that would drop every hit. Keep 0 and let rag_inject's
// first_hop_min_score do the downstream filtering. Override via RAG_SEARCH_THRESHOLD.
const threshold = parseFloat(process.env.RAG_SEARCH_THRESHOLD || "0");

for (let i = 0; i < args.length; i++) {
  if (args[i] === "--limit" && args[i + 1]) {
    limit = parseInt(args[++i], 10);
  } else if (!args[i].startsWith("-")) {
    query = args[i];
  }
}

if (!query.trim()) {
  process.stdout.write("[]");
  process.exit(0);
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

try {
  const context = buildContext();
  const results = await context.semanticSearch(
    codebasePath,
    query,
    limit,
    Number.isFinite(threshold) ? threshold : 0,
  );
  // process.stdout.write, NOT console.log (redirected to stderr above): the JSON
  // array must be the only thing on stdout for rag.py to parse it.
  process.stdout.write(JSON.stringify(results));
} catch (err) {
  console.error("[claude-context] search failed:", err);
  process.stdout.write("[]");
  process.exit(0);
}
