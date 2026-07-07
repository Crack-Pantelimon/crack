# Implementation Plan: Pull Gossip/Chat Code from Sparganothis-v2 and build Bevy Chat Binary

This plan details the steps required to pull node identities, chat managers, presence managers, etc., from `_slop/examples/Sparganothis-v2/*` into the workspace package `packages/net_crackpipe` and use it to build a new Bevy demo binary `crack_demo/demo_resolution_selector_web_bevy/src/bin/chat.rs`.

---

## 1. Proposed Crate Dependency Updates

### `packages/net_crackpipe/Cargo.toml`
Add dependencies necessary for building the chat/gossip protocol using `iroh` v0.34:
- `iroh = { version = "0.34.0", default-features = false }`
- `iroh-base = { version = "0.34.0", default-features = false, features = ["ticket"] }`
- `iroh-gossip = { version = "0.34.1", default-features = false, features = ["net"] }`
- `random_word = { version = "0.5.0", features = ["de"] }`
- `blake3 = { version = "1", package = "iroh-blake3" }`
- `hex = "0.4"`
- `n0-future = "0.1.2"`
- `postcard = "1.1.1"`
- `tokio = { version = "1", default-features = false, features = ["sync"] }`
- `tokio-stream = { version = "0.1.17", default-features = false, features = ["sync"] }`
- `tracing = "0.1"`
- `async-channel = "2.3.1"`
- `async-broadcast = "0.7.2"`
- `chrono = { version = "0.4", features = ["serde"] }`
- `web-time = "1.1"`
- `futures = "0.3"`
- `uuid = { version = "1", features = ["v4", "serde"] }`
- `serde = { version = "1", features = ["derive"] }`
- `serde_json = "1"`
- `async-trait = "0.1.88"`
- `paste = "1.0"`

### `crack_demo/demo_resolution_selector_web_bevy/Cargo.toml`
- Add `net_crackpipe = { path = "../../packages/net_crackpipe" }` to dependencies.

---

## 2. Code Copying and Decoupling

We will pull the protocol files from `_slop/examples/Sparganothis-v2/protocol/src/` to `packages/net_crackpipe/src/`.
To avoid adding a dependency on the Sparganothis `game` crate, we will:
1. Remove all `use game::timestamp::get_timestamp_now_ms;` imports.
2. Define a local helper function `get_timestamp_now_ms() -> i64` in `net_crackpipe` (e.g. inside `chat/chat_presence.rs` and `global_matchmaker.rs`, or a utility):
   ```rust
   pub fn get_timestamp_now_ms() -> i64 {
       chrono::offset::Utc::now().timestamp_millis()
   }
   ```
3. Copy `ServerInfo` structure definition directly into `chat/global_chat.rs` and remove the dependency on `api::api_method_macros::ServerInfo`.

### Source Files to Copy to `packages/net_crackpipe/src/`
- `_bootstrap_keys.rs`
- `_random_word.rs`
- `chat/chat_const.rs`
- `chat/chat_controller.rs`
- `chat/chat_presence.rs`
- `chat/chat_ticket.rs`
- `chat/global_chat.rs`
- `chat/room_raw.rs`
- `chat/direct_message.rs`
- `chat/mod.rs`
- `global_matchmaker.rs`
- `main_node.rs`
- `signed_message.rs`
- `sleep.rs`
- `echo.rs`
- `user_identity.rs`

### `packages/net_crackpipe/src/lib.rs`
Expose the pulled modules publicly:
```rust
pub mod _bootstrap_keys;
pub mod _random_word;
pub mod chat;
pub mod echo;
pub mod global_matchmaker;
pub mod main_node;
pub mod signed_message;
pub mod sleep;
pub mod user_identity;
```

---

## 3. Bevy Binary Implementation: `crack_demo/demo_resolution_selector_web_bevy/src/bin/chat.rs`

The binary will:
1. Initialize the Bevy App with the default debug scene using `make_basic_app` and `SetupDebugScenePlugin`.
2. Add `bevy_egui::EguiPlugin`.
3. Spawn a background thread running a Tokio multi-threaded runtime. This runtime will:
   - Create a `UserIdentitySecrets` (retaining the German words nickname generation gimmick).
   - Instantiate `GlobalMatchmaker` and retrieve the global chat controller.
   - Join the global chat channel.
   - Listen for new messages and presence list notifications, posting them to a thread-safe channel (`std::sync::mpsc::channel`) to be consumed by Bevy systems.
   - Wait on a channel for outgoing messages sent by the Bevy UI, broadcasting them to the global chat room.
4. Define a Bevy resource `ChatState` containing:
   - Our own nickname and assigned RGB color.
   - List of active peers (presence list) with nicknames and colors.
   - Chat history: vector of `(sender_nickname, text, color)`.
   - Outgoing channel sender.
   - Incoming channel receiver.
5. Create an update system that drains incoming channels and pushes events to `ChatState`.
6. Implement a full-screen egui chat window overlay:
   - Central panel with a semi-transparent black background.
   - **Top Left**: Scrollable list of active peers (Presence).
   - **Top Right**: Scrollable message history (Chat).
   - **Bottom Left**: Currently logged-in username (German nickname, colored).
   - **Bottom Right**: Text input box that broadcasts messages when Enter is pressed.

---

## 4. Verification Plan

### Automated/Compilation Checks
1. Run `cargo check --bin chat --package demo_resolution_selector_web_bevy` to verify clean compilation.
2. Fix any compiler errors/warnings.

### Manual Verification
1. Run the local python server `python3 scripts/local_server.py` on port `1973` to serve assets if needed.
2. Run the newly created binary:
   `cargo run --bin chat --package demo_resolution_selector_web_bevy`
3. Verify that:
   - The default scene is rendered behind the transparent UI overlay.
   - A German nickname (e.g. `Schadenfreude`) is generated and displayed in the bottom-left corner.
   - The presence list shows at least the local client.
   - Typing in the input box and pressing Enter posts the message to the chat history.
   - Launching a second instance shows both clients in the presence list and allows exchanging messages.
