# E2E Test Infrastructure & Strategy

This document describes the End-to-End (E2E) testing infrastructure for the Bevy Mission System.

## 1. Test Harness Design

The test runner utilizes a headless Bevy `App` to verify the state machine transitions and triggers without launching a full graphics window. This is achieved by registering `MinimalPlugins` along with the custom `MissionPlugin` during test setup:

```rust
fn setup_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(MissionPlugin);
    app
}
```

## 2. Database Sandbox Isolation

To prevent E2E tests from polluting the host filesystem database (`post3.db`) and to guarantee full state isolation between parallel test threads, the global SQLite connection `CONN` in the `storage_crackhouse` library is hijacked and replaced with an in-memory database during test setup:

```rust
async fn setup_db_sandbox() {
    let mut conn_guard = storage_crackhouse::impl_rusqulite::CONN.lock().await;
    let mock_conn = rusqlite::Connection::open_in_memory().expect("Failed to open mock in-memory DB");
    
    // Create the schema
    mock_conn.execute(
        "CREATE TABLE IF NOT EXISTS storage_crackhouse_MissionProgress (
            mission_id INTEGER PRIMARY KEY,
            status TEXT NOT NULL,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    ).expect("Failed to create mock schema");
    
    *conn_guard = Ok(mock_conn);
}
```

## 3. Multi-Tier Testing Coverage

The E2E suite consists of exactly 60 tests structured into four distinct coverage tiers:

*   **Tier 1: Happy Path Features (25 Tests)**
    *   Feature 1: Configuration Loading & DAG Parsing
    *   Feature 2: Trigger Detection & Coordinate Mapping
    *   Feature 3: State Machine Transitions
    *   Feature 4: HUD / egui Overlay Updates
    *   Feature 5: SQLite Database Persistence
*   **Tier 2: Boundary Conditions & Robustness (25 Tests)**
    *   Verifies system stability against corrupt files, floating point errors, zero/negative radii, duplicate indices, and unexpected connection drops.
*   **Tier 3: Pairwise Combinations (5 Tests)**
    *   Tests multi-system interactions (e.g., Trigger -> State transition -> DB Save).
*   **Tier 4: Real-World Gameplay Workflows (5 Tests)**
    *   Simulates realistic workflows such as complete tutorial mission run-throughs, failure and retry paths, fast travels, and game restart/reload retention.
