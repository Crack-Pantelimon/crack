# Project: Bevy Mission Trigger & State Machine System

## Architecture
We are implementing a modular mission system inside the Bevy client:
- **Mission Config (`assets/missions_config.json`)**: A static JSON file containing the 42 missions mapped to their name, objectives, start/end coordinates (using Bevy's coordinate system matching `MapTree` bounding box), trigger radii, dialogs, etc.
- **Mission Plugin (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/mission_plugin/`)**:
  - `config.rs`: Loads/deserializes `missions_config.json`.
  - `state.rs`: Manages active/locked/completed mission state machine DAG.
  - `trigger.rs`: Monitory player (car) position relative to active start/end coordinate triggers, checking radius, and advancing state.
  - `ui.rs`: Implements the egui HUD showing current active mission details, current objective, client, and dialogues.
  - `db.rs`: Integrates with `storage_crackhouse` to save completed mission states and load progress on start.
  - `mod.rs`: The Bevy `Plugin` setup.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | E2E Test Suite Setup | E2E Test infrastructure, runner, Tiers 1-4 test cases | None | IN_PROGRESS |
| 2 | Coordinate Mapping | Mission config asset, JSON deserialization, Bevy loading | M1 | PLANNED |
| 3 | State Machine & Triggers | Core systems for trigger detection, state changes, DAG traversal | M2 | PLANNED |
| 4 | egui HUD & 3D Markers | Render visual rings/beacons, show egui panel overlay | M3 | PLANNED |
| 5 | SQLite Persistence | Save/load mission progress from DB using storage APIs | M4 | PLANNED |
| 6 | Integration & Verification | E2E Test Verification (Phase 1) | M1, M5 | PLANNED |
| 7 | Adversarial Hardening | Challenger and Auditor runs, coverage improvements (Phase 2) | M6 | PLANNED |

## Interface Contracts
- **`MissionConfig`**:
  ```rust
  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
  ```
- **`MissionState` Resource**:
  - `current_mission: Option<u32>`
  - `completed_missions: HashSet<u32>`
  - `current_objective_idx: usize`
  - `active_state: MissionStatus` (enum: Available, Active, Completed, Failed)
- **SQLite Database Schema**:
  - Table `storage_crackhouse_MissionProgress` (columns: `mission_id INTEGER PRIMARY KEY`, `status TEXT`, `updated_at TIMESTAMP`)

## Code Layout
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/mission_plugin/`: Main plugin location.
- `assets/missions_config.json`: Structured missions config.
- `tests/mission_integration_tests.rs`: Automated integration test suite.
