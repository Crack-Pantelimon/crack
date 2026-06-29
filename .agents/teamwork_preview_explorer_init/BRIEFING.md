# BRIEFING — 2026-06-27T10:31:00Z

## Mission
Explore the Bevy and Cargo workspace to identify build methods, asset loading patterns, player tracking, and database querying mechanics.

## 🔒 My Identity
- Archetype: Explorer
- Roles: Read-only investigation: analyze problems, synthesize findings, produce structured reports
- Working directory: /home/vasile/.gemini/antigravity/scratch/crack/.agents/teamwork_preview_explorer_init
- Original parent: 6cae2cdb-0926-4505-9be4-ddc8e015dc62
- Milestone: codebase exploration

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- No network access (CODE_ONLY network mode)

## Current Parent
- Conversation ID: 6cae2cdb-0926-4505-9be4-ddc8e015dc62
- Updated: not yet

## Investigation State
- **Explored paths**:
  - `Cargo.toml`
  - `PROJECT.md`
  - `Makefile`
  - `build_worker.sh`
  - `deploy.sh`
  - `start_game_web.sh`
  - `start_game_native.sh`
  - `crack_demo/demo_resolution_selector_web_bevy/Trunk.toml`
  - `crack_demo/demo_resolution_selector_web_bevy/src/config.rs`
  - `crack_demo/demo_resolution_selector_web_bevy/src/plugins/gta_plugin/car.rs`
  - `crack_demo/demo_resolution_selector_web_bevy/src/plugins/gta_plugin/camera.rs`
  - `crack_demo/demo_resolution_selector_web_bevy/src/plugins/gta_plugin/mod.rs`
  - `crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_metadata_parquet.rs`
  - `packages/storage_crackhouse/src/lib.rs`
  - `packages/storage_crackhouse/src/api.rs`
  - `packages/storage_crackhouse/src/impl_rusqulite.rs`
  - `packages/storage_crackhouse/src/models.rs`
  - `packages/storage_crackhouse/src/types.rs`
- **Key findings**:
  - Found that Bevy native build is run via `cargo run` and web WASM build via `trunk` (Trunk.toml).
  - Assets are loaded over HTTP using custom `AssetLoader` implementations (e.g. `ParquetAssetLoader` in `map_metadata_parquet.rs`) or directly via `AssetServer`.
  - Player's position is tracked via the `Car` component on an entity containing `Transform`, `LinearVelocity`, and `AngularVelocity` from the `avian3d` physics engine.
  - SQL queries are executed using `execute_sql2` and `execute_sql_params` from `storage_crackhouse::api`. Database files `post.db` and `post2.db` exist.
- **Unexplored areas**: None; all requested areas have been fully examined.

## Key Decisions Made
- Confirmed database table existence with sqlite3 CLI checks.

## Artifact Index
- /home/vasile/.gemini/antigravity/scratch/crack/.agents/teamwork_preview_explorer_init/handoff.md — Final investigation report
