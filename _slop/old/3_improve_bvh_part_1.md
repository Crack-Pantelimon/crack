# Improve BVH occlusion — Part 1: one camera reference for all three control modes

Part 1 of 4. Parts are ordered and must land in order:
1. **(this file)** Unify the occlusion reference point: all 3 control modes use `MainCamera`.
2. Replace heightfield occluders with the real tile trimeshes.
3. Lock-step LOD walk: visibility tested against the evolving proposed render set.
4. Per-mode sample-radius sliders + velocity-predictive sphere sampling.

## Build & check (read first)

All commands run from the repo root `/home/p/VIDOEGAME/crack`. `cargo` may be
provided by an AppImage that misbehaves through its "proxy" wrapper — always
`unset ARGV0` first. Only check the two affected crates; never build/run the
whole workspace:

```sh
unset ARGV0
cargo check -p game_logic --features worker
cargo check -p demo_resolution_selector_web_bevy
```

Run both checks after every step below. Fix warnings about unused
imports/variables that your own edits introduce.

## Problem

The LOD/occlusion request is built in
`crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs`,
fn `spawn_lod_task`. Lines ~104–147 pick the occlusion "camera" differently per
control mode:

- car mode → the **car body** transform (`ActivePlayerVehicle`),
- pedestrian mode → the **character body** transform (`CharacterController`),
- freecam → the camera, but only if the two branches above didn't match.

Bugs this causes:

1. After escaping to freecam, `ControlledCharacter`/`CameraRig` still exist, so
   the second branch wins and occlusion is computed from the **abandoned
   character's position**, not the freecam camera. If `controlled.controller`
   is `None`, **no camera at all** is pushed and `OccluderWorld::is_node_visible`
   (`crack_demo/game_logic/src/visibility.rs`) returns `true` unconditionally
   (`if cameras.is_empty() { return true; }`) — freecam appears to "not run
   occlusion at all".
2. Car/pedestrian modes test visibility from the body, not from the actual
   chase camera which sits meters behind/above — subtly wrong culling.

There is exactly one render camera in all 3 modes: the entity with the
`MainCamera` marker
(`crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs:18`).
All modes must use it, via the identical code path.

## Changes — all in `lod_flow.rs`

### 1. Camera query must read `GlobalTransform`

The current query is:

```rust
q_camera: Query<
    &Transform,
    With<crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera>,
>,
```

`Transform` is the *local* transform; if the camera is ever parented to a rig
this is wrong. Change it to:

```rust
q_camera: Query<
    &GlobalTransform,
    With<crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera>,
>,
```

and everywhere below use `camera.translation()` (method on `GlobalTransform`)
instead of `camera.translation` (field on `Transform`).

### 2. Delete the per-mode camera selection

Remove these system parameters from `spawn_lod_task` entirely (they become
unused):

- `camera_rig: Option<Res<...CameraRig>>`
- `q_vehicle: Query<&Transform, With<...ActivePlayerVehicle>>`
- `controlled_char: Option<Res<...ControlledCharacter>>`
- `q_character: Query<&Transform, With<...CharacterController>>`

Delete the whole block that computes `camera_range` / `is_vehicle` (lines
~104–116) and the whole three-branch `cameras` construction (lines ~118–140).

### 3. Build exactly one `CameraReference`

Replace the deleted blocks with:

```rust
let mut cameras = Vec::new();
if let Some(camera) = q_camera.iter().next() {
    cameras.push(game_logic::lod::CameraReference {
        center: camera.translation(),
        // max_range is currently unused by the worker-side visibility code;
        // kept for wire compatibility until part 4 replaces it with a real
        // per-mode sample radius.
        max_range: 32.0,
    });
}
```

### 4. Stop pushing `reference_points` as occlusion cameras

Delete this loop (lines ~142–147):

```rust
for &ref_pos in &lod_state.reference_points {
    cameras.push(game_logic::lod::CameraReference { center: ref_pos, max_range: 5.0 });
}
```

`lod_state.reference_points` must **remain** part of `refs`
(`reference_points` in the request) — they still drive LOD distance scoring.
They just no longer act as occlusion viewpoints. Do not touch the `refs`
handling at the top of the function (the `refs.push(camera.translation())`
line stays, adjusted for `GlobalTransform` per step 1).

### 5. Clean up

Remove any now-unused imports at the top of `lod_flow.rs`. The
`quantize`/change-detection (`last: Local<...>`) logic is untouched — the
camera position already flows into the key via `refs`.

## What NOT to change

- `crack_demo/game_logic/src/lod.rs` and `visibility.rs` — untouched in this part.
- `bvh_minimap.rs` — it already reads `GlobalTransform` of `MainCamera`.
- The `MainCamera` component itself, camera rigs, or any control-mode logic.

## Verification

1. Both `cargo check` commands pass (see top).
2. Manual (ask the user to run the game): open Debug → 3D BVH Minimap, enable
   "BVH occluder (visibility cull)". In **freecam**, fly down behind a
   building: the "culled N" count (blue) must now become non-zero and change
   as you move — previously it never moved in freecam. Then enter a car and a
   pedestrian and verify culling still happens; with the camera in the same
   physical spot, all three modes should cull the same tiles.
