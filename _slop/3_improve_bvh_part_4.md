# Improve BVH occlusion — Part 4: per-mode sample-radius sliders + velocity-predictive sampling

Part 4 of 4. Requires parts 1–3 merged (single `MainCamera` reference,
trimesh occluders, lock-step walk).

## Build & check (read first)

All commands run from the repo root `/home/p/VIDOEGAME/crack`. `cargo` may be
provided by an AppImage that misbehaves through its "proxy" wrapper — always
`unset ARGV0` first. Only check the two affected crates:

```sh
unset ARGV0
cargo check -p game_logic --features worker
cargo check -p demo_resolution_selector_web_bevy
```

## Problem

When the player drives fast around a corner, a tile that was correctly
occluded a moment ago becomes visible faster than it can stream in. The
camera *point* is the right reference for what is visible **now**, but
culling decisions need to be robust to where the camera will be over the next
half-second or so.

Design decided with the maintainer (**velocity-predictive + ring**, replacing
the old Fibonacci-sphere idea):

- Each control mode gets a **sample radius** slider in the 3D BVH Minimap
  debug window: *Freecam / Car / Pedestrian*, default `0.1`, range
  `0.1..=100.0`. The radius bounds how far from the camera extra visibility
  sample origins may be placed.
- The client sends the camera's **velocity** along with its position.
- Worker-side, per camera, visibility origins are:
  1. the camera point itself (always);
  2. if `sample_radius >= MIN_SAMPLING_RADIUS`: 3 points along the velocity
     direction at `radius * {1/3, 2/3, 1}` (lookahead, capped by the slider) —
     only when speed is meaningful;
  3. an 8-point horizontal ring of radius `sample_radius` around the camera
     (same y), covering turns the velocity doesn't predict.
- **Origin pre-filter** (replaces "ignore backfaces" brainstorming): an extra
  origin is only used if the real camera can *see* it — i.e.
  `!is_ray_occluded(camera.center, origin, ...)`. An origin behind a wall or
  under the ground is thereby discarded, so it can never falsely re-grant
  visibility (this was the exact failure mode of the old 6 m sphere model).

At the default `0.1` the radius is below `MIN_SAMPLING_RADIUS` (0.25), so
only the camera point is tested — identical to today's behavior until the
user raises a slider.

## Changes

### 1. `crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/mod.rs` — state

Add to `pub struct MapLODState` (around line 101):

```rust
pub sample_radius_freecam: f32,
pub sample_radius_car: f32,
pub sample_radius_pedestrian: f32,
```

`MapLODState` is `#[derive(Default)]`, so these start at `0.0`. Real defaults
are set at manifest load, next step.

### 2. `crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/manifest_flow.rs` — defaults

In the manifest-loaded block (where `lod_state.enable_visibility_cull = true;`
is set, ~line 62), add — guarded so a manifest reload doesn't stomp a slider
the user already moved:

```rust
if lod_state.sample_radius_freecam <= 0.0 {
    lod_state.sample_radius_freecam = 0.1;
}
if lod_state.sample_radius_car <= 0.0 {
    lod_state.sample_radius_car = 0.1;
}
if lod_state.sample_radius_pedestrian <= 0.0 {
    lod_state.sample_radius_pedestrian = 0.1;
}
```

### 3. `crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/bvh_minimap.rs` — sliders

In `bvh_minimap_window`, directly under the existing
`ui.checkbox(&mut lod_state.enable_visibility_cull, ...)`:

```rust
ui.add(
    egui::Slider::new(&mut lod_state.sample_radius_freecam, 0.1..=100.0)
        .logarithmic(true)
        .text("freecam sample radius"),
);
ui.add(
    egui::Slider::new(&mut lod_state.sample_radius_car, 0.1..=100.0)
        .logarithmic(true)
        .text("car sample radius"),
);
ui.add(
    egui::Slider::new(&mut lod_state.sample_radius_pedestrian, 0.1..=100.0)
        .logarithmic(true)
        .text("pedestrian sample radius"),
);
```

### 4. `crack_demo/game_logic/src/lod.rs` — protocol

Extend `CameraReference`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CameraReference {
    pub center: Vec3,
    pub max_range: f32,
    pub velocity: Vec3,      // NEW: world-space m/s, smoothed client-side
    pub sample_radius: f32,  // NEW: per-mode slider value, meters
}
```

Grep the repo for every `CameraReference {` constructor and add the new
fields (tests included; use `velocity: Vec3::ZERO, sample_radius: 0.1` where
no better value exists).

### 5. `crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs` — kinematics + request

#### 5a. Camera kinematics resource + system (new, in this file)

```rust
/// Smoothed world-space kinematics of the MainCamera, sampled every frame.
/// Feeds the velocity-predictive occlusion sampling in the worker.
#[derive(Resource, Default)]
pub struct CameraKinematics {
    pub position: Vec3,
    pub velocity: Vec3,
    pub initialized: bool,
}

pub fn track_camera_kinematics(
    time: Res<Time>,
    q_camera: Query<
        &GlobalTransform,
        With<crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera>,
    >,
    mut kin: ResMut<CameraKinematics>,
) {
    let Some(cam) = q_camera.iter().next() else { return; };
    let pos = cam.translation();
    let dt = time.delta_secs();
    if !kin.initialized || dt <= 1e-6 {
        kin.position = pos;
        kin.velocity = Vec3::ZERO;
        kin.initialized = true;
        return;
    }
    let raw = (pos - kin.position) / dt;
    // Teleport guard: mode switches / respawns jump the camera; a bogus huge
    // velocity would poke sample origins through the whole map.
    if raw.length() > 500.0 {
        kin.velocity = Vec3::ZERO;
    } else {
        // EMA smoothing so a single jittery frame doesn't swing the lookahead.
        kin.velocity = kin.velocity.lerp(raw, 0.2);
    }
    kin.position = pos;
}
```

Register in `crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/mod.rs`:
`.init_resource::<lod_flow::CameraKinematics>()` on the app, and add
`lod_flow::track_camera_kinematics` to the same `Update` system tuple that
already contains `lod_flow::spawn_lod_task` (order before `spawn_lod_task`,
e.g. `track_camera_kinematics.before(spawn_lod_task)` or just first in the
tuple with `.chain()` if that tuple is chained — match the existing style at
`crack_plugin/mod.rs:43-44`).

#### 5b. `spawn_lod_task` — pick the radius, send velocity

Add system params:

```rust
control_state: Res<State<crate::plugins::states::GameControlState>>,
kin: Res<CameraKinematics>,
```

Pick the radius from the active mode (`GameControlState` variants are
`MapFreecam`, `DrivingCar`, `ControllingPedestrian` —
`crack_demo/demo_resolution_selector_web_bevy/src/plugins/states/mod.rs:31`):

```rust
let sample_radius = match control_state.get() {
    crate::plugins::states::GameControlState::MapFreecam => lod_state.sample_radius_freecam,
    crate::plugins::states::GameControlState::DrivingCar => lod_state.sample_radius_car,
    crate::plugins::states::GameControlState::ControllingPedestrian => {
        lod_state.sample_radius_pedestrian
    }
};
```

(`lod_state` here is the existing `Res<MapLODState>` param named
`lod_state`.) Then extend the single camera push from part 1:

```rust
cameras.push(game_logic::lod::CameraReference {
    center: camera.translation(),
    max_range: 32.0,
    velocity: kin.velocity,
    sample_radius,
});
```

#### 5c. Change-detection key

Dragging a slider must force a recompute. The `last: Local<Option<(...)>>`
tuple currently holds `(nodes, quantized_refs, budget, cull, max_lod,
tiles_per_diagonal_bits)`. Append one more element,
`sample_radius_bits: (u32, u32, u32)`:

```rust
let radius_bits = (
    lod_state.sample_radius_freecam.to_bits(),
    lod_state.sample_radius_car.to_bits(),
    lod_state.sample_radius_pedestrian.to_bits(),
);
```

Update the `Local` type, the comparison, and the `*last = Some((...))`
assignment. Do **not** put the velocity in the key (it changes every frame;
recomputes are already driven by quantized camera movement).

### 6. `crack_demo/game_logic/src/visibility.rs` — the sampling rewrite

Rewrite the body of `is_node_visible`. Keep: the `cameras.is_empty()` /
`trimeshes.is_empty()` early-outs and the `corners` array (8 corners +
center). Delete: `CAMERA_SAMPLE_RADIUS` (radius now arrives per camera), the
Fibonacci-sphere block, and the long historical comment about the old sphere
model. Keep `MIN_SAMPLING_RADIUS` at `0.25`.

New logic:

```rust
const MIN_SAMPLING_RADIUS: f32 = 0.25;
/// Velocity below this (m/s) is noise; skip lookahead points.
const MIN_LOOKAHEAD_SPEED: f32 = 0.5;

for camera in cameras {
    // Closure: can any target corner be seen from this origin?
    let sees_node = |origin: Vector| -> bool {
        corners
            .iter()
            .any(|&q| !self.is_ray_occluded(origin, q, node_path, node_bbox))
    };

    // 1. The camera point itself is always definitive when clear.
    if sees_node(camera.center) {
        return true;
    }

    let r = camera.sample_radius;
    if r < MIN_SAMPLING_RADIUS {
        continue; // point-based model: nothing more to test for this camera
    }

    // Candidate extra origins, all within radius r of the camera.
    let mut origins: Vec<Vector> = Vec::with_capacity(11);

    // 2. Velocity lookahead: where the camera is about to be.
    let speed = camera.velocity.length();
    if speed > MIN_LOOKAHEAD_SPEED {
        let v_dir = camera.velocity / speed;
        for k in [1.0 / 3.0, 2.0 / 3.0, 1.0] {
            origins.push(camera.center + v_dir * (r * k));
        }
    }

    // 3. Horizontal ring: turns the velocity doesn't predict.
    for k in 0..8 {
        let a = (k as f32) * std::f32::consts::FRAC_PI_4;
        origins.push(camera.center + Vector::new(a.cos() * r, 0.0, a.sin() * r));
    }

    for origin in origins {
        // Pre-filter: only use origins the real camera can see. An origin
        // behind a wall or under the terrain fails this test and is skipped,
        // so it can never falsely re-grant visibility (the failure mode of
        // the old large-radius sphere model).
        if self.is_ray_occluded(camera.center, origin, node_path, node_bbox) {
            continue;
        }
        if sees_node(origin) {
            return true;
        }
    }
}

false
```

Type note: `camera.center`/`camera.velocity` are `glam::Vec3` and the parry
math `Vector` in this codebase is glam-backed — the existing code already
passes `camera.center` straight into `is_ray_occluded`, so arithmetic mixing
them compiles as-is; if a mismatch does appear, convert with
`Vector::new(v.x, v.y, v.z)`.

Cost bound: worst case per camera = 1 + (3 + 8) origins, each up to 9 corner
rays + 1 pre-filter ray ≈ 108 rays per candidate node — comparable to the old
16-sample Fibonacci model, and only paid when a slider is raised above 0.25.

## What NOT to change

- `is_ray_occluded`, `insert_occluder`/`remove_node`, `TRIMESH_CACHE`,
  the lock-step walk in `lod.rs` (only `CameraReference` gains fields).
- `reference_points` handling.

## Verification

1. Both `cargo check`s + `cargo test -p game_logic --features worker`.
2. Manual, with the 3D BVH Minimap open:
   - All three sliders visible under the occluder checkbox; dragging any of
     them while stationary triggers a visible recompute (split/merge churn or
     culled-count change) — proves the change-detection key works.
   - Sliders at default 0.1: culling behavior identical to part 3.
   - Set the **car** radius to ~20–30, drive fast around a building corner:
     the tiles around the corner should already be resolved when you arrive
     (no raw/coarse pop-in). Compare with radius 0.1 to confirm the effect.
   - Stand in a pit with pedestrian radius 0.1 → distant tiles cull (the
     regression guarded by `_slop/1_fix_bvh_plan.md`); raise the radius to
     100 → culling relaxes but must NOT vanish entirely if the pit truly
     encloses the camera (the origin pre-filter discards out-of-pit origins).
