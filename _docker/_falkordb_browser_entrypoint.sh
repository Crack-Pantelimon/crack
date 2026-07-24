#!/bin/sh
# Run falkordb-browser on :3001 and a NextAuth auto-login reverse proxy on :3000.
# The upstream UI always shows a login form; we mint a session against FALKORDB_HOST
# so visiting http://localhost:3000 lands in /graph with no manual step.
set -eu

BACKEND_PORT="${AUTOLOGIN_BACKEND_PORT:-3001}"

# Upstream Next listens on the backend port; the proxy owns the published PORT.
export PORT="$BACKEND_PORT"
node server.js &
BACKEND_PID=$!

cleanup() {
  kill "$BACKEND_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Wait until the Next server answers before proxying.
i=0
while [ "$i" -lt 60 ]; do
  if node -e "require('http').get('http://127.0.0.1:${BACKEND_PORT}/api/auth/providers',r=>process.exit(r.statusCode===200?0:1)).on('error',()=>process.exit(1))" \
    >/dev/null 2>&1; then
    break
  fi
  i=$((i + 1))
  sleep 0.5
done

export PORT="${PUBLISHED_PORT:-3000}"
export AUTOLOGIN_BACKEND_HOST=127.0.0.1
export AUTOLOGIN_BACKEND_PORT="$BACKEND_PORT"
exec node /autologin.js
