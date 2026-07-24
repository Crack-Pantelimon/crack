#!/bin/bash
# Entrypoint for zepai/knowledge-graph-mcp:standalone on crack-docker-net.
# Applies Ollama/network patches then starts the upstream MCP server.
set -euo pipefail
python3 /patch_ollama.py
exec uv run --no-sync main.py
