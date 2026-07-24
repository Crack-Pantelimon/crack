# Graphiti feature prompts

These prompts assume the local `ollama`, `falkordb`, and `falkordb-browser`
services are running and that Graphiti telemetry is disabled.

Models (Ollama local LLM guide + RAG-matching embedder):

- LLM: `qwen3.5:4b` via `OpenAIGenericClient` → `http://ollama:11434/v1`
  (requests send `reasoning_effort: none` — qwen3.5 thinking is off)
- Embeddings: `all-minilm` (384-dim, same as RAG / code-search), not `nomic-embed-text`
- Group id: `crack_repo` (no hyphens — FalkorDB RediSearch breaks on `-` in `@group_id`)

## Seed a small graph

Add three episodes to the `crack_repo` group:

1. “The crack server is a FastAPI application in `.pi/crack/server`.”
2. “The RAG page uses Milvus and Ollama embeddings.”
3. “Graph Search uses Graphiti with FalkorDB and Ollama.”

Use stable episode names, then search for “Which components provide search?”
List the returned facts, source and target entities, and explain the path
between RAG and Graph Search.

## Temporal updates

Add an episode saying that Graphiti initially used a temporary SQLite store,
then add a later episode saying the production store is FalkorDB. Search for
the storage history and report which fact is current and which is historical.

## Entity exploration

Search for “Ollama”. For every returned relationship, expand the source and
target entities and produce a two-hop subgraph. Do not invent relationships
that are not present in the graph.

## Retrieval comparison

Ask the same question through `/rag` and `/graph`: “Where is the embedding
configuration defined?” Compare lexical/vector snippets with graph facts.
Identify one question where graph traversal is more useful and one where RAG
snippets are more useful.

## MCP smoke test

Using the Graphiti MCP tools, create one episode about this repository, search
for its distinctive phrase, inspect the returned entities, and remove or
isolate the test data if the server supports deletion. Confirm that telemetry
remains disabled.
read-error: ENOENT: ENOENT: no such file or directory