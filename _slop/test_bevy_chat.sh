#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/.."
export RUST_LOG=info
LOGDIR=$(mktemp -d)
BEVY_LOG="$LOGDIR/bevy.log"
CLI_LOG="$LOGDIR/cli.log"

# Pre-build both
cargo build -p demo_resolution_selector_web_bevy --bin chat || exit 1
cargo build -p chat_cli || exit 1

# Start Bevy chat
xvfb-run -a timeout 30 target/debug/chat >"$BEVY_LOG" 2>&1 &
BEVY_PID=$!

# Wait 15 seconds for Bevy to connect and log "Starting bevy chat incoming loop..."
sleep 15

# Start chat_cli, send a message, and stay alive for 10 seconds
( echo "PING_FROM_CLI_TO_BEVY"; sleep 10 ) | timeout 12 target/debug/chat_cli >"$CLI_LOG" 2>&1 &
CLI_PID=$!

wait $BEVY_PID $CLI_PID

echo "== Bevy Chat Log =="
cat "$BEVY_LOG"
echo "== CLI Chat Log =="
cat "$CLI_LOG"
