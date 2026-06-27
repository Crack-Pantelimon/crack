# Explorer Report: Bevy Mission System E2E Test Setup (Milestone 1)

This report details the architectural layout, crate dependencies, integration test conventions, stub design, and database mocking strategy required to set up the End-to-End (E2E) testing framework for the Bevy Mission Trigger & State Machine System.

---

## 1. Summary of Findings
We analyzed the cargo workspace configuration, the `demo_resolution_selector_web_bevy` crate visibility constraints, and the `storage_crackhouse` database API. We established that the Bevy client crate's internal `plugins` module is currently private (`pub(crate)`), which requires a re-export to be testable, and that the global SQLite connection `CONN` in `storage_crackhouse` can be safely intercepted and sandboxed using in-memory databases in integration tests.

---

## 2. Codebase & Crate Structure Analysis
The workspace consists of several packages managed by a root `Cargo.toml`. Key details include:
*   **Root Crate (`crack`)**: Serves as a thin wrapper that re-exports `api_asscrack`, `consensus_crackhead`, `net_crackpipe`, and `storage_crackhouse` libraries, and links worker implementations depending on the target features (`web_serviceworker_worker`, etc.).
*   **Bevy Client (`demo_resolution_selector_web_bevy`)**: Declared in `crack_demo/demo_resolution_selector_web_bevy`. It contains the main Bevy application configuration. Its entry points are:
    *   `src/lib.rs`: Exposes a library target, allowing external tests to link against its modules.
    *   `src/main_bevy.rs`: Contains the `main_bevy()` function that instantiates the Bevy `App` and registers game plugins.
*   **Module Visibility Constraint**: In `crack_demo/demo_resolution_selector_web_bevy/src/lib.rs` (lines 1-5):
    ```rust
    pub mod config;
    pub mod main_bevy;
    pub(crate) mod plugins;
    pub(crate) mod ui_egui;
    ```
    Since `plugins` is marked as `pub(crate)`, any submodules nested under it (like the future `mission_plugin`) will be completely private and inaccessible to Cargo integration tests. 
    *   **Recommendation**: In order for integration tests to verify the plugin's internal state structures and systems, the `mission_plugin` must be exposed. We recommend adding a public re-export inside `src/lib.rs`:
        ```rust
        pub use plugins::mission_plugin;
        ```
        And inside `src/plugins/mod.rs`, declare the module as public:
        ```rust
        pub mod mission_plugin;
        ```

---

## 3. Mission Plugin Stubs Structure
To ensure that integration tests compile successfully before the full logic is implemented in subsequent milestones, the `mission_plugin` should be stubbed out. 

We recommend placing the stubs under a new directory:
`crack_demo/demo_resolution_selector_web_bevy/src/plugins/mission_plugin/`

### File Layout:
1.  `mod.rs`: Registers the submodules and defines the Bevy `Plugin` interface.
2.  `config.rs`: Implements deserialization structures for `missions_config.json`.
3.  `state.rs`: Defines the state machine data structures.
4.  `trigger.rs`: Implements stubs for location trigger checks.
5.  `ui.rs`: Implements stubs for HUD egui layouts.
6.  `db.rs`: Implements database synchronization stubs.

### Recommended Stub Code:

#### `mod.rs`
```rust
pub mod config;
pub mod state;
pub mod trigger;
pub mod ui;
pub mod db;

use bevy::prelude::*;

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<state::MissionState>()
           .init_resource::<config::MissionList>()
           .add_systems(Update, (
               trigger::check_mission_triggers,
           ));
    }
}
```

#### `config.rs`
```rust
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: u32,
    pub title: String,
    pub client: String,
    pub prerequisites: Vec<u32>,
    pub start_coords: [f32; 3],
    pub end_coords: [f32; 3],
    pub radius: f32,
    pub objectives: Vec<String>,
    pub dialogues: Vec<String>,
    pub reward_cash: u32,
    pub reward_respect: u32,
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissionList {
    pub missions: Vec<Mission>,
}
```

#### `state.rs`
```rust
use bevy::prelude::*;
use std::collections::HashSet;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MissionStatus {
    #[default]
    Available,
    Active,
    Completed,
    Failed,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct MissionState {
    pub current_mission: Option<u32>,
    pub completed_missions: HashSet<u32>,
    pub current_objective_idx: usize,
    pub active_state: MissionStatus,
}
```

#### `trigger.rs`
```rust
use bevy::prelude::*;
use crate::plugins::mission_plugin::state::MissionState;

// Marker component for querying the player entity's position
#[derive(Component, Debug)]
pub struct CarMarker; 

pub fn check_mission_triggers(
    _car_query: Query<&Transform, With<CarMarker>>,
    _mission_state: ResMut<MissionState>,
) {
    // Stub implementation for Milestone 1 - returns immediately
}
```

#### `ui.rs`
```rust
use bevy::prelude::*;

pub fn render_mission_hud() {
    // Stub implementation - returns immediately
}
```

#### `db.rs`
```rust
use bevy::prelude::*;

pub fn load_mission_progress() {
    // Stub implementation - returns immediately
}

pub fn save_mission_progress() {
    // Stub implementation - returns immediately
}
```

---

## 4. Integration Test Setup & Cargo Registration
Cargo recognizes integration tests when they are placed in a `tests` directory at the root of a package.

### Target Location
We recommend putting the test file at:
`crack_demo/demo_resolution_selector_web_bevy/tests/mission_integration_tests.rs`

This matches Cargo's package directory structure and ensures that when `cargo test -p demo_resolution_selector_web_bevy` is run, Cargo builds the integration test linking it against the Bevy client library target.

### Crate Cargo.toml Integration
To ensure the test has the necessary crates, check that `crack_demo/demo_resolution_selector_web_bevy/Cargo.toml` has `bevy` (or at least `bevy/prelude`), `serde`, and `storage_crackhouse` accessible during test runs.

To make the target explicit, you can optionally append this to the crate's `Cargo.toml`:
```toml
[[test]]
name = "mission_integration_tests"
path = "tests/mission_integration_tests.rs"
```

### Headless Integration Test Stub Example
The E2E integration tests can run Bevy headlessly (without loading heavy window systems or graphics plugins) using `MinimalPlugins`. Below is the suggested stub test layout:

```rust
use bevy::prelude::*;
use demo_resolution_selector_web_bevy::plugins::mission_plugin::{
    MissionPlugin, MissionState, MissionStatus, config::MissionList, trigger::CarMarker
};

#[test]
fn test_mission_initialization() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(MissionPlugin);

    // Verify resources exist
    assert!(app.world().contains_resource::<MissionState>());
    assert!(app.world().contains_resource::<MissionList>());

    // Verify initial state
    let state = app.world().resource::<MissionState>();
    assert_eq!(state.current_mission, None);
    assert_eq!(state.active_state, MissionStatus::Available);
}
```

---

## 5. Storage Crackhouse Mocking & Interface Strategy

### Database API Analysis
In `packages/storage_crackhouse/src/impl_rusqulite.rs`, database connections are managed via a lazy static mutex:
```rust
lazy_static::lazy_static! {
pub static ref CONN: Arc<Mutex<Result<Connection>>> = Arc::new(Mutex::new(_new_connection()));
}
```
And queries are run asynchronously via the `sql_query` function:
```rust
pub async fn sql_query(sql: SQLAndParams) -> anyhow::Result<SqlResultSet> {
    let conn = CONN.lock().await;
    let conn = conn.as_ref().map_err(|e| anyhow::anyhow!("Error fetching SQL lock: {e:?}"))?;
    // ... prepares statement and executes query ...
}
```

### Mocking Strategy (In-Memory Database Sandbox)
Because `CONN` exposes the raw `Connection` inside an `Arc<Mutex<Result<Connection>>>`, we can lock the mutex inside our integration tests and swap the connection with a temporary, in-memory SQLite database. This guarantees that test state is fully isolated, does not pollute the host file system (`post3.db`), and starts from a clean slate on every run.

Example mocking setup in `tests/mission_integration_tests.rs`:
```rust
use rusqlite::Connection;
use storage_crackhouse::impl_rusqulite::CONN;

pub async fn setup_sandbox_db() {
    let mut conn_guard = CONN.lock().await;
    
    // Open a brand-new, isolated in-memory sqlite connection
    let mock_conn = Connection::open_in_memory().expect("Failed to open mock in-memory DB");

    // Initialize the schema for the mission progress tracking
    mock_conn.execute(
        "CREATE TABLE IF NOT EXISTS storage_crackhouse_MissionProgress (
            mission_id INTEGER PRIMARY KEY,
            status TEXT NOT NULL,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    ).expect("Failed to create mock schema");

    // Swap the global connection reference
    *conn_guard = Ok(mock_conn);
}
```

### Database Schema Matching
The `storage_crackhouse` crate uses `declare_model_group!` (inside `packages/storage_crackhouse/src/models.rs`) to dynamically define database tables using the format `{group_name}_{struct_name}`.

By declaring the database model as follows:
```rust
declare_model_group! {
    storage_crackhouse, // Group Name matches schema prefix

    #[db_table(pk(mission_id))]
    pub struct MissionProgress {
        pub mission_id: i64,
        pub status: String,
        pub updated_at: String,
    }
}
```
The resulting table name generated in the SQLite database will match exactly with the layout requested in `PROJECT.md`:
*   **Table Name**: `storage_crackhouse_MissionProgress`
*   **Primary Key**: `mission_id` (INTEGER / i64)
*   **Columns**: `status` (TEXT / String), `updated_at` (TIMESTAMP / String)

---

## 6. Handoff Protocol Details

### Observation
1.  **Private Modules**: In `crack_demo/demo_resolution_selector_web_bevy/src/lib.rs` (lines 3-4), the plugins module is declared as `pub(crate) mod plugins;` and `pub(crate) mod ui_egui;`.
2.  **Storage Mutex**: In `packages/storage_crackhouse/src/impl_rusqulite.rs` (lines 19-21), the connection reference is initialized as a public `static ref CONN: Arc<Mutex<Result<Connection>>> = Arc::new(Mutex::new(_new_connection()));`.
3.  **Table Naming Rule**: The code macro `declare_model_group!` in `packages/storage_crackhouse/src/models.rs` (line 73) defines the database table name using:
    `let table_name = format!("{}_{}", self.model_grp(), self.table_name());`.

### Logic Chain
1.  **Module Accessibility**: Because `plugins` is `pub(crate)`, a separate integration test crate targeting `demo_resolution_selector_web_bevy` would fail to import `demo_resolution_selector_web_bevy::plugins::*`. Re-exporting or modifying this visibility to `pub` is mathematically required to allow integration tests to link against the plugin stubs.
2.  **Global Connection Hijacking**: Because `CONN` is protected by a thread-safe mutex and publicly accessible from `storage_crackhouse::impl_rusqulite`, dereferencing the guard inside tests and swapping the underlying connection with `Connection::open_in_memory()` forces the crate's internal SQL systems to run against an isolated sandbox, solving E2E test isolation safely.

### Caveats
1.  **Multi-threaded Tests**: Because `CONN` is a static global variable, tests executing in parallel could experience state corruption if they simultaneously modify the same database instance.
    *   *Mitigation*: We recommend running tests sequentially via `cargo test -- --test-threads=1`, or acquiring a mutex lock inside each test function before performing DB assertions.
2.  **Avian3D / Egui dependencies in headless testing**: In a fully headless test run, the egui and physics plugins must be stubbed or carefully disabled/ignored to prevent compiler warnings or panics related to graphic device creation (e.g. missing display servers).

### Conclusion
Milestone 1 test setup is fully feasible. By implementing the public visibility re-exports in `lib.rs`, setting up the `tests/` subdirectory in the Bevy client package, stubbing the `mission_plugin` using minimal Bevy resource registrations, and swapping `CONN` for an in-memory db during test bootstrap, we can achieve a clean E2E test harness that compiles and runs correctly.

### Verification Method
To verify this configuration post-setup:
1.  Add the re-export to `lib.rs` and verify the crate still builds: `cargo build -p demo_resolution_selector_web_bevy`.
2.  Create the `tests/mission_integration_tests.rs` with the headless app initialization code.
3.  Run the tests package: `cargo test -p demo_resolution_selector_web_bevy --test mission_integration_tests`.
