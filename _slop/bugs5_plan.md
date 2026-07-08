# Plan: Pedestrian controls, entity/driver decoupling, drive-by shooting


---

## 1. Pedestrian controlling logic changes

All paths below are relative to `crack_demo/demo_resolution_selector_web_bevy/src/`.


### 1a. Melee weapons: continuous fire / full auto on hold

**Current behavior:** Melee and unarmed attacks fire only via `just_pressed(Left)` or when `weapon_id.automatic()` is true. `automatic()` returns `false` for `Unarmed` and `Melee(_)` ([weapon_manifest.rs:88](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_manifest.rs#L88)). This means melee cannot repeat while the button is held.

The fire gate in `drive_character_animation` ([animation.rs:148-150](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L148-L150)):
```rust
let fire_pressed = !over_ui
    && (mouse.just_pressed(MouseButton::Left)
        || (mouse.pressed(MouseButton::Left) && weapon_id.automatic()));
```

**Fix:** Change `weapon_id.automatic()` to `weapon_id.automatic() || weapon_id.is_melee() || weapon_id.is_unarmed()`. This makes all melee and unarmed attacks behave like full-auto: holding LMB repeats swings, gated by `WeaponCooldown` (which already ticks at `60/rpm` per attack).

Alternatively (cleaner): change `WeaponId::automatic()` in `weapon_manifest.rs` to return `true` for `Unarmed` and `Melee(_)` as well. This is the single-point-of-truth approach.

**Files:**
- `plugins/weapons/weapon_manifest.rs`: change `fn automatic()` to return `true` for `Unarmed | Melee(_)`.

No other changes needed — the cooldown (`WeaponCooldown`) already paces attacks per `rpm()`.

| 1a | `plugins/weapons/weapon_manifest.rs` |
1. **1a Melee auto-fire:** Hold LMB with a melee weapon — character should swing repeatedly at the weapon's RPM cadence.
Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`).  may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 



----------------



### 1b. Sprint speed +50% and sprinting sound pitch ×0.6

**Current sprint speed:** Max sprint speed = `SPRINT_MAX_MULT * JOG_SPEED` = `1.5 * 4.0 = 6.0 m/s` ([mod.rs:82](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L82)). The sprint ramps from `1.0 * JOG_SPEED` up to `SPRINT_MAX_MULT * JOG_SPEED` while Shift is held. The ramp equation at [controller.rs:213-214](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs#L213-L214):
```rust
let t = modifiers.sprint_secs / SPRINT_RAMP_TIME;
JOG_SPEED * (1.0 + (SPRINT_MAX_MULT - 1.0) * t)
```
Raising sprinting speed 50% faster means `SPRINT_MAX_MULT` goes from `1.5` to `1.5 * 1.5 = 2.25`. But note the user said "raise sprinting speed 50% faster" meaning the sprint speed itself should be 50% faster, not the multiplier. Current max sprint: 6.0 m/s → new target: 9.0 m/s → `SPRINT_MAX_MULT = 9.0 / 4.0 = 2.25`.

**Fix:**
- `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`: change `SPRINT_MAX_MULT` from `1.5` to `2.25`.

**Sprinting sound pitch:** Footstep sound is managed by `manage_footsteps_system` in [audio_fx.rs:241-319](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/audio_fx.rs#L241-L319). The playback speed is currently set at [audio_fx.rs:305-311](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/audio_fx.rs#L305-L311):
```rust
let playback_speed = if speed < 2.2 {
    0.9
} else if speed < 6.0 {
    1.3
} else {
    1.7
};
```
The highest tier (`speed >= 6.0`, i.e. sprinting) plays at `1.7` speed/pitch. "Lower the sprinting sound loop 60% of the pitch" means multiply by 0.6: `1.7 * 0.6 = 1.02`. But more likely the user means the entire sprint footstep pitch should be 60% of its current value.

**Fix:**
- `plugins/audio/audio_fx.rs`: in `manage_footsteps_system`, add `MovementModifiers` to the query (or use the speed threshold). When speed is in the sprint range (`>= 6.0`), multiply the existing `playback_speed` by `0.6`. Result: `1.7 * 0.6 = 1.02` pitch.
- The easiest approach: change the `1.7` to `1.02` directly, and also adjust the `6.0` threshold to account for the new higher sprint speed (now up to 9.0 m/s). New thresholds: `speed < 2.2` → 0.9, `speed < 5.0` → 1.3, `else` → 1.02.

**Files:**
- `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`: `SPRINT_MAX_MULT = 2.25`
- `plugins/audio/audio_fx.rs`: lower sprint-tier `playback_speed` to `1.02`


| 1b | `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`, `plugins/audio/audio_fx.rs` |
2. **1b Sprint:** Sprint should reach ~9 m/s top speed (50% faster than before). Footstep sound while sprinting should be noticeably lower-pitched.
Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`).  may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 



------------------------------------------




### 1c. Follow camera: shoulder offset + right-click aim with zoom

**Current camera:** The follow camera in [camera.rs:54-116](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs#L54-L116) positions itself at:
```rust
let anchor = pos_target + Vec3::Y * CAM_LOOK_HEIGHT;  // 1.1m above character
let offset = Quat::from_euler(YXZ, yaw, pitch, 0.0) * Vec3::new(0.0, 0.0, CAM_DISTANCE);
```
This puts the camera directly behind/above the head with no lateral offset.

Orbit input (`orbit_camera_input` [camera.rs:36-52](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs#L36-L52)): rotates on `Left` mouse button or captured mouse.

Right-click is currently used for gun aim in `drive_character_animation` ([animation.rs:151](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L151)):
```rust
let rmb = !over_ui && mouse.pressed(MouseButton::Right);
```
When RMB is held with a gun equipped, it enters `CombatKind::Aim` playing `Pistol_Idle_Loop`.

**Changes:**

1. **Shoulder offset:** Add a lateral offset to the camera anchor so it's over the right shoulder instead of the head center.
   - In `follow_camera`, change:
     ```rust
     let anchor = pos_target + Vec3::Y * CAM_LOOK_HEIGHT;
     ```
     to:
     ```rust
     let shoulder_offset = Quat::from_rotation_y(rig.yaw) * Vec3::new(0.6, 0.0, 0.0);
     let anchor = pos_target + Vec3::Y * CAM_LOOK_HEIGHT + shoulder_offset;
     ```
   - Add new constants: `CAM_SHOULDER_X: f32 = 0.6` (lateral offset to the right).

2. **RMB → Aim mode with zoom:** Add an `aiming: bool` field to `CameraRig`. When RMB is pressed:
   - Set `rig.aiming = true`.
   - Zoom the camera closer: use `CAM_AIM_DISTANCE` (e.g. 2.5) instead of `CAM_DISTANCE` (5.5). Smoothly interpolate between them.
   - If the weapon is a gun, play the aim animation (`Pistol_Idle_Loop` / `Pistol_Aim_Neutral`) — this already happens in `drive_character_animation`.
   - If unarmed or melee, zoom in the camera the same way but don't trigger any special animation (existing behavior: `CombatKind::Aim` gate is `is_gun && rmb`, so melee/unarmed already skip the aim animation). We need to extend the zoom to work for all weapon types.

3. **Change orbit from LMB to always-on (captured) or separate logic:**
   - Currently LMB does both orbit *and* fire. The orbit fires on `pressed(Left)`, combat on `just_pressed(Left)`. This works because combat reads the *edge*, orbit reads the *hold*.
   - With RMB repurposed as aim, we should change orbit to work on captured mouse motion (already supported via `capture_state.is_captured`). Left-mouse drag should still orbit. No change needed here — the existing logic already handles both.

**Implementation in `CameraRig` and `follow_camera`:**
- Add `aiming: bool` to `CameraRig`. Default `false`.
- In `follow_camera`, read RMB state (pass `mouse: Res<ButtonInput<MouseButton>>` to the system). Set `rig.aiming = mouse.pressed(Right) && !over_ui`.
- Compute `target_distance = if rig.aiming { CAM_AIM_DISTANCE } else { CAM_DISTANCE }`. Smoothly lerp `rig.current_distance` toward `target_distance`.
- Also narrow the shoulder offset when aiming: `shoulder_x = if rig.aiming { 0.3 } else { 0.6 }`.

**Animation side (existing, needs extension):**
- In `drive_character_animation`, the aim branch is `is_gun && rmb` → `CombatKind::Aim` playing `Pistol_Idle_Loop`. For unarmed/melee + RMB, we currently get `CombatKind::None` (no aim animation). Per spec: "when unarmed or using melee weapon, aiming will zoom in the camera in the same way, but will not show any special animations." → No animation change needed for melee/unarmed aim. The camera zoom handles it.

**New constants:**
```rust
const CAM_SHOULDER_X: f32 = 0.6;
const CAM_AIM_SHOULDER_X: f32 = 0.3;
const CAM_AIM_DISTANCE: f32 = 2.5;
const CAM_ZOOM_SPEED: f32 = 8.0;  // smooth zoom lerp rate
```

**Files:**
- `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`: new constants
- `plugins/pedestrians/pedestrian_controller_plugin/camera.rs`: shoulder offset, aim zoom, `CameraRig` gets `aiming: bool` and `current_distance: f32`


| 1c | `plugins/pedestrians/pedestrian_controller_plugin/camera.rs`, `mod.rs` |
3. **1c Camera/Aim:** Camera should be offset over the right shoulder. Holding RMB should zoom the camera closer. With a gun, aiming animation plays. Without a gun, camera zooms but no special animation.
Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`).  may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 



----------------------




### 1d. Gun idle pose: follow forearm bone when not aiming/shooting

**Current behavior:** `update_weapon_transforms` in [weapon_attach.rs:401-487](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L401-L487) always points the gun at the camera-ray target:
```rust
let aim_dir = Some((target - weapon_world_pos).normalize_or_zero());
// ...then builds a rotation matrix from aim_dir as the X axis
```
This means the gun is *always* pointed perfectly at the crosshair target, even when the player is just jogging around not shooting or aiming. It should instead rest naturally in the hand.

**Fix:** Only apply the aim-at-target rotation when the player is actively aiming (RMB held) or shooting. Otherwise, let the weapon inherit the wrist bone's rotation with a simple grip offset (same direction as the forearm/wrist bone, Y up).

Implementation:
1. Add a **new resource or component** `WeaponAimState` (or extend `CombatState`) so `update_weapon_transforms` can read whether the local player is aiming/shooting. Simplest approach: add `pub aiming: bool` to `CameraRig` (already planned in 1c) and read it here. Or use `CombatKind` from the controller's `CombatState`.
2. In `update_weapon_transforms`, for local player guns:
   - If `rig.aiming` or `combat.kind != CombatKind::None`:  → current behavior (aim at target).
   - Else (idle):  → set `transform.rotation` to a fixed local rotation that aligns with the forearm bone direction, Y up. This is: `Quat::IDENTITY` for gun (the default parent-relative orientation, since it's parented to the wrist bone, it will naturally follow the forearm direction).

The skeleton has `RightArm` (forearm/elbow-to-wrist) bone labeled at [skeleton.rs:17](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/skeleton.rs#L17). The wrist-to-forearm direction can be computed from the wrist's `GlobalTransform` local Y axis (the bone axis). When idle, we rotate the gun so its X axis (barrel direction) aligns with the wrist bone's local direction, and set Y = world up.

**Simplest approach:** When not aiming/shooting, just set `transform.rotation = Quat::IDENTITY` for guns (same as melee, which already does `from_rotation_x(90°)`). This makes the gun follow the natural wrist bone orientation. If that looks wrong, use a small fixed rotation like `Quat::from_rotation_z(-90°.to_radians())` to have the barrel point along the forearm.

**Query change:** `update_weapon_transforms` needs to know the local player's combat/aim state. Options:
- Read `Res<CameraRig>` for `aiming`.
- Walk up the weapon's parent hierarchy to find the controller and check its `CombatState`.
- Add a small marker `WeaponAiming` component on the controller entity, toggled by the animation system.

Best approach: read `Res<CameraRig>` (only applies to local player, remote uses facing direction which is fine).

**Files:**
- `plugins/weapons/weapon_attach.rs`: `update_weapon_transforms` — gate aim-at-target on `rig.aiming` or active combat. Add `Res<CameraRig>` param.
- `plugins/pedestrians/pedestrian_controller_plugin/camera.rs`: export `CameraRig` (already `pub`).

| 1d | `plugins/weapons/weapon_attach.rs`, `camera.rs` |
4. **1d Gun idle:** Gun should rest naturally along the forearm when not aiming or shooting. When aiming (RMB) or shooting, gun should snap to aim at the crosshair target.
Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`).  may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 


------------------




### 1e. Empty click → sound → auto-reload on 3rd click

**Current behavior:** When firing with `gun.rounds == 0`, `fire_gun_observer` returns early at [weapon_shooting.rs:165-167](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L165-L167). No empty-click sound is played. The animation system in `drive_character_animation` at [animation.rs:330-335](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L330) checks `can_shoot` (which requires `rounds > 0`) and skips firing entirely when empty.

There is no existing "empty click" / "dry fire" sound in the audio manifest. We need to add one.

**Fix:**

1. **Add `EmptyClick` audio effect:**
   - `plugins/audio/audio_fx.rs`: Add `AudioFxEventType::EmptyClick` variant. Map it to a dry-fire sound file (e.g. `"weapons/guns/gun_dry_fire.mp3"` or `"weapons/guns/empty_click.mp3"`).

2. **Add `empty_click_count: u32` to `GunState`:**
   - [weapon_shooting.rs:17](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L17): add `pub empty_click_count: u32` field, default `0`.
   - Initialize to 0 in `equip_weapon_observer` ([weapon_attach.rs:87-92](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L87-L92)).

3. **In `drive_character_animation`:** When `fire_pressed && is_gun && !can_shoot && !is_reloading`:
   - Increment `gun.empty_click_count`.
   - Play the empty click sound via `AudioFxEvent::EmptyClick`.
   - If `empty_click_count >= 3`: trigger `ReloadGunEvent` and reset `empty_click_count = 0`.
   - Still respect the `WeaponCooldown` so clicks aren't instant spam.

4. **Reset counter on weapon switch or manual reload:**
   - In `equip_weapon_observer`: `empty_click_count` is implicitly reset because a fresh `GunState` is inserted.
   - In `reload_gun_observer` ([weapon_shooting.rs:307-327](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L307-L327)): add `gun.empty_click_count = 0`.
   - In the reload-pressed branch of `drive_character_animation` ([animation.rs:380-400](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L380-L400)): counter is reset via the `ReloadGunEvent` observer.

**Files:**
- `plugins/weapons/weapon_shooting.rs`: `GunState` gets `empty_click_count: u32`, reset in `reload_gun_observer`
- `plugins/weapons/weapon_attach.rs`: init `empty_click_count: 0` in `equip_weapon_observer`
- `plugins/pedestrians/pedestrian_controller_plugin/animation.rs`: empty-click branch in `drive_character_animation`
- `plugins/audio/audio_fx.rs`: `AudioFxEventType::EmptyClick` variant


| 1e | `plugins/weapons/weapon_shooting.rs`, `weapon_attach.rs`, `pedestrian_controller_plugin/animation.rs`, `audio/audio_fx.rs` |
5. **1e Empty click:** Firing a gun with 0 rounds should play a dry-fire click sound. After 3 clicks, auto-reload starts. Switching weapons or pressing R resets the counter.
Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`).  may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 


---

## 2. Refactoring: decouple entities from drivers

This section audits the existing entity/driver architecture and proposes a refactoring plan that separates the *simulated entities* (pedestrians, cars, weapons) from the *decision makers* (player input, NPC AI, network replay).

### Current architecture audit

#### Pedestrians — three drivers

| Driver | Input system | Movement | Animation | Weapon firing |
|--------|-------------|----------|-----------|--------------|
| **Player** | `character_input` ([controller.rs:13](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs#L13)) writes `LocomotionInput` + `MovementModifiers` | Shared `CharacterLocomotionPlugin` ([locomotion.rs:23](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/locomotion.rs#L23)) — `movement`, `apply_speed_cap`, `move_and_slide`, etc. | `drive_character_animation` ([animation.rs:65](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L65)) — direct `AnimationPlayer` manipulation | `FireGunEvent` / `PendingMeleeHit` from animation.rs |
| **NPC AI** | `ai_movement` ([movement_ai.rs:31](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/movement_ai.rs#L31)) writes `LocomotionInput` + `MovementModifiers` | Same `CharacterLocomotionPlugin` | `ai_animation` ([anim_ai.rs:17](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/anim_ai.rs#L17)) → `PedestrianAnimationControlEvent` → `play_animations_system` | `ai_combat` fires `FireGunEvent` / `PendingMeleeHit` |
| **Network** | `interpolate_remote_avatars` ([multiplayer_plugin.rs:841](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L841)) — direct transform lerp, no `LocomotionInput` | Kinematic `RigidBody`, **no** `CharacterLocomotionPlugin` physics | `update_remote_animations` ([multiplayer_plugin.rs:1063](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L1063)) → `TargetAnimation` → `play_animations_system` | `apply_remote_events` — replays `PlayerEventMsg::Shoot`/`Melee` |
| **Traffic ped** | `drive_traffic_pedestrians` ([pedestrian_traffic.rs:177](crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/pedestrian_traffic.rs#L177)) overrides `LocomotionInput` of an `AiPedestrian` when `AiState::Idle` | Same `CharacterLocomotionPlugin` (they are AI peds) | Same as NPC AI (`ai_animation`) | N/A (traffic peds don't fight) |

**What's already shared:**
- ✅ `LocomotionInput` / `MovementModifiers` are the canonical interface between drivers and the locomotion physics chain.
- ✅ `CharacterLocomotionPlugin` runs on *all* `CharacterController` entities regardless of driver type.
- ✅ `locomotion_clip()` ([animation.rs:20](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L20)) is already the single speed→clip helper, used by player, AI, and network (bugs4 refactor slice ✅ implemented).
- ✅ `NetworkDriven` marker exists ([animation.rs:17](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L17)), used on remote avatars ([multiplayer_plugin.rs:651](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L651)).
- ✅ `character_physics_bundle()` ([mod.rs:387](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L387)) is used by both player spawn and `eject_driver_as_ai`.

**What's duplicated / tightly coupled:**

1. **Animation driver:** Player uses direct `AnimationPlayer` manipulation (`drive_character_animation`), AI uses event-based (`PedestrianAnimationControlEvent` → `play_animations_system`), network uses `TargetAnimation` → `play_animations_system`. The *clip selection* is shared via `locomotion_clip()`, but the *animation pipeline* has 3 entry points.

2. **Weapon firing pipeline:** Player fires from `drive_character_animation` (animation.rs), AI fires from `ai_combat` (combat.rs). Both emit `FireGunEvent` / `PendingMeleeHit`, which is good — the event-driven observer is the shared entity behavior. The coupling is in *who decides to fire*, which is correctly in the driver.

3. **Spawn bundles:** Player spawns via `spawn_controlled_pedestrian_observer` with `character_physics_bundle` + `AnimState` + `CombatState` + `ManualAnimation`. AI spawns via `spawn_ai_pedestrian_observer` with `character_physics_bundle` + AI components. Network spawns inline in multiplayer_plugin with a bespoke component bag. The player spawn and ejected-driver spawn already share `character_physics_bundle`, but network spawn does NOT use it (uses kinematic rigid body + capsule manually).

4. **Car driving:** Player drives via `keybinds_control_car` → `Drive` event → `car_drive_observer`. Traffic drives via `drive_traffic_cars` → `Drive` event → `car_drive_observer`. Network car is pure kinematic replay. The `Drive` observer pattern is already a clean decoupling point.

#### Cars — three drivers

| Driver | Steering input | Physics | Camera |
|--------|---------------|---------|--------|
| **Player** | `keybinds_control_car` → `Drive` event | `car_drive_observer` + `apply_car_steering_and_drive` | `camera_follows_car` |
| **Traffic** | `drive_traffic_cars` → `Drive` event | Same observers | N/A |
| **Network** | Transform lerp (`interpolate_remote_avatars`) | Kinematic body, no `Drive` | N/A |

**Already well-decoupled:** The `Drive` event is the shared interface. Traffic cars and player cars use the same physics pipeline. Network cars bypass physics entirely (kinematic replay), which is correct.

### Refactoring plan

The existing architecture is **mostly well-factored**. The main improvements to make:

#### 2a. Unify network remote avatar spawn with `character_physics_bundle`

Currently network spawns an ad-hoc component bag at [multiplayer_plugin.rs:645-669](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L645-L669) with manually specified `RigidBody::Kinematic`, `Collider::capsule(...)`, `CollisionLayers`, etc. This duplicates `character_physics_bundle` and will drift if physics constants change.

**Fix:** Use `character_physics_bundle` for remote on-foot avatar spawn. Override `RigidBody` to `Kinematic` and strip `CustomPositionIntegration` since remote avatars don't use move-and-slide. Or: since `CharacterController` requires `RigidBody::Kinematic` and `CustomPositionIntegration`, and remote avatars do NOT want the shared locomotion physics chain to move them, we should NOT use `character_physics_bundle` directly — instead, extract the collision shape + layers into a shared helper:

```rust
pub fn character_collision_bundle(scale: f32) -> impl Bundle {
    use crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer;
    (
        Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
        CollisionLayers::new(
            GamePhysicsLayer::Car,
            [GamePhysicsLayer::Map, GamePhysicsLayer::Car, GamePhysicsLayer::Wheel],
        ),
        CollisionEventsEnabled,
    )
}
```

Network spawn uses this instead of hardcoding the capsule dimensions and layers.

**Files:**
- `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`: extract `character_collision_bundle`
- `plugins/network/multiplayer_plugin.rs`: use `character_collision_bundle` in on-foot avatar spawn

#### 2b. Formalize driver markers

Three marker components already exist or should exist to tag what's driving a pedestrian:
- Player: **implicit** — `ControlledCharacter` resource points to the entity. No marker component.
- AI: `AiPedestrian` marker ([mod.rs:36](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/mod.rs#L36)).
- Network: `NetworkDriven` marker ([animation.rs:17](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L17)).
- Traffic: `TrafficPedestrian` marker ([mod.rs:62](crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/mod.rs#L62)).

These are already mutually exclusive. Add a `PlayerDriven` marker component on the controlled character entity for symmetry. This lets systems filter by driver type without reading the `ControlledCharacter` resource.

**Files:**
- `plugins/pedestrians/pedestrian_controller_plugin/spawn.rs`: add `PlayerDriven` marker, insert on spawn
- `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`: export `PlayerDriven`

#### 2c. Document the driver/entity boundary

The decoupling boundary is:
- **Entity interface (shared):** `LocomotionInput`, `MovementModifiers`, `Drive` event (cars), `FireGunEvent`, `ReloadGunEvent`, `PendingMeleeHit`, `EquipWeaponEvent`, `PedestrianAnimationControlEvent` / `TargetAnimation`.
- **Driver responsibility:** Write to the entity interface above. Each driver reads its own input source (keyboard, AI brain, network state) and translates it to these shared commands.
- **Entity behavior (shared):** `CharacterLocomotionPlugin` (movement), `play_animations_system` (animation), `fire_gun_observer` / `tick_pending_melee_hits` (weapon effects), `car_drive_observer` / `apply_car_steering_and_drive` (car physics).

Add a module-level doc comment to `plugins/pedestrians/pedestrian_controller_plugin/mod.rs` documenting this architecture.

#### 2d. Consolidate player animation driver to event-driven (DEFERRED)

The player's `drive_character_animation` directly manipulates `AnimationPlayer` (multi-clip blending for combat overlay on top of locomotion), while AI and network use `play_animations_system` (single-clip switching). Unifying these would require `play_animations_system` to support combat overlay blending, which is a significant change. **Defer this** — the current split is functional and the combat overlay system is player-only (AI combat uses one-shot events).

#### 2e. Consolidate traffic pedestrian as an AI overlay (DEFERRED)

Traffic peds are already AI peds with a `TrafficPedestrian` overlay that overrides `LocomotionInput` when `AiState::Idle`. This is a clean overlay pattern and doesn't need refactoring.

### Summary: what to implement now vs. defer

**Implement now:**
- 2a: Extract `character_collision_bundle`, use in network spawn
- 2b: Add `PlayerDriven` marker
- 2c: Document the architecture

**Defer:**
- 2d: Unify animation driver (requires combat overlay support in shared system)
- 2e: Traffic ped consolidation (already clean)



| 2  | `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`, `plugins/network/multiplayer_plugin.rs`, `spawn.rs` |
6. **2 Refactor:** All existing behavior unchanged. Network remote avatars use shared collision constants. `PlayerDriven` marker on player pedestrian.
Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`).  may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 





-------------------





## 3. Car controller: camera zoom and drive-by shooting

### Current in-car state

When the player enters a car ([interaction_ui.rs:268-363](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L268-L363)):
1. The pedestrian controller entity is **despawned**.
2. The pedestrian model is re-parented into the car as a `DriverMesh`.
3. `ActivePlayerVehicle` marker is inserted on the car entity.
4. `GameControlState` transitions to `DrivingCar`.
5. The camera switches to `camera_follows_car` ([camera_follow.rs:6-97](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L6-L97)).

Currently in `DrivingCar` state:
- `weapon_wheel`, `weapon_hud_ui`, `crosshair_ui` all only run in `ControllingPedestrian` state ([mod.rs:349-373](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L349-L373)).
- No weapon interaction exists while driving.

### Implementation plan

#### 3a. Weapon persistence across car entry

When entering a car, preserve the player's current weapon selection.

- Add a `driver_weapon: Option<WeaponId>` field to `DriverMesh`. When the controller is despawned during `tick_entering_car`, read its `EquippedWeapon` and store it.
- If the player had no weapon (Unarmed), pick a random gun from `WeaponManifest` for the drive-by.
- When exiting the car, the `SpawnControlledPedestrianEvent` should carry the weapon so the new pedestrian equips it.

**Files:**
- `plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs`: `DriverMesh` gets `weapon: WeaponId`, populated in `tick_entering_car`, used in `tick_driver_mesh_exit`

#### 3b. In-car weapon switching and HUD

- **Mouse wheel weapon switching while driving:** Add a system similar to `weapon_wheel` that runs in `DrivingCar` state, cycling `WeaponSelection` through guns only (no melee/unarmed in car). Store the selection on a resource or the `DriverMesh`.
- **Weapon HUD:** Create a `driver_weapon_hud_ui` system that runs in `EguiPrimaryContextPass` during `DrivingCar` state, drawing the same top-right weapon name + ammo display as `weapon_hud_ui`. Read ammo from a `DriverGunState` resource.

**New resource:** `DriverGunState` — mirrors `GunState` but lives as a resource (since the controller entity is despawned). Or: add `GunState` fields to the `DriverMesh` component or a companion component on the car entity.

**Files:**
- `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`: register new systems for `DrivingCar`
- `plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs`: `driver_weapon_wheel`, `driver_weapon_hud_ui`

#### 3c. Blind shooting (LMB while driving)

When the player clicks LMB while driving (without holding RMB):
1. Show only the center dot (no full crosshair ring).
2. Fire the gun using the standard `FireGunEvent`.
3. No special animation on the driver mesh.

**Shooting origin problem:** The gun muzzle is parented to the driver mesh inside the car. A ray from the muzzle may hit the car's own collider.

**Solution (from spec):**
1. Cast a camera ray to find the target point (same as current `fire_gun_observer`).
2. Cast a ray from the gun muzzle toward the target point, but shorter than the actual distance.
3. If it hits the player's own car: cast a second ray from outside the car toward the gun muzzle to find the car hull exit point. Use that exit point as the effective shooting origin.

**Implementation:** Modify `fire_gun_observer` to accept an optional `car_entity: Option<Entity>` in `FireGunEvent`. When present, after computing `origin` and `dir`:
- Do an additional short ray from `muzzle` toward `impact` with the *car excluded* but also a separate check: cast from `muzzle` toward the target at `distance - 0.5`. If this ray hits the player's car, cast a reverse ray from `impact` direction back toward `muzzle` with only the car in the mask, find the exit point, use that as the new `muzzle`.

**Simpler approach:** Just exclude the player's car entity from the `SpatialQueryFilter` in `fire_gun_observer`. The existing filter excludes `shooter` but the car is a different entity. Add the car entity to the exclusion list. This avoids the complex double-ray. But to get the tracer to start from outside the car, we need the exit-point approach.

**Files:**
- `plugins/weapons/weapon_shooting.rs`: `FireGunEvent` gets `car_entity: Option<Entity>`; `fire_gun_observer` handles car hull penetration
- `plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs`: LMB handler in `DrivingCar` state
- `plugins/cars_driving/driving_plugin/camera_follow.rs`: no change (camera ray origin is already from camera)

#### 3d. Aim mode while driving (RMB)

When RMB is held while driving:
1. Show the full aim reticle (crosshair ring + dot).
2. LMB while aiming fires accurately at the crosshair target.
3. Camera may zoom slightly (optional, can share `CameraRig.aiming` concept).

**Implementation:**
- Add an `aim_mode: bool` field to a new `DriverAimState` resource, set when RMB is held in `DrivingCar`.
- The crosshair system already checks `GunState` — need to make it also run in `DrivingCar` when aiming.
- Modify `camera_follows_car` to optionally zoom when aiming (reduce the `r = 16.0` to `r = 8.0`).

**Files:**
- `plugins/pedestrians/pedestrian_controller_plugin/mod.rs`: register aim/shoot systems for `DrivingCar`
- `plugins/cars_driving/driving_plugin/camera_follow.rs`: aim zoom support
- New system: `driver_combat_input` in `interaction_ui.rs`


| 3  | `plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs`, `mod.rs`, `plugins/weapons/weapon_shooting.rs`, `plugins/cars_driving/driving_plugin/camera_follow.rs` |
7. **3 Drive-by:** While driving: mouse wheel cycles guns, HUD shows weapon/ammo. LMB blind-shoots, RMB aims with reticle, LMB+RMB shoots aimed. Shots originate from outside the car if the muzzle is inside.
Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`).  may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 


----------------


