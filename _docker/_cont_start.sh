#!/bin/bash

set -ex

cd /workspace/.pi/crack/server
uv sync

source /workspace/_docker/_sandbox_common.sh

# crack-dev binds uvicorn on all interfaces; sandboxes set CRACK_PI_HOST=crack-dev via podman -e.
export CRACK_PI_HOST=0.0.0.0

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
#   blender     : http://<host>:9877/mcp   (Streamable HTTP, native)
export MCP_FIREFOX_PORT=9930
export MCP_CHROMIUM_PORT=9931
export MCP_WEBSEARCH_PORT=9932
mkdir -p "$CRACK_HARNESS_DATA_DIR/harness/mcp-http"

respawn() {  # respawn <logname> <cmd...>
    local name="$1"; shift
    ( while true; do
        "$@" >>"$CRACK_HARNESS_DATA_DIR/harness/mcp-http/${name}.log" 2>&1 || true
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

# --- Blender MCP (ports 9876 addon socket, 9877 HTTP) ---------------------
# Blender addon runs inside Blender (Xvfb) on TCP 9876; blender-mcp serves HTTP on 9877.
export MCP_BLENDER_HTTP_PORT=9877
# Force X11 backend, disable Wayland
export QT_QPA_PLATFORM=offscreen
export WAYLAND_DISPLAY=""
export DISPLAY=:99

# Start Xvfb on display :99 (one display for all Blender respawns — not xvfb-run per launch)
Xvfb :99 -screen 0 1920x1080x24 -ac +extension GLX +extension RANDR +extension RENDER >"$CRACK_HARNESS_DATA_DIR/harness/mcp-http/xvfb.log" 2>&1 &

sleep 2

respawn blender \
    blender --noaudio --addons blendermcp

sleep 5

respawn blender-mcp \
    python3 -c "from blender_mcp.server import mcp; mcp.settings.host='0.0.0.0'; mcp.settings.port=${MCP_BLENDER_HTTP_PORT}; mcp.settings.stateless_http=True; mcp.settings.json_response=True; mcp.run(transport='streamable-http')"
# --------------------------------------------------------------------------

# Single process: the queue worker runs inside the server (uvicorn app lifespan,
# in-process asyncio tasks — see crack_server/worker.py).
uv run crack-server