The answers resolve all open points. No new ambiguities.

**Updated Plan Summary:**

| Item | Decision |
|------|----------|
| **Blender HTTP port** | 9929 (published in `run.sh`) |
| **MCP HTTP transport** | Native FastMCP Streamable HTTP: `blender-mcp --transport streamable-http --host 0.0.0.0 --port 9929` (no supergateway) |
| **Addon install** | Build-time in `Dockerfile` (not `Dockerfile.base` — user said base image is fixed) |
| **Auto-start socket** | `blender --python-expr "..."` setting `blendermcp_auto_start_server=True`, `blendermcp_port=9876`, then `bpy.ops.blendermcp.start_server()` |
| **Verification** | Via `blender-mcp` MCP tool `execute_blender_code` (tests full stack: MCP server → socket → Blender) |

All files to modify are identified. No follow-up questions needed.