# Rust docstrings in `net_crackpipe` — gotchas

Notes from adding `missing_docs` documentation across the crate. Intended for a less capable model repeating this work.

## 1. Do not delete `impl` lines

The original failure in `signed_message.rs` was caused by replacing:

```rust
impl SignedMessage {
    /// ...
    pub fn verify_and_decode(...)
```

with:

```rust
    /// ...
    pub fn verify_and_decode(...)
```

Methods inside an `impl` block **must** keep the `impl Type {` opener. Without it, rustc reports `unexpected closing delimiter: }`.

**Check:** after edits, `rg '^impl ' src/signed_message.rs` should still list every `impl` that existed before.

## 2. Verify with `cargo build` after every file (or small batch)

`missing_docs` is enabled at the workspace level (`missing_docs = "warn"` in the root `Cargo.toml`). The build will succeed even with hundreds of doc warnings, so you must read compiler output — not just exit code.

Useful one-liners:

```bash
cd rust_pkg/net_crackpipe
cargo build 2>&1 | rg "missing documentation" | wc -l
cargo build 2>&1 | rg "^  --> " | sed 's/.*src\///' | cut -d: -f1 | sort | uniq -c
```

## 3. Restrict diffs to doc comments only

After each batch:

```bash
git diff -- rust_pkg/net_crackpipe/src/
```

Acceptable changes:
- Added `///` or `//!` lines
- Multiline enum variant formatting **only** when required to document struct-like variant fields

Not acceptable:
- Removed `impl`, `fn`, `pub`, or attribute lines
- Logic, signature, or visibility changes
- Reordering imports or reformatting unrelated code

Compare both staged and unstaged diffs (`git diff --cached` and `git diff`) — earlier work may already be staged.

## 4. What needs documenting

`missing_docs` fires on **public** items without docs:

| Item | Doc location |
|------|----------------|
| `pub mod` | `///` immediately above the mod line (or `//!` at file top) |
| `pub struct` / `enum` / `trait` | `///` above the item (often above `#[derive]`) |
| `pub` fields | `///` on each field |
| enum variants | `///` on each variant; named fields in variants also need field docs |
| associated types / consts / fns | `///` directly above each |
| trait methods | `///` on each method in the trait definition |

Private items, `pub(crate)` modules, and `#[allow(missing_docs)]` are out of scope unless the build still warns.

## 5. Placement relative to attributes

- **Structs/enums:** prefer `///` **above** `#[derive(...)]`, matching `room_raw.rs` / `chat_presence.rs`.
- **`#[async_trait]` traits:** doc comment can sit immediately above `#[async_trait]` or above the `trait` line; both compile. Stay consistent within a file.
- **Constants inside `impl`:** doc goes directly above the `const` or `fn`, **not** in place of the `impl` line.

## 6. Work in batches (~7 files)

For sub-agents or parallel work, assign **~7 files per batch**, then rebuild:

1. `signed_message.rs`, `echo.rs`, `main_node.rs`, `network_manager.rs`, `chat_presence.rs`, `chat_ticket.rs`, `direct_message.rs`
2. `global_chat.rs`, `room_raw.rs`
3. `chat_controller.rs` (dense — many traits/methods; fine as a single-file batch)

Larger batches increase the risk of accidental non-doc edits and make review harder.

## 7. File itinerary (net_crackpipe `src/`)

| File | Status / notes |
|------|----------------|
| `lib.rs` | Crate + module docs, `timestamp_micros`, `datetime_now` |
| `signed_message.rs` | Traits, structs, wire types — **watch `impl` lines** |
| `echo.rs` | Struct + `ALPN`, `new`, `ProtocolHandler::accept` |
| `global_matchmaker.rs` | Large `impl GlobalMatchmaker` — many methods |
| `main_node.rs` | `spawn`, accessors, `shutdown`, `join_chat` |
| `network_manager.rs` | `init`, `join_room`, accessors, `shutdown` |
| `sleep.rs` | `SleepManager` methods |
| `user_identity.rs` | `UserIdentity`, `UserIdentitySecrets`, `NodeIdentity` |
| `chat/mod.rs` | Submodule docs only |
| `chat/chat_const.rs` | Already documented constants |
| `chat/chat_controller.rs` | Many traits — second-largest batch |
| `chat/chat_presence.rs` | Presence list types + `ChatPresence` API |
| `chat/chat_ticket.rs` | `ChatTicket` + constructor |
| `chat/direct_message.rs` | ALPN, protocol struct, send API |
| `chat/global_chat.rs` | Room type, presence, message enums |
| `chat/room_raw.rs` | `GossipChatRoom` |
| `_bootstrap_keys.rs` | Private — no `missing_docs` |
| `_random_word.rs` | `pub(crate)` — no `missing_docs` |

## 8. Sub-agent handoff checklist

Before marking done:

- [ ] `cd rust_pkg/net_crackpipe && cargo build` — **0** `missing documentation` lines
- [ ] `rg '^impl '` on any file where methods were documented
- [ ] `git diff` shows only `///` / `//!` additions (minor enum layout ok)
- [ ] No duplicate doc comments on items already documented in staged changes

## 9. Common false “done” states

- Build passes but warnings remain (warnings ≠ errors).
- Sub-agent reports “0 warnings in these files” while other files still warn — always check **whole crate** count.
- Partial docs on a struct but fields still warn — every `pub` field needs its own `///`.
- Two `ChatMessage` enums exist (`signed_message.rs` and `chat_controller.rs`) — document both if both are public.
