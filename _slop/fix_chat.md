# Fix Chat — Plan

## Goal

Our chat binary `crack_demo/demo_resolution_selector_web_bevy/src/bin/chat.rs` "doesn't work
properly" (messages don't seem to propagate / never connects). We want to:

1. Build a **headless, bevy-free** `chat_cli` binary that speaks the same chat protocol.
2. Use it to get a deterministic, scriptable feedback loop (stdin → broadcast, incoming → stdout).
3. Drive two `chat_cli` processes against each other and prove messages propagate.
4. Fix whatever the two-process test exposes in the `crack_demo` crates / `net_crackpipe`.

## Findings (already verified)

- **`chat.rs` compiles cleanly.** `cargo build -p demo_resolution_selector_web_bevy --bin chat`
  succeeds (only warnings). So this is a **runtime** problem, not a compile mismatch. The egui UI
  hides all connection state behind a status string, so we can't see what's failing.
- The chat stack is a **P2P iroh-gossip network** (`net_crackpipe`), a near-verbatim copy of the
  imported `_slop/examples/Sparganothis-v2/protocol` crate. The chat modules
  (`chat/global_chat.rs`, `chat/chat_controller.rs`, …) are byte-identical to the reference.
- Connection path (`GlobalMatchmaker::new` → `new_try_once` → `connect_to_bootstrap` /
  `spawn_bootstrap_endpoint` → `connect_global_chats`): it tries **3 times** with `sleep(1+i)`
  between attempts, connects to hardcoded internet **bootstrap nodes**, then joins the gossip
  topic `GLOBAL_CHAT_TOPIC_ID`, then `wait_joined()`. Two fresh peers must each connect to
  bootstrap, join the topic, discover each other via gossip, and only *then* can a message
  propagate.
  - **Implication:** the requested `timeout 5s` is almost certainly too short. Realistic
    connect+join+discover+propagate is **~15–40s**. The test must use a longer timeout and keep
    each peer alive long enough to receive after sending.
- **Reference vs ours — one real divergence:** `GlobalChatPresence.is_server` is
  `Option<ServerInfo>` in ours (`packages/net_crackpipe/src/chat/global_chat.rs:24`) vs `bool` in
  the Sparganothis reference. `chat.rs` already passes `is_server: None`, and
  `global_matchmaker.rs:347` uses `None`, so this is self-consistent — **not** the bug, just a note
  that our copy has drifted. Don't "fix" it back to `bool`.
- The reference headless client is `_slop/examples/Sparganothis-v2/client_terminal` — a ratatui
  app whose `chat_driver` (`.../pages/chat/mod.rs`) is exactly the loop we want, minus the TUI. Our
  `chat_cli` is a stdin/stdout port of that driver.

## Relevant API (verified in `net_crackpipe`)

```rust
use net_crackpipe::{
    chat::chat_controller::{IChatController, IChatReceiver, IChatSender},
    chat::global_chat::{GlobalChatMessageContent, GlobalChatPresence},
    global_matchmaker::GlobalMatchmaker,
    user_identity::UserIdentitySecrets,
};

let secrets = UserIdentitySecrets::generate();          // random German-word nickname + rgb
let mm = GlobalMatchmaker::new(Arc::new(secrets)).await?;      // connect (retries 3x internally)
let controller = mm.global_chat_controller().await.unwrap();   // Option<ChatController<GlobalChatRoomType>>
let sender   = controller.sender();                     // ChatSender
let presence = controller.chat_presence();
sender.set_presence(&GlobalChatPresence {
    url: "".into(), platform: "chat_cli".into(), is_server: None,
}).await;
controller.wait_joined().await?;                        // blocks until in the gossip room
let recv = controller.receiver().await;                 // ChatReceiver

// send:
let preview = sender.broadcast_message(GlobalChatMessageContent::TextMessage { text }).await?;
// receive (own broadcasts are NOT echoed back through recv — echo locally like chat.rs does):
while let Some(msg) = recv.next_message().await {
    // msg.from: NodeIdentity  -> .nickname(): String, .rgb_color(): (u8,u8,u8)
    // msg.message: GlobalChatMessageContent
}
mm.shutdown().await?;
```

`ReceivedMessage<T>` fields: `from: NodeIdentity`, `message: T::M` (see
`packages/net_crackpipe/src/signed_message.rs:125`).

## Step 1 — Create the `chat_cli` crate (bevy-free)

A binary in the bevy package would still force bevy to compile as a package dependency. To be
genuinely bevy-free and fast to iterate, add a tiny workspace crate.

**New crate: `crack_demo/chat_cli/`**

`crack_demo/chat_cli/Cargo.toml`:
```toml
[package]
name = "chat_cli"
version.workspace = true
edition = "2021"

[dependencies]
net_crackpipe = { path = "../../packages/net_crackpipe" }
tokio = { version = "1", default-features = false, features = ["rt-multi-thread", "macros", "sync", "io-std", "io-util", "time"] }
anyhow = { workspace = true }
tracing-subscriber = "0.3"   # so RUST_LOG surfaces the connection state chat.rs hides
```

Register it in the root `Cargo.toml` `[workspace].members` list (next to
`crack_demo/thread_worker`, etc.).

`crack_demo/chat_cli/src/main.rs` (behavioral spec — a stdin/stdout port of the reference
`chat_driver`):

- `#[tokio::main]` → init `tracing_subscriber` from `RUST_LOG` (default `info`) so bootstrap/join
  logs are visible.
- Generate identity; print `SELF <nickname>` to stdout (test anchor).
- `GlobalMatchmaker::new(...)`; get controller; `set_presence`; `wait_joined()`.
- On success print `READY <nickname>` to stdout (test anchor for "connected").
- **Spawn a stdin reader task**: read lines (`tokio::io::BufReader::new(stdin).lines()`), and for
  each non-empty line call `sender.broadcast_message(TextMessage { text }).await`, then print
  `SENT <text>`. When stdin hits EOF, **do not exit** — the process must stay alive to receive
  replies; it is bounded by the external `timeout`.
- **Main receive loop**: `while let Some(msg) = recv.next_message().await` → print
  `RECV <nickname> <text>` for `TextMessage`. This distinguishes remote messages for the test.
- Handle Ctrl-C / process kill cleanly (best-effort `mm.shutdown()`); the `timeout` SIGTERM is the
  normal stop path.

Output-line contract (keep stable — the test greps these):
```
SELF <nick>          # our identity
READY <nick>         # wait_joined() returned; we're in the room
SENT <text>          # we broadcast a line from stdin
RECV <nick> <text>   # a message arrived from a peer
```

## Step 2 — Manual smoke test (one peer)

```bash
echo "hello world" | RUST_LOG=info timeout 40 cargo run -p chat_cli
```
Expect to reach `READY ...`. If it never prints `READY`, the bug is in
connect/bootstrap/`wait_joined` — read the `tracing` output (the whole point of the headless
binary) to localize it in `net_crackpipe::global_matchmaker`. This is where the actual "not working"
root cause will surface.

## Step 3 — Automated two-peer propagation test

The user's shape is `cat message | timeout Ns cargo run --bin chat_cli` ×2. Concretely, script it as
`_slop/test_chat.sh`:

```bash
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
```

Notes:
- **Timeout:** use `T=40` (override with env). `sleep "$T"` inside the pipe keeps stdin open so the
  peer stays alive to receive after sending; `timeout "$T"` is the hard stop.
- Each peer sends *one* uniquely-tagged line so we can assert the exact message crossed the wire,
  not just "some message."
- If `READY` never appears in a log, that's the failure to chase first (Step 2).

## Step 4 — Fix what the test exposes, in `crack_demo` / `net_crackpipe`

Likely buckets, in order of probability:
1. **Connect never completes** (`READY` never prints): bootstrap/relay/topic issue in
   `net_crackpipe::global_matchmaker`. Compare our `global_matchmaker.rs` and
   `chat/chat_controller.rs` against the reference `_slop/examples/Sparganothis-v2/protocol` — the
   chat files are identical, so look for drift in `api/`, `main_node.rs`, `_bootstrap_keys.rs`,
   `echo.rs`, or the iroh version/features in `Cargo.toml`.
2. **Connects but no propagation** (`READY` yes, `RECV` no): peers not landing on the same topic /
   not discovering each other — inspect `get_global_chat_ticket`, `bootstrap_nodes_set`, and
   `GLOBAL_CHAT_TOPIC_ID`.
3. **Propagation works headless but egui `chat.rs` still looks dead**: bug is only in the bevy
   glue (`chat.rs` `spawn_chat_thread`) — e.g. the status never leaves "Connecting…", or the
   receive loop / channel wiring. Port the now-proven `chat_cli` driver structure into `chat.rs`.

Once `chat_cli` propagates reliably, re-run `chat.rs` and confirm the UI shows presence + messages.

## Step 5 — Wire the test in (optional)

- Add `_slop/test_chat.sh` (chmod +x).
- Optionally a `just`/make target or a note in `AGENTS.md` so the two-peer test is the standard way
  to validate chat changes.

## Deliverables checklist

- [ ] `crack_demo/chat_cli/` crate (Cargo.toml + src/main.rs), added to workspace members.
- [ ] `chat_cli` prints the `SELF/READY/SENT/RECV` line contract.
- [ ] `_slop/test_chat.sh` launches two peers and asserts propagation (realistic timeout, not 5s).
- [ ] Two-peer test passes (at least one direction, ideally bidirectional).
- [ ] Root-cause fix applied in `net_crackpipe`/`chat.rs`; egui chat verified working.

## Open decision

- **`timeout` value:** the requested 5s will not survive a cold P2P connect. Plan uses ~40s. If you
  truly need a 5s gate, we'd need a persistent already-connected peer (a long-running "server"
  `chat_cli`) that the 5s client joins — say the word and I'll add that variant.
