# Handoff Report - Explorer E2E

This report provides the required 5-component handoff details for the Explorer agent in the E2E Test Setup track.

---

## 1. Observation
*   **Private Module Visibility**: In `/home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/lib.rs` (lines 3-4):
    ```rust
    pub(crate) mod plugins;
    pub(crate) mod ui_egui;
    ```
*   **Global Connection Mutex**: In `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/impl_rusqulite.rs` (lines 19-21):
    ```rust
    lazy_static::lazy_static! {
    pub static ref CONN: Arc<Mutex<Result<Connection>>> = Arc::new(Mutex::new(_new_connection()));
    }
    ```
*   **Table Naming Rule**: In `/home/vasile/.gemini/antigravity/scratch/crack/packages/storage_crackhouse/src/models.rs` (line 73 / 98 / 114):
    ```rust
    let table_name = format!("{}_{}", self.model_grp(), self.table_name());
    ```

---

## 2. Logic Chain
1.  **Requirement**: The integration test must compile against types in the `mission_plugin` submodule of the Bevy client library crate.
2.  **Constraint**: Integration tests are compiled as external crates and can only access the public API surface of the target crate.
3.  **Inference**: Since the `plugins` module in `demo_resolution_selector_web_bevy/src/lib.rs` is declared as `pub(crate)`, it is private to the crate. Therefore, attempts to write `use demo_resolution_selector_web_bevy::plugins::mission_plugin::*` from an integration test will fail to compile.
4.  **Action**: We must expose `mission_plugin` by making `plugins` public or re-exporting `mission_plugin` from the root of `demo_resolution_selector_web_bevy`.
5.  **Mocking Need**: E2E integration tests need database state isolation to prevent cross-test contamination and host filesystem pollution.
6.  **DB Access Mechanism**: The database uses a global connection reference `CONN` stored as a lazy static `Mutex`.
7.  **Inference**: Swapping the `Connection` within `CONN` using `Connection::open_in_memory()` during test startup enables full database isolation without changing code logic elsewhere.
8.  **Macro Verification**: Using `storage_crackhouse`'s `declare_model_group!` macro with group `storage_crackhouse` and struct `MissionProgress` naturally yields `storage_crackhouse_MissionProgress`, aligning with the database schema specification.

---

## 3. Caveats
*   **Sequential Test Requirement**: Because `CONN` is a static global variable, tests executing in parallel could contaminate each other if they modify the in-memory database instance simultaneously. Tests must be executed with `--test-threads=1` or run sequentially.
*   **Headless Device Creation**: Headless tests running Bevy with full graphics/window modules on headless systems (like CI environments) can cause crashes. Tests should be written using `MinimalPlugins` or disable rendering and graphics components.

---

## 4. Conclusion
E2E test setup for the Bevy mission trigger and state machine system is fully feasible. By placing integration tests in `crack_demo/demo_resolution_selector_web_bevy/tests/mission_integration_tests.rs`, re-exporting the `mission_plugin` submodule, creating stub plugin structures, and swapping `CONN` for an in-memory DB at test bootstrap, we establish a robust, compiling E2E harness.

---

## 5. Verification Method
1.  Run the Bevy client package build to ensure no regression after changing visibility settings:
    `cargo build -p demo_resolution_selector_web_bevy`
2.  Run the new integration tests specifically:
    `cargo test -p demo_resolution_selector_web_bevy --test mission_integration_tests`
3.  Invalidation condition: If the test harness fails to compile due to visibility errors or if tests pollute the local `post3.db` file, this indicates that the re-export or `CONN` swapping logic is misconfigured.
