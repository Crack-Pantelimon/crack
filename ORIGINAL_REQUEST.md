# Original User Request

## Initial Request — 2026-06-27T12:28:02+02:00

Design and implement a mission trigger and state machine system inside the Bevy web/native client, parsing the 42 markdown missions and mapping their active coordinates onto trigger zones on the 3D map of Pantelimon.

Working directory: /home/vasile/.gemini/antigravity/scratch/crack
Integrity mode: development

## Requirements

### R1. Mission-to-Map Coordinate Mapping
Define a structured configuration/database mapping each of the 42 missions to specific 3D start/end coordinates and trigger radii on the 3D map of Pantelimon.

### R2. Trigger Detection & Game State Machine
Detect when the player's position (e.g. car or character) enters a trigger zone for an active mission. Transition the state machine from available/locked -> active -> completed as objectives are met, unlocking dependent missions in the DAG.

### R3. Visual Indicators and HUD Overlay
Render 3D visual markers (e.g., colored rings or beacons) at coordinates of currently active mission triggers. Display the current mission title, client, objectives, and dialogues in an egui overlay.

### R4. Progress Persistence
Integrate the mission state machine with the repository's SQLite/P2P storage system so progress is preserved across game restarts.

## Verification Plan

### Automated Verification
- Implement a test suite or integration test (e.g. under `tests/` or via a test harness) that simulates player coordinates moving into trigger zones, verifying that:
  - Entering the start coordinates initiates the mission state.
  - Completing objectives triggers the correct state transitions.
  - Finalizing the mission updates the SQLite database.
  - Re-launching the client restores the saved progress.

## Acceptance Criteria

### Mission Configuration & State
- [ ] A structured file (e.g., JSON/YAML) exists mapping coordinates and radii to all 42 missions.
- [ ] The game state machine correctly models the dependency graph (DAG) of the 42 missions.

### Bevy Client & Triggers
- [ ] Visual markers/rings are rendered in the Bevy 3D viewport at active trigger coordinate locations.
- [ ] The system detects when the player enters the trigger boundaries, changing the game state to active.
- [ ] An egui overlay correctly displays the active mission metadata (title, client, objectives list, dialog).
- [ ] Transitioning to the end zone updates the state to completed and unlocks the next missions.

### Database Persistence
- [ ] Completed mission states are persistently saved in the local SQLite/P2P database.
- [ ] Saved progress is retrieved on startup to lock/unlock the correct set of active missions.

### Tests
- [ ] An automated integration test verifies the entire trigger, state machine, and persistence cycle.
