## 2026-06-27T10:32:32Z
You are a Worker for the Bevy Mission System E2E Test Setup (Milestone 1).
Your working directory is /home/vasile/.gemini/antigravity/scratch/crack/.agents/worker_m1/.
Your task is to implement the E2E Test infrastructure and test cases (Tiers 1-4) for the mission trigger and state machine system.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

Please execute the following steps:

1. Update Crate Visibility in Bevy Client:
   - In `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/lib.rs`, change `pub(crate) mod plugins;` to `pub mod plugins;` to expose plugins for testing.
   - Also check `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/mod.rs` and add `pub mod mission_plugin;`.

2. Create Mission Plugin Stubs:
   - Create `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/mission_plugin/` directory.
   - Implement `mod.rs`, `config.rs`, `state.rs`, `trigger.rs`, `ui.rs`, and `db.rs` with the basic stubs and data structures defined in `/home/vasile/.gemini/antigravity/scratch/crack/.agents/orchestrator_e2e/explorer_report.md`. Ensure that `MissionPlugin` is a Bevy `Plugin` that registers `MissionState` and `MissionList` resources and adds the `check_mission_triggers` system.
   - Add `CarMarker` component in `trigger.rs`.

3. Configure Dev-Dependencies:
   - In `/home/vasile/.gemini/antigravity/scratch/crack/Cargo.toml` (the workspace root Cargo.toml), add `demo_resolution_selector_web_bevy` to the `[dev-dependencies]` section:
     ```toml
     [dev-dependencies]
     demo_resolution_selector_web_bevy = { path = "crack_demo/demo_resolution_selector_web_bevy" }
     ```

4. Create E2E Integration Test Cases:
   - Create `/home/vasile/.gemini/antigravity/scratch/crack/tests/mission_integration_tests.rs`.
   - Implement a test runner harness using a headless Bevy `App` (with `MinimalPlugins`).
   - Implement exactly 60 integration test cases divided across four tiers as follows:
     - Tier 1: `test_tier1_feat1_case1` through `test_tier1_feat5_case5` (25 tests) - checking happy path behavior for:
       - Feature 1: Configuration Loading & DAG Parsing
       - Feature 2: Trigger Detection & Coordinate Mapping
       - Feature 3: State Machine Transitions
       - Feature 4: HUD / egui Overlay Updates
       - Feature 5: SQLite Database Persistence (using `storage_crackhouse::impl_rusqulite::CONN` connection swapping)
     - Tier 2: `test_tier2_feat1_case1` through `test_tier2_feat5_case5` (25 tests) - checking boundary conditions and error handling.
     - Tier 3: `test_tier3_case1` through `test_tier3_case5` (5 tests) - checking cross-feature combinations (pairwise interactions).
     - Tier 4: `test_tier4_case1` through `test_tier4_case5` (5 tests) - checking real-world gameplay workflows.
   - Ensure the tests compile successfully. Since the plugin logic is not yet fully implemented, the tests should run and fail (e.g. using `todo!()` or returning stub values that fail the assertions). This is expected behavior for Milestone 1.

5. Create Documents at Project Root:
   - Create `/home/vasile/.gemini/antigravity/scratch/crack/TEST_INFRA.md` matching the template in the E2E Testing Track instructions.
   - Create `/home/vasile/.gemini/antigravity/scratch/crack/TEST_READY.md` listing the test command, coverage summary table, and feature checklist.

6. Run and Verify:
   - Run the integration test suite using: `cargo test --test mission_integration_tests`
   - Capture the compilation output and test run output, showing that all tests are built and run.
   - Write your verification results and handoff report to `/home/vasile/.gemini/antigravity/scratch/crack/.agents/worker_m1/handoff.md`.

7. Report back when complete.
