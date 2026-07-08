#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/.."
export RUST_LOG=${RUST_LOG:-info}
T=${T:-40}                       # NOT 5s — P2P connect needs time
LOGDIR=$(mktemp -d)
A="$LOGDIR/a.log"; B="$LOGDIR/b.log"

# Pre-build once so both peers start ~together (cargo build lock otherwise skews timing).
cargo build -p chat_cli || exit 1
BIN=target/debug/chat_cli

# Peer A: says a unique phrase, then holds the line open so it can receive B.
( printf 'PING_FROM_A_%s\n' "$RANDOM"; sleep "$T" ) | timeout "$T" "$BIN" >"$A" 2>&1 &
PA=$!
( printf 'PING_FROM_B_%s\n' "$RANDOM"; sleep "$T" ) | timeout "$T" "$BIN" >"$B" 2>&1 &
PB=$!
wait $PA $PB

echo "== A log =="; cat "$A"; echo "== B log =="; cat "$B"

# Success = each peer RECV'd the other's PING line.
ok=0
grep -q 'RECV .*PING_FROM_B' "$A" && grep -q 'RECV .*PING_FROM_A' "$B" && ok=1
# Weaker gate if full duplex is flaky: at least one direction propagated.
grep -q 'RECV .*PING_FROM_A' "$B" || grep -q 'RECV .*PING_FROM_B' "$A" && half=1 || half=0

if [ "$ok" = 1 ]; then echo "PASS: bidirectional propagation"; exit 0
elif [ "$half" = 1 ]; then echo "PARTIAL: one direction only"; exit 2
else echo "FAIL: no propagation (check READY lines / RUST_LOG)"; exit 1
fi
