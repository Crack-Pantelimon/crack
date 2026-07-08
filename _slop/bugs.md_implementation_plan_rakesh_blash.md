# Implementation Plan - Resolving bugs.md (7 Items)

This plan outlines the changes required to resolve 7 bugs in the Bevy simulation app and associated crates.

---

## Proposed Changes

### 1. Mouse Capture & Camera Control (Item 1)
- **Goal**:
  - Automatically capture the mouse (hide it, lock grab mode, and recenter it to center of screen every frame to prevent runoff) when in `ControllingPedestrian` or `DrivingCar` mode.
  - While captured, mouse motion directly rotates the camera without click-dragging.
  - Pressing `Escape` (if not focused on an egui input widget) releases/uncaptures the mouse (makes cursor visible and unlocked) instead of immediately exiting the gameplay mode.
  - Pressing `Escape` when *already* released/uncaptured executes the normal state transition back to `MapFreecam`.
  - Clicking on the game area (outside of egui UI elements) recaptures the mouse.
  - In other states (`MapFreecam` or any debug scene modes), the mouse is not captured.
- **Files to Modify**:
  - [NEW RESOURCE & SYSTEM] inside `src/plugins/states/mod.rs` (or a dedicated module/system) to manage the capture state.
  - [MODIFY] [camera.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs): `orbit_camera_input` to rotate camera based on mouse motion without clicking if captured.
  - [MODIFY] [camera_follow.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs): `camera_follows_car` to check capture state and apply rotation without drag.
  - [MODIFY] [spawn.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs): `escape_to_freecam` to first release capture if active, otherwise exit.
  - [MODIFY] [keybinds_control.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/keybinds_control.rs): `keybinds_control_car` to first release capture if active, otherwise exit.

### 2. 3D Audio Left and Right Swap (Item 2)
- **Goal**:
  - Swap the left and right ear offsets for the 3D spatial listener.
- **Files to Modify**:
  - [MODIFY] [mod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/mod.rs): In `setup_spatial_listener`, `add_spatial_listener_to_new_cameras`, and `update_listener_ears`, swap the positive and negative X offsets for the left and right ears.

### 3. UI Input Gating & Keyboard/Mouse Blocking (Item 3)
- **Goal**:
  - Prevent keyboard/mouse input from bleeding into gameplay actions (WASD locomotion, throttle, handbrake, reload, interaction F key, freecam movement) when interacting with egui widgets.
- **Files to Modify**:
  - [MODIFY] [controller.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs): Gate `character_input` and `jump_or_climb` behind `!wants_keyboard_input()`.
  - [MODIFY] [interaction_ui.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs): Gate `detect_car_interaction` and `handle_exit_car` behind `!wants_keyboard_input()`.
  - [MODIFY] [animation.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs): Gate the reload key check behind `!wants_keyboard_input()`.
  - [MODIFY] [keybinds_control.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/keybinds_control.rs): Gate all keyboard driving checks behind `!wants_keyboard_input()`.
  - [MODIFY] [camera_controls.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/game_freecam/camera_controls.rs): Gate freecam keyboard movement checks behind `!wants_keyboard_input()`.

### 4. Player Death Prop and Animation (Item 4)
- **Goal**:
  - When the player dies, spawn a non-looping "death prop" pedestrian at the exact same location/rotation, showing the "Death01" animation, and hold it on the final frame. Keep this prop for 10 seconds, then despawn it.
- **Files to Modify**:
  - [MODIFY] [spawn.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs):
    - Redefine `player_death_to_freecam` to retrieve the player's current `PedestrianUrl` and `Transform`, spawn a `DeathProp` entity with a 10s timer, trigger `SpawnPedestrianEvent` parented/controlled by it, and then despawn the controller.
    - Implement `tick_death_props` to count down the 10s timer and despawn.
    - Implement `setup_death_prop_animations` to insert the `TargetAnimation` ("Death01") and `PlayOnceAnimation` on the model entity.
  - [MODIFY] [animation.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs):
    - Declare `PlayOnceAnimation` component.
    - Modify `play_animations_system` to skip calling `.repeat()` if `PlayOnceAnimation` is present.
  - [MODIFY] [mod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/mod.rs): Export `PlayOnceAnimation` and register the new systems in `PedestriansPlugin`.
  - [MODIFY] [mod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs): Register `tick_death_props` and `setup_death_prop_animations` systems.

### 5. Melee Attack Hit Detection & Online Sync (Item 5)
- **Goal**:
  - Replace the single-ray melee hit code with a spatial intersection test using a 1x1x2m cube in front of hips.
  - Flash a yellow wireframe gizmo representing this cube for 0.1s.
  - Enable remote clients to execute the same hit test (with victim-authoritative damage) when receiving `PlayerEventMsg::Melee` containing the strike position and rotation.
- **Files to Modify**:
  - [MODIFY] [multiplayer_plugin.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs):
    - Update `PlayerEventMsg::Melee` to include `position: [f32; 3]` and `rotation: [f32; 4]`.
    - Update `collect_outbound_events` to populate and send this.
    - Handle `PlayerEventMsg::Melee` by flashing the yellow gizmo locally and executing the spatial query to damage the local player/car if hit.
  - [MODIFY] [weapon_shooting.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs):
    - Implement a shared hit registration function `perform_melee_strike` using `spatial.shape_intersections(&Collider::cuboid(1.0, 2.0, 1.0), ...)`.
    - Filter for targets within 2.0 meters of the cube center.
    - Replace the old raycast in `tick_pending_melee_hits` with a call to `perform_melee_strike`.
    - Provide a system to draw yellow gizmo boxes stored in a resource list for 0.1s.

### 6. Traffic AI Melee Distance Correction (Item 6)
- **Goal**:
  - Stop the AI from walking/sprinting on top of targets during melee combat.
  - When target is within a comfortable distance (<= 1.2m), stop locomotion and rotate directly toward target center.
  - Also, change `ai_combat` to queue `PendingMeleeHit` instead of applying direct damage, so the same shape intersection test resolves the hits.
- **Files to Modify**:
  - [MODIFY] [movement_ai.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/movement_ai.rs): Update `ai_movement`'s melee hunt branch to stop locomotion and align rotation if target distance <= 1.2m.
  - [MODIFY] [combat.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/combat.rs): Update `ai_combat` to insert `PendingMeleeHit` instead of applying direct damage.

### 7. SQL API user secret key persistence (Item 7)
- **Goal**:
  - Persist `UserIdentitySecrets` across sessions/tabs in the SQL database.
  - Ensure different tabs generate/use unique NodeSecretKeys, but share the same UserSecretKey.
- **Files to Modify**:
  - [MODIFY] [mod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/mod.rs):
    - Defer network initialization until `CrackClient` is ready.
    - Load/create the `UserIdentitySecrets` from/into a dedicated `user_secrets` table via `ApiClient`.
    - Update `ChatEvent::Connected` and Bevy systems to pass the loaded nickname/color to Bevy once connected.

---

## Verification Plan

### Automated Verification
- We will build the application using `cargo check` inside `crack_demo/demo_resolution_selector_web_bevy/` to verify it compiles successfully.
- We will ensure all modifications compile cleanly without warnings or errors.

### Manual Verification
- Deploying the app in the browser or locally and testing the camera grab, escape release, 3D audio panning, UI keyboard focus, melee hit tests, and traffic AI spacing.
