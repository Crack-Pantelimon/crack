# BRIEFING — 2026-06-27T12:46:00Z

## Mission
Explore Bevy E2E test setup, crate layout, stub strategy, integration test registration, and storage mocking, then write report.

## 🔒 My Identity
- Archetype: Explorer
- Roles: Teamwork explorer
- Working directory: /home/vasile/.gemini/antigravity/scratch/crack/.agents/explorer_e2e
- Original parent: 23006dcf-3105-413a-91ec-57601b63cbc8
- Milestone: Milestone 1 - Bevy Mission System E2E Test Setup

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- CODE_ONLY network mode (no external internet/HTTP access)

## Current Parent
- Conversation ID: 23006dcf-3105-413a-91ec-57601b63cbc8
- Updated: not yet

## Investigation State
- **Explored paths**:
  - `/home/vasile/.gemini/antigravity/scratch/crack/Cargo.toml`
  - `/home/vasile/.gemini/antigravity/scratch/crack/PROJECT.md`
  - `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/Cargo.toml`
  - `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/lib.rs`
  - `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/mod.rs`
  - `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/lib.rs`
  - `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/api.rs`
  - `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/impl_rusqulite.rs`
  - `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/models.rs`
  - `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/types.rs`
- **Key findings**:
  1. The `demo_resolution_selector_web_bevy` crate exports `lib.rs` but marks `plugins` as `pub(crate) mod plugins;`. To let integration tests access the plugin, `plugins` or `mission_plugin` must be exposed publicly.
  2. Integration tests should be placed in `crack_demo/demo_resolution_selector_web_bevy/tests/mission_integration_tests.rs`.
  3. `storage_crackhouse` has a public lazy_static `CONN: Arc<Mutex<Result<Connection>>>` which can be overridden in tests to target an in-memory database.
  4. The target table name matches the `declare_model_group!` macro convention in `storage_crackhouse`.
- **Unexplored areas**:
  - The actual integration test assertions (until implementation starts)

## Key Decisions Made
- Recommended exposing `mission_plugin` by making `plugins` module public or re-exporting it.
- Recommended mock harness utilizing `CONN` lock replacement.

## Artifact Index
- /home/vasile/.gemini/antigravity/scratch/crack/.agents/orchestrator_e2e/explorer_report.md — Detailed explorer findings and recommendations
