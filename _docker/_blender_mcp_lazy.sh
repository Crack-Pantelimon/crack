#!/bin/bash
# Lazy Blender bootstrap for stdio blender-mcp (sandboxes and crack-dev pi agents).
# If nothing listens on BLENDER_PORT, start Xvfb + Blender with the blendermcp addon,
# wait for the addon socket, then exec blender-mcp. Best-effort — most sandboxes never call this.
set -e

BLENDER_PORT="${BLENDER_PORT:-9876}"
LOCK_FILE="/tmp/blender-mcp-lazy.lock"

port_open() {
    python3 - "$BLENDER_PORT" <<'PY'
import socket, sys
s = socket.socket()
s.settimeout(0.5)
try:
    s.connect(("127.0.0.1", int(sys.argv[1])))
except OSError:
    sys.exit(1)
finally:
    s.close()
PY
}

start_blender_stack() {
    export WAYLAND_DISPLAY=""
    export DISPLAY=:99
    export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/runtime-root}"
    mkdir -p -m 700 "$XDG_RUNTIME_DIR"
    if ! pgrep -x Xvfb >/dev/null 2>&1; then
        rm -f /tmp/.X99-lock /tmp/.X11-unix/X99
        Xvfb :99 -screen 0 1920x1080x24 -ac +extension GLX +extension RANDR +extension RENDER \
            >/tmp/blender-lazy-xvfb.log 2>&1 &
        sleep 2
    fi
    # Blender 5.x: `-noaudio` (single dash). `--noaudio` is treated as a .blend path.
    if ! pgrep -f 'blender -noaudio' >/dev/null 2>&1; then
        blender -noaudio --addons blendermcp >/tmp/blender-lazy-blender.log 2>&1 &
        sleep 5
    fi
    for _ in $(seq 1 60); do
        if port_open; then
            return 0
        fi
        sleep 1
    done
    echo "blender-mcp lazy: timed out waiting for :${BLENDER_PORT}" >&2
    return 1
}

if ! port_open 2>/dev/null; then
    exec 9>"$LOCK_FILE"
    if ! flock -w 120 9; then
        echo "blender-mcp lazy: could not acquire lock" >&2
        exit 1
    fi
    if ! port_open 2>/dev/null; then
        start_blender_stack
    fi
fi

exec blender-mcp "$@"
