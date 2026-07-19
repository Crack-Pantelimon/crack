# Plan

## Initial build/check instructions

These are Docker/MCP wiring changes, not an app build. Baseline before editing:

```bash
# From inside the running crack-dev container (cwd anywhere):
blender --version                                          # expect Blender 5.1.2 (already present live)
which blender-mcp                                          # expect /usr/local/bin/blender-mcp
test -f /opt/blender/5.1/scripts/addons/blender_mcp.py && echo addon_ok
curl -s http://localhost:9847/api/tasks | head -c 200      # crack-server still up
ss -tlnp | rg '9930|9931|9932'                             # existing HTTP MCP ports listening
cat /workspace/.mcp.json                                   # 3 servers only today
```

From the host (optional, for port publish baseline):

```bash
cd /workspace/_docker && head -n 30 run.sh   # ports 9847, 21122, 9930–9932
```

No `cargo`/`pytest` suite covers this path; regressions are “container still boots + existing MCP ports still answer.”

## Problem statement

The task is to make **Blender 5.1.*** and **blender-mcp** a durable part of `crack-dev`, matching the existing MCP pattern (stdio in `/workspace/.mcp.json` + HTTP bridge in `_cont_start.sh` + port in `run.sh`), then prove control by writing `/workspace/tmp/test.blend` (cube → sphere).

Today `_docker/Dockerfile` only installs ffmpeg/imagemagick. Live exploration already unpacked Blender 5.1.2 under `/opt/blender` and installed the addon + `blender-mcp` 1.6.0, but nothing is baked into the image or wired into startup.

Architecture is two processes: (1) Blender with the addon TCP socket on **9876** (auto-starts on addon register; **rejects `--background`** because commands need the GUI event/timer loop); (2) stdio MCP `blender-mcp`, which connects to that socket (`BLENDER_HOST`/`BLENDER_PORT`). HTTP exposure should follow chromium/web-search: `supergateway` + `tcp_forward.py` on **9929** (next port down from firefox 9930).

“Headless” here means no real display: run Blender under **Xvfb** without `-b`. Verified recipe: `Xvfb` + `XDG_RUNTIME_DIR` + unset `WAYLAND_DISPLAY` + `XDG_SESSION_TYPE=x11` + `LIBGL_ALWAYS_SOFTWARE=1` + `--gpu-backend opengl`. Official Linux packages are **`.tar.xz`**, not `.deb` — pin `https://download.blender.org/release/Blender5.1/blender-5.1.2-linux-x64.tar.xz`.

`_cont_start.sh` globally `unset DISPLAY` for browsers; Blender’s respawn must set its own `DISPLAY` from Xvfb and must not re-export `DISPLAY` for the whole script. Bake Blender under `/opt` (not only `/root`) because `VOLUME /root` means image installs under `/root` do not update the existing volume.

## Changes

### 1. `_docker/Dockerfile` — bake Blender 5.1.2 + addon + blender-mcp

After the existing apt block, add:

```dockerfile
# Blender 5.1.2 (official linux-x64 tarball — no .deb upstream) + Xvfb for MCP GUI mode
RUN apt-get update && apt-get install -y --no-install-recommends \
    xvfb \
    libgl1 \
    libglx-mesa0 \
    libx11-6 \
    libxi6 \
    libxxf86vm1 \
    libxfixes3 \
    libxrender1 \
    && rm -rf /var/lib/apt/lists/*

ARG BLENDER_VERSION=5.1.2
ARG BLENDER_URL=https://download.blender.org/release/Blender5.1/blender-${BLENDER_VERSION}-linux-x64.tar.xz
RUN curl -fsSL -o /tmp/blender.tar.xz "$BLENDER_URL" \
    && mkdir -p /opt \
    && tar -xJf /tmp/blender.tar.xz -C /opt \
    && mv /opt/blender-${BLENDER_VERSION}-linux-x64 /opt/blender \
    && ln -sf /opt/blender/blender /usr/local/bin/blender \
    && rm /tmp/blender.tar.xz

# blender-mcp addon (system scripts path survives VOLUME /root)
RUN curl -fsSL -o /opt/blender/5.1/scripts/addons/blender_mcp.py \
    https://raw.githubusercontent.com/ahujasid/blender-mcp/main/addon.py

# stdio MCP server (system site-packages; also survives /root volume)
RUN pip install --break-system-packages blender-mcp==1.6.0
```

Motivation: pin the download URL in-image; keep binaries under `/opt`; ensure `xvfb` is present for next containers.

### 2. `/workspace/.mcp.json` — register blender like the other servers

Add a `blender` entry:

```json
{
  "mcpServers": {
    "web-search": { ... },
    "chromium":   { ... },
    "firefox":    { ... },
    "blender": {
      "command": "blender-mcp",
      "args": [],
      "env": {
        "BLENDER_HOST": "localhost",
        "BLENDER_PORT": "9876",
        "BLENDER_MCP_DISABLE_TELEMETRY": "1"
      }
    }
  }
}
```

Motivation: pi-mcp-adapter discovers stdio servers from this file; `_cont_start.sh` already copies it to `/root/.config/mcp/mcp.json`.

### 3. `_docker/_cont_start.sh` — Xvfb + Blender daemon + HTTP MCP on 9929

After the existing port exports (~L54–56), add `MCP_BLENDER_PORT=9929` and include it in the comment port list.

Before the firefox/chromium/web-search respawns (or after web-search), start Blender under a dedicated X display, then bridge `blender-mcp`:

```bash
export MCP_BLENDER_PORT=9929
export BLENDER_DISPLAY=:97
export XDG_RUNTIME_DIR=/tmp/runtime-root
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"

# Virtual display for Blender (do NOT export DISPLAY globally — browsers stay headless)
respawn blender-xvfb \
    Xvfb "${BLENDER_DISPLAY}" -screen 0 1280x1024x24 -nolisten tcp

# Wait briefly for Xvfb, then run Blender WITHOUT --background
respawn blender-daemon \
    env DISPLAY="${BLENDER_DISPLAY}" \
        XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR}" \
        XDG_SESSION_TYPE=x11 \
        LIBGL_ALWAYS_SOFTWARE=1 \
        WAYLAND_DISPLAY= \
        blender --gpu-backend opengl --addons blender_mcp

# HTTP SSE bridge (same pattern as chromium/web-search)
respawn blender \
    npx -y supergateway --cors --port "$((MCP_BLENDER_PORT + 10000))" \
        --stdio "env BLENDER_HOST=localhost BLENDER_PORT=9876 BLENDER_MCP_DISABLE_TELEMETRY=1 blender-mcp"
respawn blender-fwd \
    python3 /workspace/_docker/tcp_forward.py "${MCP_BLENDER_PORT}" 127.0.0.1 "$((MCP_BLENDER_PORT + 10000))"
```

If `respawn` cannot take `env` cleanly, wrap in a tiny `/workspace/_docker/blender_daemon.sh` called by `respawn`. Ensure the Blender process stays alive (no `--python-expr` that exits).

Motivation: persistent addon socket + host-reachable MCP on 9929; keep global `unset DISPLAY` for chromium.

### 4. `_docker/run.sh` — publish 9929

Add:

```bash
-p "127.0.0.1:9929:9929" \
```

next to the other MCP `-p` lines (after 9932).

### 5. `_docker/README.md` — document Blender + ports

Extend the installed-software table and MCP sections:

- Blender 5.1.2 from the tar.xz URL above → `/opt/blender`
- blender-mcp 1.6.0 + addon path
- Host URL: `http://localhost:9929/sse`
- Note: no official `.deb`; must not use `blender -b`; Xvfb recipe; `/root` volume caveat for userprefs

### 6. Live container apply (this box, without full image rebuild)

Mirror Dockerfile steps if missing, then apply `_cont_start.sh` / `.mcp.json` / `run.sh` changes. Start Xvfb + Blender daemon now (or restart via re-running the new respawn blocks). Verify end-to-end (see Automatic verification). Leave `/workspace/tmp/test.blend` as the success artifact.

Optional helper script `_docker/blender_daemon.sh` if env wrapping in `respawn` is awkward — only if needed.

## What NOT to change

- `_docker/Dockerfile.base` (unless later promoting Blender there; user asked for `Dockerfile`)
- Existing MCP servers’ commands/ports (9930–9932) and `tcp_forward.py` behavior
- Global `unset DISPLAY` / chromium flags in `_cont_start.sh`
- crack-server / stages / harness Python app code
- Blender addon protocol / upstream `blender_mcp.py` logic (no fork to force `0.0.0.0` on 9876 — in-container MCP uses `localhost:9876`; host uses HTTP 9929)
- Do not use `blender --background` / `-b` for the MCP daemon
- Do not publish 9876 unless later needed; 9929 is the HTTP MCP port

## Automatic verification

Run inside the container after wiring:

```bash
# 1. Binary + addon
blender --version | head -1                    # Blender 5.1.2
test -x /usr/local/bin/blender
test -f /opt/blender/5.1/scripts/addons/blender_mcp.py

# 2. Daemons (after starting Xvfb + blender + bridges)
ss -tlnp | rg '9876|9929|19929'                # addon socket + forwarder (+ optional internal)
rg -n "BlenderMCP server started" /workspace/.pi/crack/harness/mcp-http/blender-daemon.log

# 3. MCP stdio path: execute cube→sphere→save via a one-shot client
#    (example using a short Python MCP client or blender-mcp tool invocation)
mkdir -p /workspace/tmp
# Prefer driving through the MCP tool execute_blender_code with code equivalent to:
#   import bpy
#   bpy.data.objects.remove(bpy.data.objects["Cube"], do_unlink=True)
#   bpy.ops.mesh.primitive_uv_sphere_add()
#   bpy.ops.wm.save_as_mainfile(filepath="/workspace/tmp/test.blend")

# Concrete non-interactive check if MCP client scripting is heavy: talk to addon socket
# with the same JSON protocol blender-mcp uses, OR use:
python3 - <<'PY'
# minimal: connect TCP 9876 and send execute_code (match blender-mcp framing)
# ...implementation during coding step...
PY

test -f /workspace/tmp/test.blend
# Confirm sphere present / cube absent via a follow-up get_scene_info or bpy query

# 4. HTTP bridge smoke
curl -sS -o /dev/null -w "%{http_code}\n" http://127.0.0.1:9929/sse || true
# (SSE may need Accept headers; at minimum port must be listening)

# 5. Regressions
ss -tlnp | rg '9930|9931|9932'
curl -s http://localhost:9847/api/tasks >/dev/null
```

Image path (when rebuilding):

```bash
cd /workspace/_docker && ./build.sh
# then run.sh (publishes 9929); re-check blender --version inside new container
```

## Manual verification

1. Open pi (or Cursor MCP) with the updated `.mcp.json`; `mcp({ server: "blender" })` lists tools including `execute_blender_code`.
2. Ask to replace the default cube with a UV sphere and save `/workspace/tmp/test.blend`.
3. On the host: confirm `127.0.0.1:9929` is mapped; optionally point a host MCP client at `http://localhost:9929/sse`.
4. Spot-check logs under `/workspace/.pi/crack/harness/mcp-http/` for blender-xvfb / blender-daemon / blender / blender-fwd — no Wayland crash loops.
5. Confirm chromium/firefox MCP still work (example.com) so Blender’s Xvfb did not break browser headless via leaked `DISPLAY`.

## Overview / Summary

**Goal:** Pin Blender 5.1.2 in `_docker/Dockerfile`, wire blender-mcp like the other MCP servers (stdio + HTTP **9929**), keep a long-lived Xvfb Blender (no `-b`) with addon on **9876**, and prove it by writing `/workspace/tmp/test.blend`.

**Shape:** Dockerfile bake → `.mcp.json` entry → `_cont_start.sh` daemons (Xvfb, Blender, supergateway, tcp_forward) → `run.sh` publish → README → live verify.

**Main risks:** Wayland/X11 env must stay correct or Blender crashes; `--background` silently breaks the addon; `/root` volume hides installs under `/root`; leaking `DISPLAY` into browser MCP; `respawn` + multi-env `env` quoting — use a small wrapper script if needed.

I've already gathered sufficient information and provided the Lay of the Land and full plan. No further clarification needed.