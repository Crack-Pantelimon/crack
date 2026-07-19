#!/bin/bash

set -ex


cd /workspace/.pi/crack/server
uv sync

# --- Shared environment for every child process ---------------------------
# This script is the container entrypoint, so every export below is inherited
# by the worker, the web server, and (transitively) the `pi` subprocesses and
# MCP servers they spawn. Keep tool/browser paths here so nothing has to guess.
export CRACK_PI_PROJECT_ROOT=/workspace
export HOME=/root
# Toolchains (also set as Docker ENV, re-exported here so a `docker exec` or a
# child with a scrubbed env still finds them): cargo/wasm-pack, uv python, node.
export PATH="/usr/local/cargo/bin:/usr/local/python/bin:/usr/local/bin:/usr/bin:/bin:$PATH"
# Browsers + WebDriver, so any tool (MCP or CLI) resolves them without probing.
export CHROME_BIN=/usr/bin/chromium
export CHROME_PATH=/usr/bin/chromium
export CHROMIUM_PATH=/usr/bin/chromium
export FIREFOX_BIN=/usr/bin/firefox-esr
export CHROMEDRIVER_BIN=/usr/bin/chromedriver
export GECKODRIVER_BIN=/usr/local/bin/geckodriver
export PLAYWRIGHT_BROWSERS_PATH=/root/.cache/ms-playwright
# Running headless as root: no X display, and chromium needs --no-sandbox.
unset DISPLAY
export CHROMIUM_FLAGS="--no-sandbox --disable-gpu"
# --------------------------------------------------------------------------

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

# --- Network-reachable MCP endpoints (for host/outside users) --------------
# The in-container `pi` agents talk to the MCP servers over stdio (mcp.json
# above). To also let a host user reach the SAME servers over localhost, expose
# each one over HTTP on a fixed port (published in run.sh). playwright/mcp
# serves HTTP natively (Streamable HTTP at /mcp); chrome-devtools-mcp and
# web-search are stdio-only, so supergateway bridges them (stdio -> SSE at /sse).
# supergateway binds IPv6 :: only, which Docker's IPv4 proxy can't reach, so it
# runs on an internal loopback port fronted by tcp_forward.py on 0.0.0.0. Each
# piece runs under a respawn loop so a crash self-heals without downing the box.
#   firefox     : http://<host>:9930/mcp   (Streamable HTTP)
#   chromium    : http://<host>:9931/sse   (SSE)
#   web-search  : http://<host>:9932/sse   (SSE)
export MCP_FIREFOX_PORT=9930
export MCP_CHROMIUM_PORT=9931
export MCP_WEBSEARCH_PORT=9932
mkdir -p /workspace/.pi/crack/harness/mcp-http

respawn() {  # respawn <logname> <cmd...>
    local name="$1"; shift
    ( while true; do
        "$@" >>"/workspace/.pi/crack/harness/mcp-http/${name}.log" 2>&1 || true
        echo "[mcp-http] ${name} exited; respawning in 3s" >&2
        sleep 3
      done ) &
}

respawn firefox \
    npx -y @playwright/mcp@0.0.78 --browser firefox --headless --isolated \
        --allow-unrestricted-file-access --output-dir /workspace/.playwright-mcp \
        --host 0.0.0.0 --allowed-hosts "*" --port "${MCP_FIREFOX_PORT}"

respawn chromium \
    npx -y supergateway --cors --port "$((MCP_CHROMIUM_PORT + 10000))" \
        --stdio "npx -y chrome-devtools-mcp@1.6.0 --headless --isolated --executablePath /usr/bin/chromium --chromeArg=--no-sandbox --chromeArg=--disable-gpu --no-usage-statistics --allow-unrestricted-paths"
respawn chromium-fwd \
    python3 /workspace/_docker/tcp_forward.py "${MCP_CHROMIUM_PORT}" 127.0.0.1 "$((MCP_CHROMIUM_PORT + 10000))"

export BROWSER_HEADLESS=true  # web-search-mcp's playwright fallback runs headless
respawn web-search \
    npx -y supergateway --cors --port "$((MCP_WEBSEARCH_PORT + 10000))" \
        --stdio "node /root/web-search-mcp/dist/index.js"
respawn web-search-fwd \
    python3 /workspace/_docker/tcp_forward.py "${MCP_WEBSEARCH_PORT}" 127.0.0.1 "$((MCP_WEBSEARCH_PORT + 10000))"
# --------------------------------------------------------------------------

# Single-instance, auto-refreshing worker: if the flock is already held, exit 0.
( flock -n /workspace/.pi/crack/harness/worker.lock uv run crack-worker || true ) &
uv run crack-server