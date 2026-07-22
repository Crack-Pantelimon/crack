# Shared cheap setup for crack-dev and sandboxes: env exports, MCP config, Blender addon sync.
# Sourced by _cont_start.sh (then eager HTTP MCP bridges) and _sandbox_start.sh (then sleep).

# --- Shared environment for every child process ---------------------------
export CRACK_PI_PROJECT_ROOT=/workspace
export CRACK_HARNESS_DATA_DIR=/crack-harness-data
mkdir -p "$CRACK_HARNESS_DATA_DIR/harness" "$CRACK_HARNESS_DATA_DIR/unscripted_chats"
# One-time migration: if legacy in-repo state exists and the volume is empty, move it.
LEGACY=/workspace/.pi/crack
if [ -d "$LEGACY/unscripted_chats" ] && [ -z "$(ls -A "$CRACK_HARNESS_DATA_DIR/unscripted_chats" 2>/dev/null)" ]; then
    echo "[migrate] copying legacy harness state onto crack-harness-data volume"
    cp -a "$LEGACY/harness/." "$CRACK_HARNESS_DATA_DIR/harness/" 2>/dev/null || true
    cp -a "$LEGACY/unscripted_chats/." "$CRACK_HARNESS_DATA_DIR/unscripted_chats/" 2>/dev/null || true
fi
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

# MCP servers (web-search + browsers) for pi agents: the pi-mcp-adapter resolves
# .mcp.json as <cwd>/.mcp.json (no upward walk), but worker-spawned agents run
# with cwd=/workspace/.pi/crack/server — so sync the repo copy into the global
# config the adapter reads regardless of cwd (see _docker/README.md).
mkdir -p /root/.config/mcp
cp /workspace/.mcp.json /root/.config/mcp/mcp.json
# web-search-mcp is a stdio server launched lazily by the adapter; sanity-check the build.
[ -f /root/web-search-mcp/dist/index.js ] || \
    echo "WARNING: web-search-mcp not built at /root/web-search-mcp (see _docker/README.md)" >&2

# --- Blender addon sync (no Xvfb/Blender here — lazy via _blender_mcp_lazy.sh) ---
export BLENDER_ADDON_PORT=9876
export BLENDER_HOST=127.0.0.1
export BLENDER_PORT=9876
export DISABLE_TELEMETRY=true
# /root is a named volume — sync addon from image path every boot (like mcp.json above).
BLENDER_ADDON_DIR="/root/.config/blender/5.1/scripts/addons"
mkdir -p "${BLENDER_ADDON_DIR}"
rm -f "${BLENDER_ADDON_DIR}/blender_mcp.py"
cp /opt/blender_mcp_addon.py "${BLENDER_ADDON_DIR}/blendermcp.py"
