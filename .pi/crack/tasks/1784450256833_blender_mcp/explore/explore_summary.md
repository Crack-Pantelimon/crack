## Summary

The exploration attempted to install Blender 5.1.2 and set up the Blender MCP server in the container. Key findings:

### Blender Installation
- Blender 5.1.2 was downloaded from the official Blender repository and extracted to `/opt/blender-5.1.2-linux-x64/`
- The blender-mcp addon was cloned from GitHub and installed to `/root/.config/blender/5.1/scripts/addons/blender_mcp.py`

### MCP Server Configuration Issues
- **Critical limitation**: The Blender MCP addon **cannot start its server in background mode (`--background` / `-b`)** — commands would never execute
- The addon requires Blender to run with a GUI or virtual display (`xvfb-run -a blender` without `--background`)
- Wayland display connection errors persist despite setting `XDG_RUNTIME_DIR` and `WAYLAND_DISPLAY=""` — Blender keeps trying to use Wayland instead of X11

### Current Blockers
1. Blender insists on Wayland even with xvfb-run; need to force X11 backend (possibly via `QT_QPA_PLATFORM=offscreen` or Blender's `--window-system x11` if available)
2. The MCP server needs to run persistently as a daemon, not in a one-shot Python expression
3. Port 9876 needs to be exposed and the server configured to bind `0.0.0.0`

### File References

- `_docker/Dockerfile:1-50` (main Dockerfile where Blender installation should be added)
- `_docker/_cont_start.sh:1-20` (container startup script for MCP servers)
- `_docker/build.sh:1-15` (build script)
- `_docker/run.sh:1-15` (run script with port mappings)