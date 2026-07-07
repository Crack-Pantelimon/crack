# Plan: Integrate crack worker-RPC into the bevy game (game_logic crate + CrackPlugin)

## Context

The game (`crack_demo/demo_resolution_selector_web_bevy`) currently does all heavy data work synchronously on the main thread: parquet map-manifest parsing (`check_and_parse_parquet`), OSM geojson fetch/parse/projection (`geojson.rs`), and the LOD refinement core (`recompute_lod_mark_changes`, self-logged when >12ms). The workspace already contains the "crack" library (typed async RPC to a web worker on wasm / an in-process tokio thread on native), but the game doesn't use it. Goal: move network I/O and CPU-heavy work into the crack worker, driven from bevy via async tasks, with identical behavior on web and desktop.

User decisions: **reqwest** for worker HTTP (works wasm+native); OSM call does **fetch + parse + projection** in the worker (it has the cached map tree); model group is a **minimal placeholder** (one kv table) to exercise registration/migration.

## Verified facts the design relies on

- `declare_api_group2!{ Group, [(Method, Arg, Ret), ...] }` + `implement_api_group2!{ Group, [(Method, fn_path), ...] }`; args/returns are postcard-serialized → need serde derives. Impls implement a foreign trait on the group type ⇒ **decls and impls must be in the same crate** (impls feature-gated).
- `ApiClient` is `#[derive(Clone)]`, Arc-based ([api_client.rs](packages/api_asscrack/src/api/api_client.rs)). `ApiClient::new` + `ThreadWorkerFactory::load_worker` spawn via `n0_future::task::spawn` = `tokio::spawn` on native ⇒ **must run inside a tokio runtime on native**. `client.call::<M>()` itself needs no runtime context (mpsc/oneshot only) ⇒ callable from bevy `AsyncComputeTaskPool` tasks.
- Method impl futures must be **Send**; reqwest's wasm future is !Send ⇒ bridge via `wasm_bindgen_futures::spawn_local` + `tokio::sync::oneshot` in an http helper.
- bevy_math 0.19 → **glam 0.32.1**; game_logic pins `glam = "0.32"` with serde ⇒ its `Vec3` IS bevy's `Vec3`, zero conversion. reqwest 0.12 already in Cargo.lock; the game's `parquet = "53"` already compiles for wasm32.
- Web glue already ships: `index.html` loads `crack2-client.js`; `build_worker.sh` copies the worker pkg into the game's `public/pkg_web_serviceworker`. Worker must be rebuilt after adding groups.
- `run_migrate_tables` (storage_crackhouse) is invoked nowhere yet — we call it via a new api method.

## Step 1 — New crate `crack_demo/game_logic`

Add to root `Cargo.toml` `[workspace] members`.

`Cargo.toml`: deps `api_asscrack`, `_crack_utils` (by path), `glam = { version = "0.32", features = ["serde"] }` (comment: must track bevy_math's glam), `tokio` (default-features off, `sync` only — wasm-safe), serde/anyhow/tracing. Feature **`worker`** enables optional deps: `storage_crackhouse` (path), `parquet = "53"` (no default features, `snap`), `bytes`, `serde_json`, `reqwest = { version = "0.12", default-features = false, features = ["rustls-tls"] }`. wasm target dep: `wasm-bindgen-futures`. **No bevy dependency, ever** (worker build).

Modules:
- `map.rs` — canonical serde types moved from `map_plugin/mod.rs`: `MapTileAssetId`, `MapTreeNodePath`, `BBox`, `MapTreeAssetInfo`, `MapTreeNodeInfo`, plus `MapTreeData { assets, all_nodes, children, parents, bbox, roots }` (= `MapTree` minus `parsed`) and `MapManifestResult { tree: MapTreeData }`.
- `geo.rs` — pure math moved from `geojson.rs`: `GeoBBox`, `octant_path_to_geobbox`, `find_tile_for_lat_lon` (takes `&MapTreeData`), `project_point`, `lat_lon_to_ecef`, `get_enu_rotation_matrix`, `lat_lon_to_bevy`, `parse_bbox_from_txt`, `ProjectionRef { ref_point, rot_matrix }` (replaces `GeoJsonCoordinatesResource`).
- `osm.rs` — serde types: `FeatureGeometry`, `GeoJsonFeature` (same field names/pubness as today — traffic + traffic_test depend on them), `RawGeoJsonFeature`/`RawFeatureGeometry`, `OsmDataResult { categories: BTreeMap<String, Vec<GeoJsonFeature>> }`.
- `lod.rs` — `LodComputeRequest { spawned_nodes, reference_points (camera already pushed in), lod_budget, max_lod, tiles_per_diagonal }`, `LodComputeResponse { split_requests, merge_requests }`, and pure `compute_lod_changes(&MapTreeData, &LodComputeRequest) -> LodComputeResponse` = [map_lod.rs:217-412](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs#L217-L412) heap core + `compute_distance_to_aabb`, minus ECS gating. Replace egui's `OrderedFloat` with a local `Score(f32)` Ord wrapper (`total_cmp`). Keep the `dt > 12ms` timing log here via `_crack_utils::get_timestamp_now_ms`.
- `api.rs` — always compiled:
  ```rust
  pub struct FetchArgs { pub base_url: String }
  declare_api_group2! { GameLogicApiGroup, [
      (FetchMapManifest, FetchArgs, crate::map::MapManifestResult),
      (FetchOsmData, FetchArgs, crate::osm::OsmDataResult),
      (ComputeLodChanges, crate::lod::LodComputeRequest, crate::lod::LodComputeResponse),
      (RunGameMigrations, (), ()),
  ] }
  ```
- `worker/` (cfg feature `worker`):
  - `mod.rs` — `implement_api_group2!` wiring the four fns; `compute_lod_changes_api` reads the manifest cache, errors with "LOD requested before manifest was fetched" if empty.
  - `http.rs` — `http_get_bytes/http_get_text`: native = plain `reqwest::get(...).error_for_status()?.bytes()`; wasm = spawn_local + oneshot bridge (Send-safe).
  - `manifest_impl.rs` — `parse_tree_nodes` moved verbatim from [map_metadata_parquet.rs](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_metadata_parquet.rs) (returns Result), `build_map_tree` = the grouping/tree/bbox logic from `check_and_parse_parquet` (lines ~210-332; camera + MapLODState seeding do NOT move). Cache:
    ```rust
    static MANIFEST_CACHE: tokio::sync::RwLock<Option<Arc<MapTreeData>>> = RwLock::const_new(None);
    ```
    `get_or_fetch_manifest(base_url)`: read-check, then hold the **write lock across the fetch** (dedups concurrent callers), double-check, fetch `{base}/3d_data_v2/data_out/manifest.parquet`, parse, store. `fetch_map_manifest` returns a clone.
  - `osm_impl.rs` — same RwLock pattern (`OSM_CACHE`). Impl: `get_or_fetch_manifest` first (self-sufficient if game ordering ever changes), fetch `zone-bbox.txt` → `ProjectionRef`, fetch `_list.txt` → fetch each geojson, serde_json parse → `parse_raw_geojson_feature` (moved verbatim) → projection pass moved verbatim from `project_geojson_coordinates`. Preserve exact URL format strings.
  - `models.rs` — `declare_model_group! { GameLogicModels, #[db_table(pk(id))] pub struct GameKvEntry { pub id: i64, pub val: Option<String> } }` + `run_game_migrations(())` calling `run_migrate_tables`. Migrations run via the **RunGameMigrations api call during client init** (on web this is the only ordering safe w.r.t. the JS-glue sqlite VFS install).

## Step 2 — Register groups in both workers

- [web_worker/src/lib.rs](crack_demo/web_worker/src/lib.rs): add `game_logic = { path = "../game_logic", features = ["worker"] }`; add `Arc::new(game_logic::api::GameLogicApiGroup)` to the `make_api_mapping` vec.
- `crack_demo/thread_worker`: **new `src/lib.rs`** exposing `make_registered_mapping()` (WorkerApiGroup2 + StorageCrackhouseApiGroup + GameLogicApiGroup) and `spawn_in_process_worker() -> anyhow::Result<WorkerPipe>` (ThreadWorkerFactory; must be called inside tokio). Add game_logic dep with `worker` feature. `main.rs` REPL switches to `make_registered_mapping()` and keeps working.

## Step 3 — Game crate deps + CrackPlugin

[game Cargo.toml](crack_demo/demo_resolution_selector_web_bevy/Cargo.toml): remove `parquet`, `bytes`; add `game_logic` (path, **no** `worker` feature — wasm game build stays free of reqwest/parquet/sqlite) and `api_asscrack` (path). Target-gated:
```toml
[target.'cfg(not(target_family = "wasm"))'.dependencies]
thread_worker = { path = "../thread_worker" }   # embeds worker in-process (pulls game_logic/worker)
tokio = { version = "1", default-features = false, features = ["rt-multi-thread", "sync"] }
[target.'cfg(target_family = "wasm")'.dependencies]
web_serviceworker_crackloader = { path = "../../packages/web_serviceworker_crackloader" }
wasm-bindgen-futures = { workspace = true }
```
Factory selection by `#[cfg(target_family = "wasm")]` (transport is target-specific; the `web` feature stays a URL/canvas concern — Trunk always enables it anyway).

New `src/plugins/crack_plugin/` (`mod.rs`, `manifest_flow.rs`, `osm_flow.rs`; LOD spawn/poll systems stay in `map_lod.rs` to avoid visibility churn, registered by CrackPlugin):
- Resources: `CrackClient(ApiClient)` (Clone), `CrackClientSlot(Arc<Mutex<Option<anyhow::Result<ApiClient>>>>)`, native-only `CrackRuntime(Arc<tokio::runtime::Runtime>)`, `CrackTasks { manifest, osm, lod: Option<Task<...>> }`.
- `Startup: start_crack_client_init` — shared `init_client()` future: build pipe (wasm: `WebWorkerFactory{}.load_worker()`, mirrors [web_frontend/src/crack.rs](crack_demo/web_frontend/src/crack.rs); native: `thread_worker::spawn_in_process_worker()`), `ApiClient::new`, `WorkerPing` handshake, then `RunGameMigrations`. Driven: **native** — build multi-thread runtime with `enable_all()`, store as resource (lives for app lifetime; hosts worker dispatcher + demux task), `runtime.spawn` the init writing into the slot; **wasm** — `wasm_bindgen_futures::spawn_local` (needs `window`, so not a task pool).
- `Update: install_crack_client` (moves slot Ok → `CrackClient` resource; Err → log + retrigger), then `(poll_manifest, spawn_manifest, poll_osm, spawn_osm, poll_lod, spawn_lod).chain().run_if(resource_exists::<CrackClient>)`.
- Per-call tasks: `AsyncComputeTaskPool::get().spawn(async move { client.call::<M>(arg).await })`, polled with `block_on(poll_once(&mut task))`. Register CrackPlugin in `main_game_plugin.rs` before MapPlugin.

## Step 4 — Manifest flow (replaces AssetServer parquet path)

- `spawn_manifest_task`: if `!map_tree.parsed && task.is_none()` → call `FetchMapManifest { base_url: DATA_BASE_URL }`.
- `poll_manifest_task`: on Ok — move tree fields into `MapTree`, place camera (exact code from map_metadata_parquet.rs:312-322), seed `MapLODState` exactly as today (budget = roots' asset count + 320, random 0.1–0.2s timer, max_lod 24, tiles_per_diagonal 1.30), set `parsed = true`. On Err — log + clear task (auto-retry). Downstream (`spawn_root_map_tiles` on `is_changed`, `check_map_loaded_status` → states) untouched.
- **Delete `map_metadata_parquet.rs` entirely** + its MapPlugin registrations (`init_parquet_handles`, `check_and_parse_parquet`, `ParquetAsset(Loader)`).

## Step 5 — OSM flow

- `spawn_osm_task`: gate exactly like `trigger_geojson_loading` (state == `MapFinished` && `!geojson_loading_started`); set the flag, call `FetchOsmData`.
- `poll_osm_task`: on Ok — `database.categories = result.categories; database.parsed = true;` + per-category logs. Existing `update_geojson_loading_finished` flips `OsmFinished` + tooltip — unchanged. Traffic `build_road_graph` unchanged.
- [geojson.rs](crack_demo/demo_resolution_selector_web_bevy/src/plugins/geojson.rs) **delete**: `GeoJsonTextAsset(+Loader)`, `GeoJsonHandles`, `GeoJsonLoaderState`, `trigger_geojson_loading`, `check_geojson_loading`, `parse_raw_geojson_feature`, `project_geojson_coordinates`, `Raw*` types, `raw_categories`/`files_loaded` fields, `GeoJsonCoordinatesResource` + all the geo math fns (verified geojson.rs-internal; moved to game_logic). **Keep**: `GeoJsonDatabase { categories, parsed }`, search/selection/loading-status/tooltip/overlay resources, all UI/label/gizmo/bus systems, `query_point_ground_y` (used by traffic/spawn.rs). **Re-export** `pub use game_logic::osm::{FeatureGeometry, GeoJsonFeature};` so `traffic/road_graph.rs` and `bin/traffic_test.rs` compile unchanged.

## Step 6 — Map types + LOD split

- [map_plugin/mod.rs](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/mod.rs): replace local `BBox`/`MapTileAssetId`/`MapTreeNodePath`/`MapTreeAssetInfo`/`MapTreeNodeInfo` with `pub use game_logic::map::...`; `MapTree` stays a game-side Resource with the same fields + `parsed`. `MapLODState` unchanged.
- [map_lod.rs](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs): `recompute_lod_mark_changes` (193-413) → `spawn_lod_task` + `poll_lod_task` pair:
  - `spawn_lod_task` gating in order: (1) **single-in-flight**: bail if `tasks.lod.is_some()`; (2) bail while `q_merge`/`q_split`/`q_pending` non-empty or `TileSwapRequests` pending (exact current condition); (3) bail if tree/tiles empty; (4) **memoize with the same `Local<Option<(BTreeSet<MapTreeNodePath>, Vec<Vec3>, u32)>>` triple** (spawned set, refs incl. camera, budget) — bail on unchanged. Then snapshot inputs → call `ComputeLodChanges`.
  - `poll_lod_task`: on Ok apply `split_requests`/`merge_requests` to `TileSwapRequests` **unconditionally** (safe: gating guarantees the swap pipeline was idle while in flight, matching old same-frame-snapshot semantics; memoization re-fires if refs drifted). On Err: log, clear task, clear `Local` (retry). Poll before spawn in the chain.
  - Rest of the swap pipeline (`start_tile_swap_requests`, `do_split/merge_requests`, `reveal_pending_tiles`, `check_map_loaded_status`) untouched.

## Verification

Build matrix:
1. `cargo check -p game_logic` and `--features worker`, plus `--features worker --target wasm32-unknown-unknown` (proves reqwest-wasm Send-bridge + parquet on wasm)
2. `cargo check -p web_worker --target wasm32-unknown-unknown`; then `./build_worker.sh` (**required** — refreshes the shipped worker pkg in the game's `public/`)
3. `cargo check -p thread_worker` (lib+bin), `cargo check -p demo_resolution_selector_web_bevy`, `cargo check --workspace`
4. wasm game: `trunk build` in the game dir

Runtime smoke:
5. Native (data server on 127.0.0.1:1973): `./start_game_native.sh` — expect: ping ok → migration SQL for `GameLogicModels_GameKvEntry` → manifest parse logs (now from worker impl) → camera placed → tiles spawn → "Initial map load complete" → OSM category counts → "GeoJSON loading is fully completed!" → fly around, verify splits/merges + the LOD timing log.
6. Web: `./start_game_web.sh`, console shows worker registration + same sequence; OSM overlay UI + traffic still work.
7. `cargo run -p thread_worker` REPL: `SELECT * FROM GameLogicModels_GameKvEntry;` returns empty set (migration proof).

## Risks

- **reqwest fetch in a dedicated worker (wasm)** — expected to work (fetch via global scope); if not, fallback is a ~30-line `web_sys`/`js_sys::global()` fetch confined to `worker/http.rs`.
- **glam pin coupling** to bevy's minor — future bevy upgrades need the game_logic pin bumped (loud compile error; documented in Cargo.toml).
- **Large postcard payloads** over `serde_wasm_bindgen` on web (manifest/OSM, once per session) — measure; hitch happens in the worker, not the main thread.
- Any future `ApiClient::new` outside `CrackRuntime` panics on native — all confined to `start_crack_client_init`.
