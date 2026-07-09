# VFX Shaders v2 — Bug Review & Fix Specification

## 0. What this document is

The v1 plan (`_slop/plan_vfx_shader_v1.md`) was implemented by another model in commit
`8a9178b ("vfx")`. The result lives in:

- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/` (new plugin dir)
- `.../src/plugins/weapons/weapon_shooting.rs` (`GunFxEvent` trigger)
- `.../src/plugins/cars_driving/driving_plugin/car_disable.rs` (`CarExplosionEvent` trigger)

Six problems remain, split across two subsystems that happen to be entangled through the
same commit. This document states the root cause of each, with `file:line` and the exact
mechanism, then gives ordered, concrete fixes (new structs, function headers, WGSL). It is
written to be handed to an implementer directly.

The six problems:

1. **IK/aiming** — the aim-IK arm points *behind the character's back* at certain global
   orientations instead of at the target (`arm_ik.rs`).
2. **Pedestrian `F` no longer enters a car as driver** (`interaction_ui.rs` /
   `car_disable.rs`).
3. **Clouds are never visible** (`clouds.rs`, `clouds.wgsl`).
4. **Gun tracer/trail renders wrong** — visible only as a dark sliver ("shadow"), not as a
   bright smoke/tracer trail (`billboard_fx.wgsl`, `gun_fx.rs`).
5. **No muzzle smoke after firing** (`gun_fx.rs`).
6. **No VFX test harness** — need a `vfx_demo` binary that spawns any effect at the
   mouse-click 3D point, mirroring `bin/audio_demo.rs`.

Problems 3–6 are pure VFX; 1–2 are gameplay regressions surfaced alongside the VFX work.
Fixes are independent and can be parallelized after reading §1.

---

## 1. Shared facts (read first)

| Fact | Source | Consequence |
|------|--------|-------------|
| Web build is `Backends::GL` (WebGL2) | [basic_app.rs:34](../crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs#L34) | No compute/storage buffers. All fixes stay in vertex/fragment WGSL + CPU spawn. |
| `BillboardParams` uniform layout is fixed | [materials.rs:16-26](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs#L16) | Any new field must keep 16-byte alignment; extend `_pad` slots, do not reorder. |
| One WGSL über-shader, `switch P.kind` | [billboard_fx.wgsl:84](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/billboard_fx.wgsl#L84) | Fix per-kind branches in place; don't fork the file. |
| Quad mesh is `Rectangle::new(1.0,1.0)`, so local vertex `pos ∈ [-0.5, 0.5]` | [spawn.rs:85](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/spawn.rs#L85) | The shader assumes `pos ∈ [-0.5,0.5]` and does `uv = pos.xy * 2.0` → `uv ∈ [-1,1]`. **This is the tracer bug (see D4).** |
| `AdditiveFxMaterial` = `AlphaMode::Add`, no shadows/prepass | [materials.rs:42-47](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs#L42) | Additive glow effects. |
| `BlendFxMaterial` = `AlphaMode::Blend`, no shadows/prepass | [materials.rs:76-81](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs#L76) | Smoke effects. |
| egui panels must run in `EguiPrimaryContextPass` | [audio/mod.rs:288](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/mod.rs#L288) | UI in `Update` panics. |
| Demo binaries are auto-discovered from `src/bin/*.rs` | [bin/audio_demo.rs](../crack_demo/demo_resolution_selector_web_bevy/src/bin/audio_demo.rs) | New `bin/vfx_demo.rs` needs no `Cargo.toml` edit. |
| `arm_ik::apply_arm_ik` runs `PostUpdate`, after `AnimationSystems`, before `TransformSystems::Propagate` | [mod.rs:424-429](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L424) | `GlobalTransform` is stale here; IK composes world transforms from local `Transform`s by hand ([arm_ik.rs:48-65](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L48)). |

---

## 2. Defects

### D1 — Arm IK flips behind the back (antiparallel `from_rotation_arc` singularity) — PRIMARY (problem 1)

`aim_joint_local_rotation` ([arm_ik.rs:106-121](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L106)):

```rust
let delta_world = Quat::from_rotation_arc(current_dir, desired_dir);
let new_world_rot = delta_world * joint_world.rotation;
```

`Quat::from_rotation_arc(a, b)` is **undefined about which axis it spins** when `a` and `b`
are (near) antiparallel: Bevy/glam picks an *arbitrary* perpendicular axis for the 180° case.
When the character faces roughly *away* from the aim target, the shoulder's current bone
direction (`shoulder → elbow`, i.e. down the rest-pose arm) and the desired direction
(`shoulder → target`) approach antiparallel, so the chosen axis snaps unpredictably and the
whole arm rotates *behind the back*. The exact orientation at which it flips depends on the
character's world yaw — precisely the reported symptom ("depends on global rotation, at a
specific orientation the arm aims behind the back").

The intended guard is `compute_spine_compensation`
([arm_ik.rs:32-44](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L32)),
which pre-rotates the spine when the target is beyond `ARM_DEAD_ZONE_ANGLE = 120°`
([arm_ik.rs:18](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L18))
off forward. It fails for three compounding reasons:

- **Discontinuity at the threshold.** `excess_yaw = angle - ARM_DEAD_ZONE_ANGLE.copysign(angle)`
  is 0 exactly at ±120° and grows past it, so between the shoulder's natural reach (~90°) and
  120° there is a band where no compensation is applied yet the arm already must invert →
  singular `from_rotation_arc`.
- **Stale FK.** The spine compensation is *written* into `rotations`
  ([arm_ik.rs:163](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L163)),
  but `two_bone_ik_positions` and the shoulder/elbow aims below all read the **pre-IK
  snapshot** (`transform_sets.p0()`), so the arm is solved as if the spine had *not* turned.
  The final pose therefore double-counts or fights the compensation.
- **World-Y assumption.** `Quat::from_rotation_y(excess_yaw) * spine_world.rotation` yaws about
  world-Y, which is only correct while the character is upright; any animated torso lean makes
  it drift.

### D2 — Elbow solved against the un-updated shoulder (FK ordering) — problem 1 (secondary)

In `apply_arm_chain_ik` ([arm_ik.rs:191-208](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L191)),
the shoulder rotation and the elbow rotation are both computed from the **same** pre-IK world
snapshot and pushed into `rotations`. The elbow's `aim_joint_local_rotation` uses the
*original* elbow world position/orientation, not the position the elbow will have *after* the
shoulder rotates. For a two-bone chain this leaves the forearm mis-oriented (hand/muzzle off
the aim line) even when the shoulder is correct — the aim looks "soft"/wrong close to the
dead zone and amplifies D1.

### D3 — `F` ejects instead of entering, and the health gate is mis-scaled — problem 2

Two independent issues in `detect_car_interaction`
([interaction_ui.rs:200-296](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L200)):

- **(D3a) Two-press eject/enter is now the *only* path for occupied cars.**
  Lines 274-295: if the target car has any `DriverMesh` child, `F` calls `eject_driver_as_ai`
  and returns; you must press `F` a *second* time on the now-empty car to enter. After eject,
  the ex-driver AI ped stands at `car − 2.0·X`
  ([interaction_ui.rs:447](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L447) /
  [car_disable.rs](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/car_disable.rs)),
  right where the player is standing. The crosshair raycast
  ([interaction_ui.rs:240](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L240))
  excludes only `ped_entity`, so on the second `F` the ray frequently hits the freshly-ejected
  ped (a person, not a `Car`), `car_root` resolves to `None`, and the function returns — the
  player *never* gets in. Symptom: "F does not get you in as driver anymore."

- **(D3b) Magic-number health gate.** Lines 268-272:
  ```rust
  if let Ok(car_health) = q_car_health.get(car) {
      if car_health.current < 100.0 { return; }   // blocks entry
  }
  ```
  `100.0` is a hard-coded copy of `CAR_DISABLE_HP`
  ([spawn_car.rs:84](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs#L84)).
  A car is marked `DisabledCar` at `health.current <= CAR_DISABLE_HP`
  ([car_disable.rs:43](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/car_disable.rs#L43)),
  so the correct "is this car wrecked?" test is the presence of `DisabledCar`, not a
  `< 100.0` float compare. As written a car sitting at exactly `100.0` is *disabled yet
  enterable*, and any future change to `CAR_DISABLE_HP` silently desyncs. (Fresh cars spawn at
  `1000.0` — [spawn_car.rs:165](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs#L165) —
  so this is not the *primary* cause but must be fixed alongside D3a.)

### D4 — Tracer quad is degenerate; smoke/tracer read as a dark sliver — problem 4

`gun_fx.rs` builds the tracer entity with a **rotated, non-uniformly scaled** transform
([gun_fx.rs:85-112](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/gun_fx.rs#L85)):
`rotation = from_rotation_arc(Vec3::X, shot_dir)`, `scale = (length, 1, 1)`. But the tracer
branch of the vertex shader
([billboard_fx.wgsl:32-42](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/billboard_fx.wgsl#L32))
**re-derives** the beam from `model[0].xyz` and rebuilds the quad itself:

```wgsl
let shot_vector = model[0].xyz;                 // = rotation*X * length  (length ~ metres)
world = center + shot_vector * pos.x + lateral * (pos.y * radius);
```

with `pos.x ∈ [-0.5, 0.5]`. Two defects fall out:

- **(D4a) Half-length beam, off-center.** The quad only spans `shot_vector * (±0.5)`, i.e.
  half the muzzle→impact length, centered on the segment midpoint — the visible streak is
  half as long as the shot and offset, so it reads as a stub, not a trail.
- **(D4b) Width mismatch → sub-pixel sliver.** `lateral` half-width is `pos.y * radius` with
  `pos.y ∈ [-0.5,0.5]` and `radius = tracer_width (0.04)`, giving a total width of `0.04 m`
  over a multi-metre beam — a hairline. Because it is additive over bright scene/sky, the only
  frame where it's dense enough to notice is where it overlaps a dark surface, so it looks like
  a "shadow of the trail." The fragment's `intensity = smoothstep(1,0,abs(uv.y))`
  ([billboard_fx.wgsl:104-108](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/billboard_fx.wgsl#L104))
  then fades most of that hairline to ~0. Net: near-invisible except as a dark edge.
- **(D4c) No screen-facing correction for length.** `lateral = normalize(cross(shot_dir, camera_dir))`
  is fine, but the beam is not camera-facing along its length, so when you look *down* the shot
  the quad collapses to a point (correct for a ribbon) yet gives no core glow — needs an
  additive HDR core independent of `abs(uv.y)`.

### D5 — Muzzle smoke never shows — problem 5

`tick_gun_smoke_emitters` ([gun_fx.rs:146-205](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/gun_fx.rs#L146))
emits `FxKind::SmokePuff` blend billboards, but they are effectively invisible for three
reasons that stack:

- **(D5a) Alpha far too low.** Smoke color is `vec4(0.7,0.7,0.7,0.25)`
  ([gun_fx.rs:182](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/gun_fx.rs#L182)); the fragment
  multiplies `alpha = puff * fade * color.a` with `fade = smoothstep(0,0.15,age)*(1-age) ≤ 1`
  and `puff ≤ 1`, so peak on-screen alpha is ≪ 0.25 — a barely-there grey smudge over a bright
  scene.
- **(D5b) Radius too small + short life.** `start 0.05 → end 0.45 m`, `lifetime 0.6 s`
  ([gun_fx.rs:186-188](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/gun_fx.rs#L186)):
  a sub-half-metre puff for 0.6 s at the muzzle is trivially missed.
- **(D5c) Emitter can attach to an entity with no usable `GlobalTransform` / muzzle extent.**
  The emitter is inserted on `model_state.entity.unwrap_or(shooter)`
  ([gun_fx.rs:67-77](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/gun_fx.rs#L67));
  `tick_gun_smoke_emitters` then needs `(&GlobalTransform, Option<&WeaponExtents>)` on it
  ([gun_fx.rs:152](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/gun_fx.rs#L152)). The weapon
  *model* entity carries `WeaponExtents` and a `GlobalTransform`, but if the emitter lands on
  the *shooter* (fallback) the puff spawns at the shooter origin with no muzzle offset — often
  inside the body mesh, occluded. There is also no reuse of the real muzzle point already
  computed in `fire_gun_observer` ([weapon_shooting.rs:183-199](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L183)).

### D6 — Clouds never render — problem 3

`clouds.rs` + `clouds.wgsl`. Several independent problems, at least the first two of which each
suppress the clouds on their own:

- **(D6a) Coverage threshold above the noise range → almost nothing passes.**
  `fbm` here is `0.5·v + 0.25·v + 0.125·v` with `v ∈ [0,1]`, so `fbm ∈ [0, 0.875]` and its
  *typical* value hovers ~0.35–0.45. The fragment does
  `d = smoothstep(u.coverage, 1.0, fbm)` with default `coverage = 0.45`
  ([settings.rs:52](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/settings.rs#L52),
  [clouds.wgsl:51](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/clouds.wgsl#L51)).
  With the low edge at 0.45 and fbm rarely exceeding it, `d ≈ 0` almost everywhere; the few
  pixels that pass are then multiplied by `opacity = 0.35`
  ([settings.rs:53](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/settings.rs#L53))
  → sub-0.1 alpha. Effectively nothing.
- **(D6b) Altitude/scale likely off-camera or degenerate.** The plane sits at `y = 120`
  ([clouds.rs:34](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/clouds.rs#L34)),
  `10000×10000 m`, `scale = 0.004`. Depending on the map's world scale and the camera far-plane
  the plane can be entirely above the frustum, and `world_pos.xz * 0.004` over ±5000 m gives
  fbm coordinates up to ±20 — fine, but combined with D6a there's nothing to see. Altitude must
  be validated against the actual map/camera (the existing skybox is drawn in
  `main_scene_plugin.rs`).
- **(D6c) No debug/solid mode to confirm the plane exists at all.** There is currently no way to
  tell "plane missing" from "plane invisible." A `debug_solid` path is needed to bisect D6a/D6b.
- **(D6d) `sync_cloud_uniforms` runs every frame writing the whole uniform** even when settings
  are unchanged ([clouds.rs:42-59](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/clouds.rs#L42));
  harmless correctness-wise but should be gated on `settings.is_changed()`.

### D7 — No VFX test harness — problem 6

There is only a hidden `V`-key debug fireball at the origin
([mod.rs:55-86](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/mod.rs#L55)).
There is no binary equivalent to `bin/audio_demo.rs` that lets you pick an effect and spawn it
at a clicked 3D point, which is exactly the tool needed to iterate on D4/D5/D6 without booting
the whole game.

---

## 3. Fixes

### F1 — Robust arm aim (fixes D1 + D2)

Replace the singular `from_rotation_arc` aim + one-shot spine hack with (a) a **character/spine
yaw pre-align** that always brings the target within the shoulder's natural cone, (b) an
antiparallel-safe rotation helper, and (c) a **re-FK between shoulder and elbow**.

**F1a — antiparallel-safe minimal rotation.** New helper in `arm_ik.rs`:

```rust
/// `from_rotation_arc` that stays continuous through the antiparallel case by falling back to a
/// stable reference axis (character up, then character right) instead of glam's arbitrary pick.
fn safe_rotation_arc(from: Vec3, to: Vec3, fallback_axis: Vec3) -> Quat {
    let a = from.normalize_or_zero();
    let b = to.normalize_or_zero();
    if a.length_squared() < 1e-6 || b.length_squared() < 1e-6 {
        return Quat::IDENTITY;
    }
    let d = a.dot(b).clamp(-1.0, 1.0);
    if d < -0.9995 {
        // ~antiparallel: spin 180° about a well-defined axis perpendicular to `a`.
        let axis = a.cross(fallback_axis).normalize_or_zero();
        let axis = if axis.length_squared() < 1e-6 {
            a.cross(Vec3::Y).normalize_or_zero()
        } else {
            axis
        };
        return Quat::from_axis_angle(axis, std::f32::consts::PI);
    }
    Quat::from_rotation_arc(a, b)
}
```

`aim_joint_local_rotation` ([arm_ik.rs:106](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L106))
changes its signature to take the fallback axis and use the helper:

```rust
fn aim_joint_local_rotation(
    joint_world: &Transform,
    parent_world: &Transform,
    child_pos: Vec3,
    desired_child_pos: Vec3,
    fallback_axis: Vec3,           // NEW: pass char_up (char_gt.rotation()*Vec3::Y)
) -> Quat {
    let joint_pos = joint_world.translation;
    let current_dir = (child_pos - joint_pos).normalize_or_zero();
    let desired_dir = (desired_child_pos - joint_pos).normalize_or_zero();
    if current_dir.length_squared() < 1e-6 || desired_dir.length_squared() < 1e-6 {
        return joint_world.rotation;
    }
    let delta_world = safe_rotation_arc(current_dir, desired_dir, fallback_axis);
    let new_world_rot = delta_world * joint_world.rotation;
    parent_world.rotation.inverse() * new_world_rot
}
```

**F1b — continuous yaw pre-align (replaces `compute_spine_compensation`).** Instead of a
dead-zone step function, always rotate the spine (or, preferably, the whole character root when
in `ControllingPedestrian`) by the **horizontal** yaw between character-forward and
target-direction, *smoothly*, so the arm's required swing never approaches 180°:

```rust
/// Horizontal yaw (radians, signed) the torso should add so the target sits in front.
/// Continuous: 0 when already facing the target, growing monotonically, clamped to a max.
fn torso_yaw_toward(char_forward: Vec3, to_target: Vec3, max_yaw: f32) -> f32 {
    let fwd = Vec3::new(char_forward.x, 0.0, char_forward.z).normalize_or_zero();
    let to  = Vec3::new(to_target.x,   0.0, to_target.z).normalize_or_zero();
    if fwd.length_squared() < 1e-6 || to.length_squared() < 1e-6 { return 0.0; }
    let ang = Vec2::new(fwd.x, fwd.z).angle_to(Vec2::new(to.x, to.z)); // (-π, π]
    // Only compensate the part beyond the shoulder's comfortable ~70° cone.
    const COMFORT: f32 = 70.0 * std::f32::consts::PI / 180.0;
    let excess = (ang.abs() - COMFORT).max(0.0);
    excess.copysign(ang).clamp(-max_yaw, max_yaw)
}
```

Apply it to the spine world rotation about the **character's** up axis (not world-Y):

```rust
let up = char_up;                                   // char_gt.rotation() * Vec3::Y
let yaw = torso_yaw_toward(char_forward, target - char_pos, 60f32.to_radians());
let new_world_rot = Quat::from_axis_angle(up, yaw) * spine_world.rotation;
rotations.push((spine_ent, parent_world.rotation.inverse() * new_world_rot));
```

**F1c — re-FK the shoulder before solving the elbow (fixes D2).** After computing the spine and
shoulder local rotations, *recompute* the shoulder/elbow/wrist world transforms with those
rotations applied before solving the elbow. Two viable approaches:

- **Cheap:** compute the shoulder's new world rotation, then transform the *rest-pose* elbow
  offset by it to get the elbow's post-shoulder world position, and solve the elbow aim against
  that. i.e. thread the freshly-computed rotations through a small local FK helper rather than
  re-reading `transform_sets.p0()`.
- **Correct/general:** change `apply_arm_chain_ik` to write shoulder rotation into a scratch
  `HashMap<Entity, Quat>` overlay, and make `world_transform` consult the overlay so downstream
  joints see updated parents:

```rust
fn world_transform_with_overrides(
    entity: Entity,
    transforms: &Query<&Transform>,
    parents: &Query<&ChildOf>,
    overrides: &HashMap<Entity, Quat>,   // NEW: pending local-rotation writes
) -> Option<Transform> {
    // identical chain walk to world_transform, but for each `ent` use
    // overrides.get(&ent) to replace the local rotation before mul_transform.
}
```

Solve order becomes: spine → (record) → shoulder → (record) → elbow, each reading the overlay.

**F1d — verification hooks.** Behind a `cfg!(debug_assertions)` gizmo, draw a line from the
wrist along the forearm and a second from the wrist to `aim_point`; when correct they coincide.
This makes the fix observable in `bin/pedestrian_controller` (RMB to aim).

> Scope note: keep the existing gate structure in `apply_arm_ik`
> ([arm_ik.rs:273-372](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L273))
> — both `ControllingPedestrian` and `DrivingCar` call `write_arm_ik_rotations`; only the math
> inside `apply_arm_chain_ik` changes, so both paths are fixed at once.

### F2 — `F` reliably enters an empty car (fixes D3)

In `detect_car_interaction`
([interaction_ui.rs:200](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L200)):

- **F2a — exclude people (and the target's own occupants) from the interaction raycast**, so a
  freshly-ejected driver can't swallow the second `F`. Build the filter to exclude the player
  *and* any `CharacterController`/`DriverMesh`, or resolve `hit.entity` up its parent chain and,
  if it lands on a person rather than a `Car`, re-cast past it. Minimal version — widen the
  excluded set:

  ```rust
  let mut excluded = vec![ped_entity];
  excluded.extend(q_people.iter());          // Query<Entity, With<CharacterController>>
  let filter = SpatialQueryFilter::default().with_excluded_entities(excluded);
  ```

- **F2b — replace the magic-number health gate with a `DisabledCar` check.** Add
  `q_disabled: Query<(), With<DisabledCar>>` and:

  ```rust
  if q_disabled.get(car).is_ok() {
      return; // wrecked cars are not enterable
  }
  ```

  Delete the `car_health.current < 100.0` block entirely (and its `q_car_health` param if now
  unused). `DisabledCar` is the single source of truth
  ([car_disable.rs:49](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/car_disable.rs#L49)).

- **F2c — make "enter" the default for an *unoccupied* car and keep eject only for occupied
  cars** (unchanged), but after an eject, **do not require re-aiming through the crosshair**:
  set a short-lived `PendingEnterCar { car, until }` on the player so the very next `F` (or an
  auto-enter after the eject animation) seats them without a second raycast. This removes the
  "ray hits the ejected ped" failure entirely:

  ```rust
  #[derive(Component)]
  pub struct PendingEnterCar { pub car: Entity, pub until: f32 } // elapsed-secs deadline
  ```

  If `PendingEnterCar` is present and not expired, `F` bypasses the raycast and goes straight to
  `EnteringCarTimer` for that car.

- **F2d — relax the proximity check for the enter case.** The `> 1.2 m` crosshair-hit-distance
  test ([interaction_ui.rs:263](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L263))
  is brittle in third-person (screen center rarely lands within 1.2 m of a car you're standing
  next to). Change to "player *body* within `ENTER_RADIUS` (e.g. 3.0 m) of the car root
  transform," independent of where the crosshair points:

  ```rust
  const ENTER_RADIUS: f32 = 3.0;
  if ped_tf.translation().distance(car_gt.translation()) > ENTER_RADIUS { return; }
  ```

> These four are independent; F2a+F2b alone restore entry in the common case, F2c/F2d harden UX.

### F3 — Fix the tracer ribbon (fixes D4)

Decide the ribbon geometry in **one** place. Recommended: keep the CPU simple (spawn at
midpoint, identity scale) and build the full-length, fixed-*screen*-width ribbon in the vertex
shader from explicit endpoints passed in the uniform. This removes the model-matrix coupling
that caused D4a/D4b.

**F3a — carry beam endpoints + width in the uniform.** Extend `BillboardParams`
([materials.rs:16](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs#L16))
using the existing padding budget (add a second uniform block rather than breaking the
16-byte-aligned `BillboardParams`, to avoid disturbing every other kind):

```rust
/// Extra uniform bound at @group(2) @binding(1); only meaningful for FxKind::Tracer.
#[derive(Clone, Copy, ShaderType, Debug, Default)]
pub struct TracerParams {
    pub muzzle: Vec4,   // xyz = world muzzle, w unused
    pub impact: Vec4,   // xyz = world impact, w unused
    pub width:  f32,    // world half-width fallback
    pub _pad0:  f32,
    pub _pad1:  f32,
    pub _pad2:  f32,
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct AdditiveFxMaterial {
    #[uniform(0)] pub params: BillboardParams,
    #[uniform(1)] pub tracer: TracerParams,   // NEW (default for non-tracers)
}
```

(Or, if adding a binding is undesirable, reuse `start_radius`/`end_radius` for width and pack
`muzzle`/`impact` by spawning the entity at `muzzle` with `Transform` whose translation is
`muzzle` and whose `model[0]` is the *full* `impact − muzzle` vector — then fix the shader math
in F3b to use `pos.x ∈ [0,1]` mapping. Endpoints-in-uniform is cleaner and is the recommended
path.)

**F3b — vertex shader: full-length, camera-width ribbon.** Replace
[billboard_fx.wgsl:32-42](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/billboard_fx.wgsl#L32):

```wgsl
if (P.kind == 5u) {
    // Map pos.x ∈ [-0.5,0.5] → t ∈ [0,1] along the beam; pos.y → lateral offset.
    let t = pos.x + 0.5;
    let a = T.muzzle.xyz;
    let b = T.impact.xyz;
    let beam = b - a;
    let point = a + beam * t;

    let cam = view.world_position - point;
    let beam_dir = normalize(beam);
    var lateral = normalize(cross(beam_dir, cam));
    if (!(dot(lateral, lateral) > 0.0)) { lateral = view.world_from_view[1].xyz; } // parallel guard

    // Constant *screen*-ish width: scale by distance so the ribbon keeps a visible thickness.
    let dist = length(cam);
    let half_w = max(T.width, 0.0025 * dist);
    world = point + lateral * (pos.y * 2.0 * half_w);
    out.uv = vec2<f32>(t, pos.y * 2.0);           // uv.x = along-beam 0..1, uv.y = ∈[-1,1]
}
```

**F3c — fragment: bright HDR core + soft edge.** Replace
[billboard_fx.wgsl:104-108](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/billboard_fx.wgsl#L104):

```wgsl
case 5u: { // Tracer: hot core, soft lateral falloff, fade along life
    let edge = smoothstep(1.0, 0.0, abs(in.uv.y));  // 1 at core → 0 at edge
    let core = pow(edge, 4.0);                       // tight bright center
    rgb = mix(P.color.rgb, vec3(1.0), core);         // whiten the core
    alpha = (0.5 * edge + 0.6 * core) * (1.0 - age) * P.color.a;
}
```

**F3d — spawn side (`gun_fx.rs`).** Stop rotating/scaling; spawn at the midpoint (or muzzle)
with identity transform and fill `TracerParams`:

```rust
let tracer = TracerParams {
    muzzle: muzzle.extend(0.0),
    impact: impact.extend(0.0),
    width: settings.tracer_width.max(0.02),
    ..default()
};
let mat = additive_mats.add(AdditiveFxMaterial {
    params: BillboardParams { kind: FxKind::Tracer as u32, lifetime: 0.06, /* … */ ..base },
    tracer,
});
commands.spawn((
    Mesh3d(meshes.quad.clone()),
    MeshMaterial3d(mat),
    Transform::from_translation((muzzle + impact) * 0.5),
    VfxLifetime { despawn_at },
));
```

Every other `spawn_additive_billboard_fx` call site must now also pass `TracerParams::default()`
— update the helper signature in `spawn.rs`
([spawn.rs:19](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/spawn.rs#L19))
to fill it automatically for non-tracer kinds.

### F4 — Make muzzle smoke visible (fixes D5)

- **F4a — spawn smoke at the *real* muzzle from the event, not from an emitter on an uncertain
  entity.** `GunFxEvent` already carries `muzzle`
  ([gun_fx.rs:8-14](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/gun_fx.rs#L8)).
  In `gun_fx_observer`, spawn 1–2 smoke puffs immediately at `event.muzzle` with an upward
  `VfxDrift`, and reserve the persistent `GunSmokeEmitter` only for a light lingering wisp. This
  sidesteps D5c (no dependency on `WeaponModelState.entity` having a `GlobalTransform`).

- **F4b — raise alpha, size, and life.** Use blend smoke with:
  ```rust
  BillboardParams {
      color: Vec4::new(0.8, 0.8, 0.82, 0.7),   // was a=0.25
      lifetime: 0.9,                            // was 0.6
      start_radius: 0.15,
      end_radius: 0.9,                          // was 0.45
      seed: rand::random(),
      kind: FxKind::SmokePuff as u32, ..
  }
  ```
- **F4c — strengthen the smoke fragment fade** so low `age` isn't crushed
  ([billboard_fx.wgsl:90-94](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/billboard_fx.wgsl#L90)):
  ```wgsl
  case 1u, 2u: {
      let puff = disk * (0.6 + 0.4 * n);
      let fade = smoothstep(0.0, 0.08, age) * (1.0 - age); // faster fade-in
      alpha = puff * fade * P.color.a;
  }
  ```
- **F4d — keep the emitter, but fix its attach target** to prefer the weapon-model entity and
  early-out cleanly if it lacks a `GlobalTransform`; throttle to every Nth shot on automatic
  weapons via a `VfxSettings.muzzle_smoke_every` field to avoid clutter.

### F5 — Make clouds visible (fixes D6)

- **F5a — retune defaults** ([settings.rs:52-56](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/settings.rs#L52)):
  `cloud_coverage: 0.30` (below typical fbm), `cloud_opacity: 0.7`, keep `wind ~0.005`,
  `scale 0.0015`.
- **F5b — normalize the noise and soften the threshold** in `clouds.wgsl`
  ([clouds.wgsl:47-52](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/clouds.wgsl#L47)):
  ```wgsl
  @fragment
  fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
      let uv = in.world_pos.xz * u.scale + globals.time * u.wind;
      // fbm ∈ [0,0.875]; remap to ~[0,1] so `coverage` behaves intuitively.
      let n = clamp(fbm(uv) / 0.875, 0.0, 1.0);
      let d = smoothstep(u.coverage, u.coverage + 0.35, n); // wider ramp than (coverage,1)
      if (d <= 0.001) { discard; }
      return vec4<f32>(u.color.rgb, d * u.opacity * u.color.a);
  }
  ```
- **F5c — add a `debug_solid` uniform + toggle** to render the plane as flat 30%-alpha white,
  independent of noise, to bisect "missing" vs "invisible":
  ```rust
  pub struct CloudParamsUniform { /* … existing … */ pub debug_solid: f32 /* 0/1 */, pub _pad3: f32 }
  ```
  ```wgsl
  if (u.debug_solid > 0.5) { return vec4<f32>(1.0, 1.0, 1.0, 0.3); }
  ```
  Expose it in `ui.rs` and default off.
- **F5d — validate altitude against the real scene.** Confirm the camera far-plane exceeds the
  cloud altitude and that `y = 120` sits above the map surface at play areas; if the map is
  larger-scale, raise altitude and increase plane size proportionally. Use F5c to confirm the
  plane is in frustum first. Keep `cull_mode: None`
  ([materials.rs:130-138](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs#L130))
  so it shows from below.
- **F5e — gate `sync_cloud_uniforms` on `settings.is_changed()`**
  ([clouds.rs:42](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/clouds.rs#L42)).

### F6 — `vfx_demo` binary (fixes D7)

**New files:**

```
src/bin/vfx_demo.rs                       # app harness, mirrors bin/audio_demo.rs
src/plugins/visual_fx/demo.rs             # VfxDemoPlugin: click-to-spawn + picker UI
```

**`src/plugins/visual_fx/demo.rs`** — new module (add `pub mod demo;` to `visual_fx/mod.rs`;
keep it out of the default `VisualFXPlugin` so it only loads in the demo binary):

```rust
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use super::materials::{AdditiveFxMaterial, BlendFxMaterial, BillboardParams, FxKind};
use super::spawn::{spawn_additive_billboard_fx, spawn_blend_billboard_fx, VfxMeshes, VfxDrift};
use super::car_explosion::CarExplosionEvent;
use super::gun_fx::GunFxEvent;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DemoEffect {
    Fireball, ExplosionSmoke, BlackSmoke, MuzzleFlash, SparkBurst, Tracer,
    MuzzleSmoke, CarExplosion /* full combo */,
}
impl DemoEffect {
    pub const ALL: [DemoEffect; 8] = [ /* … */ ];
    pub fn label(self) -> &'static str { /* … */ }
}

#[derive(Resource)]
pub struct VfxDemoState {
    pub selected: DemoEffect,
    pub last_pick: Option<Vec3>,
    pub last_spawn: f32,        // debounce, mirrors SOUND_DEBOUNCE_SECS
    pub auto_face_camera: bool, // spawn tracers pointing at camera etc.
}
impl Default for VfxDemoState { /* Fireball, none, -1.0, true */ }

pub struct VfxDemoPlugin;
impl Plugin for VfxDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VfxDemoState>()
            .add_systems(Update, (click_ground_to_spawn, draw_pick_gizmo))
            .add_systems(EguiPrimaryContextPass, vfx_demo_ui);
    }
}

/// Left-click the ground → raycast → spawn the selected effect at the hit point.
/// Mirrors `audio::click_ground_to_play` (debounce, egui-guard, viewport_to_world).
fn click_ground_to_spawn(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    spatial: SpatialQuery,
    meshes: Option<Res<VfxMeshes>>,
    mut additive: ResMut<Assets<AdditiveFxMaterial>>,
    mut blend: ResMut<Assets<BlendFxMaterial>>,
    mut state: ResMut<VfxDemoState>,
    mut contexts: EguiContexts,
) { /* see behavior below */ }

fn draw_pick_gizmo(state: Res<VfxDemoState>, mut gizmos: Gizmos) { /* sphere+line marker */ }

fn vfx_demo_ui(mut contexts: EguiContexts, mut state: ResMut<VfxDemoState>) {
    // egui::Window "🎆 VFX Demo": radio list of DemoEffect::ALL + a hint line.
}
```

`click_ground_to_spawn` behavior, per `DemoEffect`:
- `Fireball / ExplosionSmoke / BlackSmoke / MuzzleFlash / SparkBurst / MuzzleSmoke`: build the
  matching `BillboardParams` (reuse the exact params from `car_explosion.rs` / `gun_fx.rs`) and
  call the corresponding spawn helper at `hit_point`.
- `Tracer`: spawn from `hit_point` to `hit_point + Vec3::Y*3.0` (or toward the camera when
  `auto_face_camera`) via the F3 tracer path, so ribbon geometry is testable in isolation.
- `CarExplosion`: `commands.trigger(CarExplosionEvent { position: hit_point })` to exercise the
  full fireball+smoke+emitter combo and the observer wiring.

**`src/bin/vfx_demo.rs`** — mirror `bin/audio_demo.rs`
([bin/audio_demo.rs](../crack_demo/demo_resolution_selector_web_bevy/src/bin/audio_demo.rs)):

```rust
use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        physics_plugin::PhysicsPlugin,
        visual_fx::{VisualFXPlugin, demo::VfxDemoPlugin},
    },
    ui_egui::UiState,
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("VFX Demo")
        .add_plugins(bevy_egui::EguiPlugin::default())
        .insert_resource(UiState::with_physics_debug()) // VfxSettings window toggled via UiState
        .add_plugins(PhysicsPlugin)                      // SpatialQuery for ground raycast
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(VisualFXPlugin)                     // registers all 3 materials + systems
        .add_plugins(VfxDemoPlugin)
        .run();
}
```

Notes:
- `VisualFXPlugin::build` already registers the three `MaterialPlugin`s and `embedded_asset!`s
  ([mod.rs:31-38](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/mod.rs#L31)),
  so the demo needs nothing extra for shaders. It also spawns clouds at `Startup`
  ([mod.rs:41](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/mod.rs#L41)),
  which conveniently makes the demo a cloud test too.
- The `V`-key origin fireball
  ([mod.rs:55-86](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/mod.rs#L55))
  should be removed from `VisualFXPlugin` (or gated behind `cfg!(debug_assertions)`) now that the
  demo binary supersedes it.
- `VfxSettings` window: gate `ui::vfx_controls_window` on `UiState.show_vfx_shaders`
  ([ui.rs:14](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/ui.rs#L14));
  in the demo, default it open so all toggles/sliders are reachable while clicking.

---

## 4. Ordered implementation checklist

1. **F6 (`vfx_demo` binary).** Build the harness first — every other VFX fix is validated by
   clicking effects into the debug scene. `cargo run --bin vfx_demo`.
2. **F3 (tracer).** Fix ribbon length + width + core; verify with the `Tracer` demo effect.
3. **F4 (muzzle smoke).** Verify with `MuzzleSmoke` and `CarExplosion` demo effects.
4. **F5 (clouds).** Toggle `debug_solid` to confirm the plane, then tune coverage/opacity.
5. **F2 (`F` enters car).** Needs the full game (`bin/…` main app) with traffic; verify enter on
   both empty and occupied cars.
6. **F1 (arm IK).** Verify in `bin/pedestrian_controller` (RMB aim) with the F1d gizmo, rotating
   the character through a full 360° to confirm no behind-the-back flip at any yaw.

Steps 1–4 are pure VFX and independent; 5 and 6 are gameplay and independent of the VFX chain.

---

## 5. Verification

- **Arm IK (F1):** in `bin/pedestrian_controller`, hold RMB and orbit the camera a full turn
  around the character while keeping the crosshair on a wall. The forearm/muzzle line
  (F1d gizmo) must track the crosshair continuously with **no** frame where it snaps behind the
  torso. Repeat while `DrivingCar` (drive-by aim).
- **Enter car (F2):** approach an occupied traffic car, press `F` (ejects driver), press `F`
  again → player seats as driver (state → `DrivingCar`, weapon HUD appears). Approach an empty
  freecam-spawned car, single `F` → seats immediately. Shoot a car to `DisabledCar` → `F` is
  refused.
- **Tracer (F3):** in `vfx_demo`, spawn `Tracer`; the ribbon spans the full segment, keeps a
  visible width at range, has a bright white core, and is never a thin dark line. In-game, fire
  a gun and confirm the trail reads as a glowing streak.
- **Muzzle smoke (F4):** in `vfx_demo`, `MuzzleSmoke` produces a clearly visible ~1 m grey puff
  that rises and fades over ~0.9 s. In-game, firing leaves a puff at the muzzle.
- **Clouds (F5):** `debug_solid` on → flat white haze overhead confirms the plane renders; off →
  drifting fBm clouds visible against the sky at default `coverage 0.30 / opacity 0.7`.
- **Demo binary (F6):** `cargo run --bin vfx_demo` boots the flat scene; the "🎆 VFX Demo" panel
  lists all effects; left-clicking the ground spawns the selected effect at the pick marker;
  `CarExplosion` fires the full combo.

---

## 6. Notes / decisions to confirm with the author

- **IK root vs spine rotation (F1b).** When `ControllingPedestrian`, rotating the *character
  root* yaw to face the aim (like most third-person shooters) is more robust than spine-only
  compensation and also aligns locomotion; but it changes movement feel. This plan keeps
  spine-only to avoid gameplay changes — flag if a full torso/root turn is preferred.
- **Two-press car entry (F2c).** The eject-then-enter design is intentional; F2 keeps it but
  removes the raycast trap. If single-press "enter, pushing any occupant out" is wanted instead,
  that's a larger UX change — confirm.
- **Tracer uniform binding (F3a).** Adding `@binding(1)` touches `AsBindGroup` for
  `AdditiveFxMaterial` and every spawn call site. The alternative (pack endpoints into the model
  matrix) avoids the binding but keeps the fragile matrix coupling. Recommend the extra binding.
- **Cloud altitude (F5d).** `y = 120` is a guess from v1; must be checked against the real map
  scale and camera far-plane. Left as a verification step, not a hard-coded change.
