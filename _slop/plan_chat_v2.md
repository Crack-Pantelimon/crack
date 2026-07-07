# Chat V2 Fix Plan (Revised): Bootstrap Discovery 404 Errors

## Problem Statement

When running `chat.rs`, the GlobalMatchmaker fails to connect to any bootstrap nodes. The pkarr
discovery service returns 404 for 4 of 5 bootstrap node IDs, and the one that resolves
(`10f61de4e5`) has a stale record with no live process behind it.

## Root Cause Analysis

### Code Diff: Zero Meaningful Differences

All `.rs` files between `_slop/examples/Sparganothis-v2/protocol/src/` and
`packages/net_crackpipe/src/` are functionally identical. The only changes are:
- `game::timestamp::get_timestamp_now_ms` replaced with inline function (same logic)
- `ServerInfo` moved from `api::api_method_macros` to inline in `global_chat.rs` (same fields)
- Module visibility changed from `pub(crate)` to `pub` (no behavioral impact)

**The code is a clean clone. The bug is NOT a code difference.**

### The Original Code Does NOT Add NodeAddr Directly

No — the original Sparganothis code does NOT inject `NodeAddr` for its own bootstrap. The
`spawn_bootstrap_endpoint()` function in the original is byte-for-byte identical to ours:

1. Spawn a `MainNode` with one of the 5 hardcoded bootstrap keys
2. Store it in `inner.bootstrap_main_node`
3. Call `self.connect_to_bootstrap(false)` — which queries pkarr for ALL 5 bootstrap keys
4. For the key we just spawned, pkarr has no record yet → 404

**The original code has the same latent bug.** It never manifests because:

- The Sparganothis deployment always has a **server running** (`start_server.sh`)
- The server calls `GlobalMatchmaker::new()` which spawns a bootstrap and publishes to pkarr
- By the time a client connects, the server's bootstrap key is already in pkarr
- So `connect_to_bootstrap(true)` succeeds on the FIRST try in the `new_try_once` flow
- The `spawn_bootstrap_endpoint()` fallback path is never exercised in isolation

**Our situation**: we have NO Sparganothis server running. We are the first and only node.
The fallback path runs, and it fails because of the pkarr race condition.

### What the 404 Actually Means

The pkarr server at `https://net.sparganothis.org/pkarr` stores node_id→relay_url records.
When a node publishes its info (`PkarrPublisher`), it does a PUT. When another node wants to
find it (`PkarrResolver`), it does a GET.

- **404** = "this node ID has never published a record here (or it expired)"
- This is **normal** for bootstrap keys that no one is currently running

From the trace logs:

| Bootstrap Node (short) | Pkarr Resolve | Why |
|------------------------|--------------|-----|
| `10f61de4e5` | Found (stale) | Some past session published this; no live process |
| `7d4f0da52b` | 404 | Never published |
| `317e7c276f` | 404 | Never published |
| `cb717152a5` | 404 | Never published |
| `ee24a3dca8` | 404 | Never published |

## Proposed Fix

Since the original code doesn't handle the "solo first node" case, we need to add it.
The fix follows the exact same patterns already used in the codebase.

### Fix: Inject `NodeAddr` + Poll Relay Readiness in `spawn_bootstrap_endpoint()`

After spawning the bootstrap `MainNode` and storing it in `inner.bootstrap_main_node`,
we need to:

1. **Wait for the bootstrap endpoint to establish its relay connection** — poll
   `endpoint.home_relay()` every 100ms using `_crack_utils::sleep_ms` until it returns
   `Some(relay_url)` (with a reasonable timeout)
2. **Add the bootstrap's `NodeAddr` directly to our own endpoint** using
   `own_ep.add_node_addr()` — this bypasses pkarr resolution for our own bootstrap, which is
   the standard iroh pattern for connecting to a known-local peer

This is the minimal fix that makes the solo node case work while preserving the exact same
behavior for the existing case (where foreign bootstrap nodes are already online).

### Code Changes

#### `packages/net_crackpipe/src/global_matchmaker.rs`

In `spawn_bootstrap_endpoint()`, between storing the bootstrap node and calling
`connect_to_bootstrap(false)`:

```rust
pub async fn spawn_bootstrap_endpoint(&self) -> Result<bool> {
    // ... existing code: pick boostrap_idx, create bootstrap_key ...

    let bootstrap_endpoint = MainNode::spawn(/* ... */).await?;
    let bs_node_id = bootstrap_key.public();
    {
        let mut inner = self.inner.write().await;
        inner.bootstrap_main_node = Some(bootstrap_endpoint);
    }

    // --- NEW: wait for bootstrap relay + inject NodeAddr ---

    // Poll until the bootstrap endpoint has connected to its relay,
    // then inject its address directly into our own endpoint's node map
    // so connect_to_bootstrap() can reach it without pkarr resolution.
    let max_attempts = 30; // 30 * 100ms = 3s max
    let mut relay_url = None;
    for _attempt in 0..max_attempts {
        {
            let inner = self.inner.read().await;
            if let Some(ref bs_node) = inner.bootstrap_main_node {
                if let Some(url) = bs_node.endpoint().home_relay() {
                    relay_url = Some(url.0.clone());
                    break;
                }
            }
        }
        _crack_utils::sleep_ms(100).await;
    }

    if let Some(ref url) = relay_url {
        if let Some(own_ep) = self.own_endpoint().await {
            let addr = iroh::NodeAddr::new(bs_node_id)
                .with_relay_url(url.clone());
            let _ = own_ep.add_node_addr(addr);
            info!("Injected bootstrap node {} relay={} into own endpoint", 
                  bs_node_id.fmt_short(), url);
        }
    } else {
        warn!("Bootstrap endpoint did not connect to relay within timeout");
    }

    // --- END NEW ---

    info!("Connecting to own bootstrap endpoint");
    self.connect_to_bootstrap(false).await?;
    info!("Successfully connected to own bootstrap endpoint");
    self.check_spawned_bootstrap_is_unique().await
}
```

#### `packages/net_crackpipe/Cargo.toml`

Add `_crack_utils` dependency (for `sleep_ms`):
```toml
_crack_utils = { path = "../_crack_utils" }
```

#### `packages/net_crackpipe/src/global_matchmaker.rs` (imports)

Add at the top:
```rust
// (no new import needed — _crack_utils::sleep_ms is called with full path)
```

### No Other Changes Needed

- `chat.rs` binary — unchanged
- `main_node.rs` — unchanged  
- `echo.rs` — unchanged
- All chat modules — unchanged
- Retry logic in `GlobalMatchmaker::new()` — unchanged (3 attempts, already correct)
- Periodic task logic — unchanged

### Why This Will Work

1. **`endpoint.home_relay()`** returns `Some(url)` once the magicsock actor has connected to
   the relay and set `home_relay`. From the trace logs, this takes ~500-800ms. Polling at
   100ms intervals with a 3s timeout is conservative enough.

2. **`own_ep.add_node_addr(addr)`** is the standard iroh API for telling an endpoint "I know
   where this node is." After this call, `endpoint.connect(bs_node_id, Echo::ALPN)` will
   route through the relay directly without needing pkarr discovery.

3. **The Echo handshake** between our own endpoint and the bootstrap endpoint will work
   because both endpoints are connected to the same relay (`net2.sparganothis.org`). The
   relay acts as a rendezvous point.

4. **When foreign bootstrap nodes ARE online** (the normal Sparganothis case),
   `connect_to_bootstrap(true)` succeeds on line 293 and `spawn_bootstrap_endpoint()` is
   never called — so our changes have zero impact on the existing flow.

## Testing Plan

1. `RUST_LOG=iroh::discovery=debug,net_crackpipe=info cargo run --bin chat`
2. Verify: "Injected bootstrap node ... into own endpoint" appears in logs
3. Verify: "added connection to bootstrap node #N" appears
4. Verify: no "failed to create global matchmaker" error
5. Verify: chat UI reaches "Connected!" status
6. Run two instances simultaneously to verify gossip peer discovery works
