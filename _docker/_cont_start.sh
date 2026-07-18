#!/bin/bash

set -ex


cd /workspace/.pi/crack/server
uv sync
export CRACK_PI_PROJECT_ROOT=/workspace
mkdir -p /workspace/.pi/crack/harness
# MCP servers (web-search + browsers) for pi agents: the pi-mcp-adapter resolves
# .mcp.json as <cwd>/.mcp.json (no upward walk), but worker-spawned agents run
# with cwd=/workspace/.pi/crack/server — so sync the repo copy into the global
# config the adapter reads regardless of cwd (see _docker/README.md).
mkdir -p /root/.config/mcp
cp /workspace/.mcp.json /root/.config/mcp/mcp.json
# web-search-mcp is a stdio server launched lazily by the adapter; sanity-check the build.
[ -f /root/web-search-mcp/dist/index.js ] || \
    echo "WARNING: web-search-mcp not built at /root/web-search-mcp (see _docker/README.md)" >&2
# Single-instance, auto-refreshing worker: if the flock is already held, exit 0.
( flock -n /workspace/.pi/crack/harness/worker.lock uv run crack-worker || true ) &
uv run crack-server