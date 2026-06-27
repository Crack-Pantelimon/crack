#!/usr/bin/env python3
import os
import sys
import http.server
import socketserver

PORT = 1973
BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIST_DIR = os.path.join(BASE_DIR, "crack_demo", "demo_resolution_selector_web_bevy", "dist")
DATA_DIR = os.path.join(BASE_DIR, "_data")

class CustomHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*')
        super().end_headers()

    def translate_path(self, path):
        # Strip query parameters and anchors
        normalized_path = path.split('?', 1)[0].split('#', 1)[0]
        
        # Route to data directory if requested
        if normalized_path.startswith('/3d_data') or normalized_path.startswith('/sound_data'):
            rel_path = normalized_path.lstrip('/')
            full_path = os.path.join(DATA_DIR, rel_path)
            return full_path
        else:
            # Route to dist directory
            rel_path = normalized_path.lstrip('/')
            if not rel_path:
                rel_path = 'index.html'
            full_path = os.path.join(DIST_DIR, rel_path)
            return full_path

if __name__ == '__main__':
    # Ensure directories exist
    os.makedirs(DIST_DIR, exist_ok=True)
    os.makedirs(DATA_DIR, exist_ok=True)
    
    # Change working directory so SimpleHTTPRequestHandler can resolve properly
    os.chdir(DIST_DIR)
    
    handler = CustomHTTPRequestHandler
    socketserver.TCPServer.allow_reuse_address = True
    
    print(f"Starting server on http://127.0.0.1:{PORT}")
    print(f"Mapping '/' to: {DIST_DIR}")
    print(f"Mapping '/3d_data' and '/sound_data' to subfolders of: {DATA_DIR}")
    
    try:
        with socketserver.TCPServer(("", PORT), handler) as httpd:
            httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down server.")
        sys.exit(0)
    except Exception as e:
        print(f"Error starting server: {e}", file=sys.stderr)
        sys.exit(1)
