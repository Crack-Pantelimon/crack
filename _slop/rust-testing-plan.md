# Documentation + Test Harness for all Rust crates

## Context

The workspace has 13 Rust crates (8 in `rust_pkg/`, 5 in `crack_demo/`) plus the main Bevy
game — and almost none of it is tested or documented. Only 3 crates (`_crack_utils`,
`thread_crackworker`, `storage_crackhouse`) have any tests, no crate has a `README.md`, and
the hand-written portion of each `AGENTS.md` (above the `## Auto-generated signatures` marker)
is empty. There is no per-crate test runner and no headless test path for the Bevy game.

Goal: give every crate a `README.md`, a hand-written `AGENTS.md` preamble, an executable
`test.sh`, and **one smoke test per module** (all green). Add a `make_headless_app()` to the
Bevy game plus one headless smoke test for the main game. Tie it together with a root
`test.sh`. Tests capturing the "past trouble" recorded in memory become living guards.

**Everything runs inside the dev container** `crack-dev` (repo mounted at `/workspace`):
`docker exec crack-dev bash -lc 'cd /workspace/... && <cmd>'`. Tooling: `cargo 1.97`,
`wasm-pack 0.15`, targets `wasm32-unknown-unknown` + `x86_64`, Firefox 140 ESR + geckodriver
0.37, Chromium 150 + chromedriver 150, node 22 — all in `PATH`.

**Decisions locked with the user:** (1) one smoke test per module, get the whole suite green
first; extra lifecycle tests are a follow-up. (2) Bevy: `make_headless_app()` + one smoke test
for the main game only now (side bins later). (3) wasm: `--node` for pure crates, real
`--firefox --chrome --headless` browser runs for any crate needing DOM / Workers / OPFS.

---

## Per-crate matrix

`N` = native `cargo test`; `Wn` = `wasm-pack test --node`; `Wb` = `wasm-pack test --headless
--firefox --chrome`. "Modules" = smoke-test targets.

| Crate | Platform | Modules (smoke targets) | Notes |
|---|---|---|---|
| `rust_pkg/_crack_utils` | N + Wn | lib | pure fns; has tests already |
| `rust_pkg/api_asscrack` | N + Wn | api_client, api_method_macros, api_worker_declarations, crack_worker, api_worker | ping via `make_api_mapping` |
| `rust_pkg/consensus_crackhead` | N + Wn | lib (empty) | trivial link test only |
| `rust_pkg/net_crackpipe` | N + Wb | lib pure fns, chat_const, chat_ticket, user_identity, sleep | rand→getrandom → browser; **memory guards** |
| `rust_pkg/storage_crackhouse` | N + Wb | api, impl_rusqulite, models, types | native rusqlite; wasm = OPFS → browser |
| `rust_pkg/thread_crackworker` | N | lib | has tests already |
| `rust_pkg/web_serviceworker_crackloader` | Wb | lib | Worker API → browser |
| `rust_pkg/web_serviceworker_crackslave` | Wb | lib | **build/link smoke only** (needs real worker scope) |
| `crack_demo/game_logic` | N + Wn | api, geo, glb, lod, map, network, osm, pedestrian, tile, weapon (+ visibility under `worker`) | pure/serde; no getrandom |
| `crack_demo/thread_worker` | N | lib | `spawn_in_process_worker` + `WorkerPing` |
| `crack_demo/web_worker` | Wb | lib | cdylib; registration build-smoke |
| `crack_demo/chat_cli` | N | main | bin-only; test pure `network_manager_config` |
| `crack_demo/demo_resolution_selector_web_bevy` | N | basic_app (headless) | Bevy headless smoke (main game) |

---

## Work items

### 1. Wasm test dependency
Add to root `Cargo.toml` `[workspace.dependencies]`:
`wasm-bindgen-test = "0.3"`. In each crate that runs a wasm target, add it under
`[target.'cfg(target_arch = "wasm32")'.dev-dependencies]`.
Rely on the **existing** root `.cargo/config.toml` (already sets
`getrandom_backend="wasm_js"` for `wasm32`) — do **not** re-set `RUSTFLAGS` in `test.sh`
(env RUSTFLAGS would clobber that config). Fix the two inert misnamed files
`web_serviceworker_crackslave/.cargo/cargo.toml` and
`crack_demo/web_worker/.cargo/cargo.toml` → `.cargo/config.toml` only if a wasm build fails to
pick the target (otherwise leave; `test.sh` passes the target explicitly).

### 2. Smoke tests (one per module)

**Sync dual-target pattern** (native + wasm from one test body):
```rust
#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;   // makes #[test] run under wasm
    use super::*;

    #[test]
    fn smoke_<module>() { /* construct + assert / serde round-trip */ }
}
```
**Async dual-target pattern** (worker/ping tests): keep an `async fn body()`, then
`#[cfg(not(target_arch="wasm32"))] #[tokio::test]` and
`#[cfg(target_arch="wasm32")] #[wasm_bindgen_test]` wrappers calling it.

Representative targets (not exhaustive):
- `_crack_utils`: `get_timestamp_now_ms`, `random_u32`, `sleep_ms` (extend existing tests).
- `api_asscrack` / `thread_crackworker` / `thread_worker`: reuse the canonical ping path from
  `thread_crackworker/src/lib.rs` (`make_api_mapping([WorkerApiGroup2])` +
  `ApiClient::call::<WorkerPing>(())`); `thread_worker` uses `spawn_in_process_worker()`.
- `net_crackpipe`: `timestamp_micros`, `datetime_now`, `get_relay_domain`,
  `ChatTicket::new_str_bs`, `UserIdentitySecrets::generate`. Add inline `mod test` (some
  modules are `pub(crate)`). See §5 for the memory-derived guard tests.
- `storage_crackhouse`: native `run_migrate_tables` + `execute_sql2` round-trip (extend the
  existing `test_migrate`); wasm = OPFS install + trivial query in-browser.
- `game_logic`: per always-on module — `geo::octant_path_to_geobbox`/`lat_lon_to_ecef`,
  `lod::compute_distance_to_aabb`, `network::bootstrap_topics`, serde round-trips for
  `osm/pedestrian/weapon/glb/tile/map/api`. Keep the existing `visibility` tests (run under
  `--features worker` in `test.sh`).
- `web_serviceworker_crackloader` / `web_worker`: instantiate the factory / compose
  `make_api_mapping(...)` in a `#[wasm_bindgen_test]` (browser). `crackslave`: build/link smoke
  only (a `#[wasm_bindgen_test] fn links()` that references the public fns without invoking
  worker-scope APIs).
- `chat_cli`: `#[cfg(test)]` in `main.rs` asserting `game_logic::network::network_manager_config()`
  builds.
- `consensus_crackhead`: `#[test] fn smoke() {}`.

### 3. Bevy headless infra (`demo_resolution_selector_web_bevy`)

In `src/basic_app.rs`, add `make_headless_app(title: &str) -> App` beside `make_basic_app`.
Mirror `make_basic_app` (memory asset source, `AssetPlugin` meta-check Never, `LogPlugin`,
`ClearColor`) but build `DefaultPlugins` for headless — the spacejwz/spawn pattern, **not**
`MinimalPlugins`, because the game plugins need render types + `AssetServer`:
```rust
DefaultPlugins.build()
    .set(WindowPlugin { primary_window: None,
        exit_condition: bevy::window::ExitCondition::DontExit, ..default() })
    .set(RenderPlugin { render_creation:
        WgpuSettings { backends: None, ..default() }.into(), ..default() })
    .disable::<bevy::winit::WinitPlugin>()
```
Do **not** insert `WinitSettings` (needs an event loop). Then in `src/main.rs` add:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn main_game_survives_ten_headless_frames() {
        let mut app = crate::basic_app::make_headless_app("Pantelimon");
        app.add_plugins(crate::main_game_plugin::MainGamePlugin);
        for _ in 0..10 { app.update(); }
        let n = app.world_mut()
            .query::<&bevy::render::camera::Camera>().iter(app.world()).count();
        assert!(n >= 1, "expected >=1 camera, got {n}");
    }
}
```
(`main.rs` currently has only `fn main()` — the test module lives there; the bin is part of
the lib crate so `crate::` paths resolve.) **Risk:** `bevy_egui`/render startup systems may
panic without a real window/GPU. Mitigation ladder during execution: keep `RenderPlugin`
with `backends: None`; if a specific plugin still panics, spawn a fake
`world_mut().spawn(Window::default())` (per `how_to_test_apps.rs`); last resort mark the test
`#[ignore]` with a documented reason. This test is **native-only** (no wasm).

### 4. `test.sh` scripts

**Per-crate** `test.sh` (executable, `chmod +x`), e.g. pure crate:
```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
echo "== <crate>: <one-line cargo description> =="
cargo test
wasm-pack test --node          # or: --headless --firefox --chrome  (DOM/OPFS/worker)
```
For browser crates, export `GECKODRIVER=/usr/local/bin/geckodriver` and
`CHROMEDRIVER=/usr/bin/chromedriver` so wasm-pack reuses the installed drivers offline (and
point chromedriver at `/usr/bin/chromium` if it can't auto-locate Chrome). Native-only crates
omit the wasm line; wasm-only crates omit `cargo test`. `game_logic` adds
`cargo test --features worker` for the visibility tests.

**Root** `/workspace/test.sh` (executable): `cd "$(dirname "$0")"`, then run each crate's
`test.sh` in dependency order (`_crack_utils` → `api_asscrack` → workers → `net_crackpipe` →
`storage_crackhouse` → `game_logic` → demo/game), stopping at the first failure with a clear
message, else print `ALL TESTS OK`. This supersedes the current minimal `start_test.sh`
(leave that file or repoint it at `test.sh`).

### 5. Docs (`README.md` + `AGENTS.md` preamble) per crate

Create `README.md` (none exist) and fill the empty region **above** the
`## Auto-generated signatures` marker in each `AGENTS.md` (never touch below it — sigmap
regenerates it). 1–2 paragraphs: what the crate is, how to use it (key public API), gotchas /
patterns from the code, and a pointer to its `test.sh`. Fold in the recorded "past trouble":
- `net_crackpipe`: the **[[chatcontroller-lifetime-footgun]]** (a `ChatController` must be
  owned for the room's whole life; sender/receiver clones don't keep dispatch/presence alive)
  and **[[multiplayer-gossip-join-peers]]** (high-rate gossip rooms need `join_peers`, not
  bootstrap-only relay). Add a native smoke/regression test asserting the controller keeps the
  room's tasks alive after cloning a sender — a living guard for the footgun.
- `demo_resolution_selector_web_bevy` AGENTS.md already has hand-written guidance (bevy 0.19
  `despawn()`, no `std::Instant` on wasm, no threads → use worker API routes) — preserve it and
  append the headless-test note + the **[[car-physics-hover-model]]** invariant (ground response
  stays in clamped velocity space; no spring forces / hit normals / Transform teleports).
- Cross-cutting gotchas to document where relevant: getrandom `wasm_js` backend is wired via
  the **root** `.cargo/config.toml` (per-crate `.cargo/cargo.toml` files are misnamed and
  inert); `net_crackpipe` is edition 2021 while the rest are 2024; `web_serviceworker_crackslave`
  can only be link-smoke-tested (needs a real dedicated-worker scope).

---

## Verification

Run inside the container, bottom-up:
1. Per crate: `docker exec crack-dev bash -lc '/workspace/rust_pkg/<crate>/test.sh'` — iterate
   until green. Start with pure native crates (`_crack_utils`, `api_asscrack`,
   `thread_crackworker`, `game_logic`), then worker/ping, then browser wasm
   (`storage_crackhouse`, `net_crackpipe`, service-worker crates), then the Bevy headless test.
2. Confirm the first browser run actually launches Firefox + Chromium headless (watch for
   driver/`chromium` path issues; fall back to the `GECKODRIVER`/`CHROMEDRIVER`/chrome-binary
   env vars noted in §4).
3. Full sweep: `docker exec crack-dev bash -lc '/workspace/test.sh'` → must end `ALL TESTS OK`.
4. Docs: verify each `AGENTS.md` edit sits above the `## Auto-generated signatures` marker and
   every crate has a `README.md`.
