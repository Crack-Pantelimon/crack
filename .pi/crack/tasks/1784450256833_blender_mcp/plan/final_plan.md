# Plan

## Initial build/check instructions
```bash
# Build the Docker image (from repo root)
cd /workspace
./_docker/build.sh

# Run the container (from repo root)
./_docker/run.sh

# Verify existing services are up
curl -s http://localhost:9847/api/tasks      # crack-server
curl -s http://localhost:9930/mcp           # playwright-mcp (firefox)
curl -s http://localhost:9931/sse           # chrome-devtools-mcp (via supergateway)
curl -s http://localhost:9932/sse           # web-search-mcp (via supergateway)

# Inside container: verify base tools
docker exec crack-dev /bin/bash -exc "blender --version 2>&1 || echo 'blender not installed'"
docker exec crack-dev /bin/bash -exc "xvfb-run --version 2>&1 || echo 'xvfb not installed'"
docker exec crack-dev /bin/bash -exc "pip show blender-mcp 2>&1 || echo 'blender-mcp not installed'"
```

## Problem statement
The task is to install **Blender 5.1.2** (latest 5.1.x) and the **blender-mcp** server inside the `crack-dev` Docker container, configured identically to the existing MCP servers (playwright, chrome-devtools, web-search). Blender must run headless via `xvfb-run` (the MCP addon cannot operate in `--background` mode). The Blender addon listens on TCP **9876** for its internal socket server; the MCP HTTP endpoint for external clients uses **port 9877** (next HTTP port down from 9932). The setup must be persistent across container rebuilds by baking installation into `_docker/Dockerfile` and startup into `_docker/_cont_start.sh`. Finally, verify the full stack works by using the MCP `execute_blender_code` tool to create a sphere replacing the default cube and save as `/workspace/tmp/test.blend`.

Key constraints from exploration:
- Blender 5.1.2 Linux x64 tarball: `https://download.blender.org/release/Blender5.1/blender-5.1.2-linux-x64.tar.xz` (SHA256: `aaccb355f50183979b698bcce7467103a76261b5fa59f4972295842662a285fb`)
- blender-mcp 1.6.4 on PyPI installs `blender-mcp` CLI (MCP server) and provides `addon.py` (Blender addon)
- The addon **requires a GUI or virtual display** — it starts a TCP socket server on port 9876 inside Blender; `blender -b` (background) never processes commands
- `xvfb-run -a blender` (without `--background`) works; must set `QT_QPA_PLATFORM=offscreen` and `WAYLAND_DISPLAY=""` to force X11
- Existing MCP servers in `_cont_start.sh` use a `respawn` loop, bind `0.0.0.0`, and are published in `run.sh` ports 9930/9931/9932
- `.mcp.json` at repo root is copied to `/root/.config/mcp/mcp.json` for the `pi-mcp-adapter` (stdio); HTTP exposure uses supergateway on internal port + `tcp_forward.py` to 0.0.0.0
- blender-mcp MCP server (`uvx blender-mcp`) connects to Blender addon via `BLENDER_HOST`/`BLENDER_PORT` env vars (defaults localhost:9876)
- Blender's `--addons` flag (available since 3.0) enables addons at startup; the addon module name must match the filename (`blendermcp.py`)
- Blender 5.1.x config directory is `5.1` (not `5.1.2`) under `~/.config/blender/`

## Changes

### 1. `_docker/Dockerfile` — Install Blender 5.1.2, xvfb, blender-mcp, and addon
**File:** `/workspace/_docker/Dockerfile` (lines 1-25, replace entire file)

**Before:**
```dockerfile
FROM johnnysmitherson/crack-dev:base

RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    imagemagick
VOLUME /root
WORKDIR /workspace

EXPOSE 22

CMD ["/usr/sbin/sshd", "-D"]
```

**After:**
```dockerfile
FROM johnnysmitherson/crack-dev:base

# Install Blender 5.1.2 dependencies + xvfb for headless GUI
RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    imagemagick \
    xvfb \
    libgl1-mesa-glx \
    libxi6 \
    libxrender1 \
    libxxf86vm1 \
    && rm -rf /var/lib/apt/lists/*

# Download and install Blender 5.1.2
ARG BLENDER_VERSION=5.1.2
ARG BLENDER_SHA256=aaccb355f50183979b698bcce7467103a76261b5fa59f4972295842662a285fb
ARG BLENDER_URL=https://download.blender.org/release/Blender5.1/blender-5.1.2-linux-x64.tar.xz

RUN cd /opt \
    && wget -q "${BLENDER_URL}" -O blender.tar.xz \
    && echo "${BLENDER_SHA256}  blender.tar.xz" | sha256sum -c - \
    && tar -xf blender.tar.xz \
    && mv blender-${BLENDER_VERSION}-linux-x64 blender-${BLENDER_VERSION} \
    && ln -sf /opt/blender-${BLENDER_VERSION}/blender /usr/local/bin/blender \
    && rm blender.tar.xz

# Install blender-mcp (provides MCP server CLI + addon.py)
# Use --break-system-packages since container is isolated
RUN pip install --no-cache-dir --break-system-packages blender-mcp==1.6.4

# Install blender-mcp addon for Blender 5.1
# The addon module must be named 'blendermcp.py' to match bl_info["name"] = "Blender MCP"
RUN BLENDER_ADDON_DIR="/root/.config/blender/5.1/scripts/addons" \
    && mkdir -p "${BLENDER_ADDON_DIR}" \
    && python3 -c "import blender_mcp; import os; print(os.path.dirname(blender_mcp.__file__))" | xargs -I{} cp {}/addon.py "${BLENDER_ADDON_DIR}/blendermcp.py"

VOLUME /root
WORKDIR /workspace

EXPOSE 22 9876 9877

CMD ["/usr/sbin/sshd", "-D"]
```

**Motivation:** Bakes Blender, xvfb, blender-mcp, and the addon into the image. The addon is copied as `blendermcp.py` (matching module name) to Blender 5.1's config directory. Ports 9876 (Blender addon socket) and 9877 (MCP HTTP) exposed.

---

### 2. `_docker/_cont_start.sh` — Start Blender (via xvfb) + blender-mcp MCP server (via supergateway on port 9877)
**File:** `/workspace/_docker/_cont_start.sh` (append after web-search block, before worker/server)

**Add after the web-search-fwd respawn block (before the worker/server section):**
```bash
# --- Blender MCP (ports 9876 addon socket, 9877 HTTP) -----------------------
# Blender addon runs inside Blender (xvfb) on TCP 9876.
# blender-mcp MCP server (stdio) connects to it; we bridge via supergateway to HTTP :9877.
export BLENDER_ADDON_PORT=9876
export MCP_BLENDER_HTTP_PORT=9877
export BLENDER_HOST=127.0.0.1
export BLENDER_PORT=9876
# Force X11 backend, disable Wayland
export QT_QPA_PLATFORM=offscreen
export WAYLAND_DISPLAY=""
export DISPLAY=:99

# Start Xvfb on display :99
Xvfb :99 -screen 0 1920x1080x24 -ac +extension GLX +extension RANDR +extension RENDER >/workspace/.pi/crack/harness/mcp-http/xvfb.log 2>&1 &

# Wait for Xvfb to be ready
sleep 2

# Start Blender with the addon enabled via --addons flag (auto-starts server on port 9876)
# The addon's auto_start_server defaults to True and uses blendermcp_port (default 9876)
respawn blender \
    xvfb-run -a -s "-screen 0 1920x1080x24" \
    blender --noaudio --addons blendermcp

# Give Blender time to start the socket server
sleep 5

# Bridge blender-mcp MCP server (stdio) to HTTP on port 9877 via supergateway
# supergateway runs on internal port 19877 (9877+10000), tcp_forward exposes on 0.0.0.0:9877
respawn blender-mcp \
    npx -y supergateway --cors --port "$((MCP_BLENDER_HTTP_PORT + 10000))" \
        --stdio "uvx --python 3.11 blender-mcp"
respawn blender-mcp-fwd \
    python3 /workspace/_docker/tcp_forward.py "${MCP_BLENDER_HTTP_PORT}" 127.0.0.1 "$((MCP_BLENDER_HTTP_PORT + 10000))"
```

**Motivation:** Starts Xvfb, launches Blender with `--addons blendermcp` (enables addon at startup, which auto-starts its socket server on 9876 per addon defaults). Then runs `uvx blender-mcp` (MCP server) bridged via supergateway to HTTP on 9877 (published in run.sh). Uses `respawn` for crash recovery like other MCPs. Separate ports: 9876 for Blender addon socket, 9877 for MCP HTTP endpoint.

---

### 3. `_docker/run.sh` — Publish port 9877
**File:** `/workspace/_docker/run.sh` (add port mapping)

**Add to docker run command (after `-p "127.0.0.1:9932:9932"`):**
```bash
  -p "127.0.0.1:9877:9877" \
```

**Motivation:** Exposes Blender MCP HTTP endpoint to host on port 9877 (next port down from 9932). The Blender addon socket on 9876 remains internal.

---

### 4. `/workspace/.mcp.json` — Add blender-mcp server config for pi-mcp-adapter (stdio)
**File:** `/workspace/.mcp.json` (add to mcpServers object)

**Add entry:**
```json
    "blender": {
      "command": "uvx",
      "args": ["--python", "3.11", "blender-mcp"],
      "env": {
        "BLENDER_HOST": "localhost",
        "BLENDER_PORT": "9876",
        "DISABLE_TELEMETRY": "true"
      }
    }
```

**Motivation:** Allows `pi` agents inside the container to use the blender MCP via stdio (the adapter reads `.mcp.json` from cwd). The HTTP bridge on 9877 is for external clients. The MCP server connects to Blender addon on localhost:9876.

---

### 5. Verification script — Create sphere, replace cube, save test.blend
**File:** `/workspace/_docker/verify_blender_mcp.py` (new file)

```python
#!/usr/bin/env python3
"""Verify blender-mcp works: replace default cube with sphere, save /workspace/tmp/test.blend"""
import asyncio
import json
import sys
import os

# Use the MCP client to call execute_blender_code via HTTP endpoint
async def main():
    import httpx
    
    url = "http://localhost:9877/mcp"
    headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
    
    # Initialize
    init_msg = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "verify", "version": "1.0"}
        }
    }
    
    async with httpx.AsyncClient(timeout=60.0) as client:
        # Initialize
        resp = await client.post(url, json=init_msg, headers=headers)
        print(f"Initialize: {resp.status_code}")
        
        # List tools
        list_tools = {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}
        resp = await client.post(url, json=list_tools, headers=headers)
        print(f"Tools: {resp.text[:500]}")
        
        # Call execute_blender_code to replace cube with sphere
        code = """
import bpy

# Delete default cube
bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete()

# Create sphere
bpy.ops.mesh.primitive_uv_sphere_add(radius=1.0, location=(0, 0, 0))
sphere = bpy.context.active_object
sphere.name = "TestSphere"

# Save
output_path = "/workspace/tmp/test.blend"
os.makedirs(os.path.dirname(output_path), exist_ok=True)
bpy.ops.wm.save_as_mainfile(filepath=output_path)
print(f"Saved to {output_path}")
"""
        call_tool = {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "execute_blender_code",
                "arguments": {"code": code, "user_prompt": "verify blender mcp"}
            }
        }
        resp = await client.post(url, json=call_tool, headers=headers, timeout=120.0)
        print(f"Execute result: {resp.status_code}")
        print(resp.text[:2000])
        
        # Verify file exists
        if os.path.exists("/workspace/tmp/test.blend"):
            print("SUCCESS: /workspace/tmp/test.blend created")
            return 0
        else:
            print("FAIL: test.blend not found")
            return 1

if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
```

**Motivation:** Automated verification that the full stack (Blender + addon + MCP server + HTTP bridge) works end-to-end. Runs inside container (docker exec) against localhost:9877/mcp.

---

## What NOT to change
- `_docker/Dockerfile.base` — base image unchanged
- `_docker/build.sh` — build script unchanged
- `_docker/tcp_forward.py` — reusable TCP forwarder unchanged
- `_docker/README.md` — documentation unchanged (optional update later)
- `/workspace/.pi/crack/server/src/crack_server/` — application code unchanged
- Existing MCP configs (playwright, chromium, web-search) — must keep working
- Port assignments for existing services (9847, 9930, 9931, 9932, 21122) — unchanged

---

## Automatic verification
```bash
# 1. Rebuild and restart container
cd /workspace
./_docker/build.sh
./_docker/run.sh

# 2. Wait for services to stabilize (~15s)
sleep 15

# 3. Check Blender process and ports
docker exec crack-dev /bin/bash -exc "ps aux | grep -E '(blender|Xvfb)' | grep -v grep"
docker exec crack-dev /bin/bash -exc "ss -ltn | grep -E '9876|9877'"

# 4. Test MCP HTTP endpoint (port 9877)
curl -s -X POST http://localhost:9877/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}'

# 5. Run verification script (inside container)
docker exec crack-dev /bin/bash -exc "cd /workspace && python3 _docker/verify_blender_mcp.py"

# 6. Verify output file exists and is valid
docker exec crack-dev /bin/bash -exc "ls -la /workspace/tmp/test.blend && blender -b /workspace/tmp/test.blend --python-expr \"import bpy; print([o.name for o in bpy.data.objects])\""
```

---

## Manual verification
1. **Host browser:** Open `http://localhost:9877/mcp` — should show MCP endpoint responding
2. **Container logs:** `docker logs crack-dev -f | grep -E '(blender|Xvfb|blender-mcp)'` — verify Blender starts, addon loads, server binds 9876
3. **Blender file inspection:** `docker exec crack-dev blender -b /workspace/tmp/test.blend --python-expr "import bpy; print('Objects:', [o.name for o in bpy.data.objects]); print('Mesh:', [m.name for m in bpy.data.meshes])"` — should show `TestSphere` with UV sphere mesh
4. **MCP tool call via pi:** In the web UI (http://localhost:9847), create a task and use the chat to ask pi to call `execute_blender_code` — verify it works through the stdio MCP path too

---

## Overview / Summary
**Goal:** Add Blender 5.1.2 + blender-mcp to the `crack-dev` container as a persistent, auto-starting MCP service. Blender addon socket on port 9876, MCP HTTP endpoint on port 9877, matching the pattern of existing MCP servers.

**Solution shape:**
1. **Dockerfile:** Install Blender 5.1.2 (official tarball, verified SHA256), xvfb, mesa GL, blender-mcp 1.6.4 (pip with --break-system-packages), copy addon as `blendermcp.py` to Blender 5.1 config dir
2. **Container startup (`_cont_start.sh`):** Start Xvfb :99 → launch Blender via xvfb-run with `--addons blendermcp` (enables addon at startup, auto-starts socket server on 9876) → bridge `uvx blender-mcp` via supergateway to HTTP :9877 (with tcp_forward to 0.0.0.0)
3. **Port publishing (`run.sh`):** Add `-p 127.0.0.1:9877:9877`
4. **MCP config (`.mcp.json`):** Add stdio config for pi-mcp-adapter (connects to Blender addon on localhost:9876)
5. **Verification:** Python script calls MCP `execute_blender_code` via HTTP on localhost:9877/mcp to replace cube with sphere, save `/workspace/tmp/test.blend`

**Main risks:**
- Blender addon may fail to auto-start server (race condition) — mitigated by `sleep 5` and `respawn` loop
- Xvfb/GL issues on headless — mitigated by `QT_QPA_PLATFORM=offscreen`, `WAYLAND_DISPLAY=""`, mesa packages
- blender-mcp version compatibility — pinned to 1.6.4 with Python 3.11
- Port conflicts — 9876/9877 are free (next down from 9932)
- Addon module name must match filename (`blendermcp.py`) for `--addons` to work