# Car camera & drive-by aiming — plan

**Sub-problem:** While driving you can't hold the camera off-forward to shoot —
it auto-snaps back behind the car; the reticle is always on screen; and shooting
is always allowed. Desired: **drive one-handed** with the camera auto-centered
behind the car, and only when you **hold RMB (aim)** does the camera free-look,
the reticle appear, and LMB fire. Aim state = RMB held (mirrors the on-foot rig,
`rig.aiming = mouse.pressed(Right)`,
[camera.rs:119](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs#L119)).

---

## Current state

While in `GameControlState::DrivingCar` the mouse is **captured**
(`is_captured = true`,
[states/mod.rs:66-73](crack_demo/demo_resolution_selector_web_bevy/src/plugins/states/mod.rs#L66-L73)),
and three systems run:

1. **`camera_follows_car`**
   ([camera_follow.rs:7-103](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L7-L103)),
   registered `run_if(in_state(DrivingCar))`
   ([driving_plugin/mod.rs:102-107](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L102-L107)):
   - `drag_active = is_captured || (!egui && LMB)`
     ([camera_follow.rs:52-53](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L52-L53)) —
     because the mouse is captured, mouse motion **always** rotates the camera.
   - **Auto-center at `speed > 1.0`**
     ([camera_follow.rs:66-77](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L66-L77))
     eases yaw/pitch back to *behind-the-car* every frame → this is the "pops back
     forward" the user is fighting.
2. **`driveby_fire`**
   ([interaction_ui.rs:1128-1207](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L1128-L1207)):
   fires on LMB with **no aim gate**
   ([interaction_ui.rs:1171-1177](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L1171-L1177)).
3. **`driving_crosshair_ui`**
   ([interaction_ui.rs:1210-1233](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L1210-L1233)):
   draws the reticle whenever the driver `has_gun` — **always on**.

## Target design

Introduce one shared **aim signal** for car mode and branch all three systems on
it.

`aiming = is_captured && !egui_focused && mouse.pressed(MouseButton::Right)`

Recommend computing it once and stashing it in a tiny resource
`#[derive(Resource, Default)] struct DrivingAim { pub aiming: bool }`, updated by a
small system ordered before the three consumers (or at the top of
`camera_follows_car`, then read by the other two). Reusing the on-foot
`CameraRig.aiming` is possible but muddy (that rig belongs to the on-foot follow
camera); a dedicated resource is cleaner. Keep the egui guard so aiming can't
trigger while the pointer is over egui.

### 1. `camera_follows_car`
- Replace `drag_active` with `aiming`: mouse motion rotates the camera **only
  while aiming**
  ([camera_follow.rs:52-63](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L52-L63)).
  When not aiming, keep draining `mouse_motion.read()` (the existing `else` at
  [camera_follow.rs:61-63](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L61-L63))
  so deltas don't accumulate and jerk the view on the next aim.
- Gate the auto-center block on **`!aiming`** instead of only `speed > 1.0`
  ([camera_follow.rs:66-77](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L66-L77)).
  When not aiming, always ease yaw/pitch toward `default_yaw/default_pitch`
  (behind the car) so releasing RMB reliably returns the camera to forward, even
  at low speed. When aiming, skip recenter entirely so the camera holds where the
  player points. (Optionally keep a mild speed factor so it recenters faster at
  speed, but the key change is: no recenter while aiming.)
- Net effect: not aiming → camera locked behind car, one-handed driving; aiming →
  full free-look with the captured mouse.

### 2. `driveby_fire`
- Add the aim gate to `fire_pressed`: require `aiming` (RMB held) in addition to
  LMB
  ([interaction_ui.rs:1171-1177](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L1171-L1177)).
  So `fire = aiming && (LMB just_pressed || (automatic && LMB pressed))`. LMB does
  nothing to the gun unless RMB is also held. (Leave R-to-reload ungated so you
  can reload while cruising.)

### 3. `driving_crosshair_ui`
- Only paint the reticle when `aiming`
  ([interaction_ui.rs:1218-1232](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L1218-L1232)).
  Add the `DrivingAim` resource (or `mouse` + egui-guard) as a param and early-out
  when not aiming, keeping the existing `has_gun` check.

## Gotchas

- **Mouse is captured while driving**, so RMB/LMB are readable and the "drag"
  concept collapses to "aiming or not". Don't also require an un-captured LMB
  drag — that path (`!egui && LMB`) is dead weight now; the `aiming` signal covers
  everything.
- **Weapon barrel orientation.** The driver's gun aims via the arm-IK / weapon
  systems toward the camera crosshair. Once the camera stops auto-centering while
  aiming, the barrel will track wherever the player points — confirm the weapon
  aim path (`update_weapon_transforms`, and the driver arm-IK at
  [arm_ik.rs:333](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs#L333))
  reads the live camera forward, not a car-relative direction. If it reads the
  camera, no change needed.
- **Recenter easing constant.** `reset_speed = 2.0`
  ([camera_follow.rs:72](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L72))
  now governs the return-to-forward feel on RMB release; tune if the snap-back is
  too abrupt or too sluggish.

## Test

`/run`, spawn/enter an armed car:
- **Not aiming:** drive with WASD; moving the mouse does nothing to the camera; it
  stays locked behind the car; no reticle; LMB does not fire.
- **Hold RMB:** reticle appears; mouse freely orbits the camera around the car and
  it does **not** snap back; LMB fires along the crosshair.
- **Release RMB:** reticle vanishes, LMB stops firing, camera eases back to
  behind-the-car (both while moving and while stopped).
