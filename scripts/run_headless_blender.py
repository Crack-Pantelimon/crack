import sys
import time
import bpy

# Add the addon directory to sys.path so we can import it
addon_path = '/home/vasile/.gemini/antigravity/scratch/crack/blender-mcp/src/blender-addon'
sys.path.append(addon_path)

import mcp_connector_v2

# Register the addon operators and panel
print("[HEADLESS BLENDER] Registering MCP Connector v2 addon...")
mcp_connector_v2.register()

# Start the WebSocket server using the operator
print("[HEADLESS BLENDER] Starting MCP Server...")
bpy.ops.mcp.start_server()

# Enter a continuous loop in the main thread to process the WebSocket message queue
print("[HEADLESS BLENDER] Server running on ws://127.0.0.1:9876")
print("[HEADLESS BLENDER] Entering event loop. Press Ctrl+C to stop.")
try:
    while True:
        mcp_connector_v2.process_queue()
        time.sleep(0.05)
except KeyboardInterrupt:
    print("[HEADLESS BLENDER] Stopping server...")
finally:
    bpy.ops.mcp.stop_server()
    mcp_connector_v2.unregister()
    print("[HEADLESS BLENDER] Server stopped.")
