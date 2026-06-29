# Project Plan: Bevy Mission Trigger & State Machine System

We are implementing a mission trigger and state machine system for the Bevy client. It parses 42 missions, maps them to Pantelimon coordinates, triggers state transitions, displays them in an egui overlay and 3D viewport, and persists progress to SQLite.

## Parallel Track Architecture
Following the Dual Track pattern, we will launch two tracks:
1. **E2E Testing Track**: Spawns an E2E Testing Orchestrator to design and implement a comprehensive test suite (Tiers 1-4) independently, publishing `TEST_READY.md` and `TEST_INFRA.md`.
2. **Implementation Track**: Progresses through milestones (coordinate database, state machine & triggers, egui HUD & 3D indicators, SQLite persistence), eventually integrating with the E2E tests, followed by Phase 2 (Adversarial Coverage Hardening).

## Planned Milestones
- **Milestone 1**: E2E Test Suite Creation (E2E Testing Track)
- **Milestone 2**: Coordinate Configuration & Parsing (Implementation Track)
- **Milestone 3**: Trigger Detection & State Machine (Implementation Track)
- **Milestone 4**: egui HUD & 3D Markers (Implementation Track)
- **Milestone 5**: SQLite Persistence (Implementation Track)
- **Milestone 6**: Test Integration & Verification (Phase 1)
- **Milestone 7**: Adversarial Hardening & Auditing (Phase 2)

## Team Organization
- **E2E Testing Orchestrator** (Conv ID: [TBD]): Responsible for E2E Test suite.
- **Explorer** (Conv ID: [TBD]): Performs read-only code exploration.
- **Worker** (Conv ID: [TBD]): Implements features and verifies builds.
- **Reviewers** (Conv ID: [TBD]): Independently reviews code.
- **Challengers** (Conv ID: [TBD]): Runs adversarial tests.
- **Auditor** (Conv ID: [TBD]): Performs integrity audits.
