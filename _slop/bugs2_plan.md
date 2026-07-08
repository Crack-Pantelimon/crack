# Plan: camera collision, chat bubbles, self HUD, car park mode, volume slider

## Context

Five gameplay-polish fixes for the Bevy game in `crack_demo/demo_resolution_selector_web_bevy/`:

1. **Camera clips through world** — both follow cameras (on-foot + driving) set their position with no collision test, so the camera phases through ground/buildings (can't look up because ground renders in front, can't fit in tight spaces).
2. **No chat bubbles** — global-chat messages only appear in the chat window; nothing above the speaker in-world.
3. **No self label** — remote players get a floating name + HP bar, but the local player only has a top-left text HUD.
4. **Car never settles** — the velocity-space controller re-injects tiny velocities every frame, so a stopped car jitters at ~1 km/h instead of parking.
5. **No master volume** — there is no global volume control in the game's Options menu (only in a standalone audio demo).

---

## 1. Camera collision (raycast, place at 90% of first hit)

Reuse the existing `avian3d` ray API:
`spatial_query.cast_ray(origin, Dir3, max_distance, solid, &SpatialQueryFilter) -> Option<RayHitData>` (`.distance`), as used in [driving_plugin/mod.rs:917](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L917) and [traffic/spawn.rs:35](crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/spawn.rs#L35).

Filter: `SpatialQueryFilter::from_mask([GamePhysicsLayer::Map]).with_excluded_entities([player_or_car_entity])` — hits only static world geometry (map colliders are members of `GamePhysicsLayer::Map`, see [map_lod.rs:43](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs#L43)), ignores the followed entity. `GamePhysicsLayer` is defined in [driving_plugin/mod.rs:100](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L100).

**On-foot camera** — [pedestrian_controller_plugin/camera.rs:54-104](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs#L54):
- Add `spatial_query: avian3d::prelude::SpatialQuery` param to `follow_camera`.
- Replace the unconditional `cam.translation = anchor + offset` (line 102) with: cast from `anchor` toward `offset` for `max_distance = CAM_DISTANCE`, excluding `controller_ent`. If hit, `let dist = (hit.distance * 0.9).min(CAM_DISTANCE); cam.translation = anchor + offset.normalize() * dist;` else `anchor + offset`. Guard `Dir3::new(offset)` with `.ok()` / fall back to no-collision when `offset` is near-zero.

**Driving camera** — [camera_follow.rs:6-84](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs#L6):
- Add `spatial_query: avian3d::prelude::SpatialQuery` param to `camera_follows_car`.
- At line 82 (`camera_transform.translation = center + offset`), apply the same raycast from `center` toward `offset` (`max_distance = r = 16.0`), excluding the car entity. To exclude the car, add `Entity` to the `car_query` tuple. Place at `center + offset.normalize() * (hit.distance * 0.9).min(r)`.

---

## 2. Global-chat bubbles above the speaker

**Plumb the speaker identity.** `ChatEvent::Message` ([network/mod.rs:44](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/mod.rs#L44)) currently carries only nickname/text/color — no way to map to an entity. Add `node_id: PublicKey` to the variant and fill it from `msg.from.node_id()` at the chat receive site ([network/mod.rs:473-479](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/mod.rs#L473)). `RemotePlayers.0` is keyed by `PublicKey` ([multiplayer_plugin.rs:168](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L168)), so this gives a direct lookup to the avatar root entity (`RemoteAvatar::OnFoot { root, .. }` / `InCar { root }`).

**Bubble store.** Add a resource in `network/mod.rs`, init in the network plugin:
```rust
#[derive(Resource, Default)]
pub struct ChatBubbles {
    pub by_node: HashMap<PublicKey, (String, f64)>, // node_id -> (text, expiry_secs)
    pub own: Option<(String, f64)>,
}
```
- In `drain_chat_events` ([network/mod.rs:529](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/mod.rs#L529)) on `ChatEvent::Message`: truncate `text` to 70 chars (`text.chars().take(70).collect()` + `"…"` if longer), insert `by_node[node_id] = (text, now + 3.0)` using `time.elapsed_secs_f64()` (add `Res<Time>` to the system).
- For the local player's own message: set `bubbles.own = (text, now + 3.0)` at the send site in [network/global_chat_ui.rs](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/global_chat_ui.rs) where `input_buffer` is pushed to `outgoing_tx` (own gossip echoes are not guaranteed, so set it explicitly on send).

**Render.** Reuse the `Camera::world_to_viewport` + `egui::Area::pivot(CENTER_BOTTOM)` pattern already in [`draw_remote_billboards` (multiplayer_plugin.rs:1674)](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L1674):
- Add `Res<ChatBubbles>` + `Res<Time>` to `draw_remote_billboards`; after the name/HP `vertical` block, if `bubbles.by_node.get(&marker.node_id)` exists and not expired, draw the text (with a dark rounded background rect) one line above the name.
- Expiry is checked at draw time (compare to `time.elapsed_secs_f64()`); no separate GC system needed, but drop stale entries opportunistically to bound the map.

---

## 3. Own HP bar + name above the local player

New system `draw_self_billboard` (same file/module as `draw_remote_billboards`, registered in the same `EguiPrimaryContextPass` set at [multiplayer_plugin.rs:271](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L271)):
- Params: `EguiContexts`, `Query<(&Camera,&GlobalTransform),With<Camera3d>>`, `Res<ControlledCharacter>` ([spawn.rs:14](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs#L14)), `Query<(&GlobalTransform,&Health,&CharacterScale)>`, `Res<ChatState>` (for `own_nickname`/`own_color`), `Res<ChatBubbles>`, `Res<Time>`.
- Resolve `controlled.controller`; read its `GlobalTransform`, `Health` ([faction.rs:90](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/faction.rs#L90)) and `CharacterScale`. Draw name + HP bar (`health.current / health.max`, same green/yellow/red thresholds and 50×4 bar as the remote billboards) at `translation + Vec3::Y * 1.8 * scale`, plus `bubbles.own` if unexpired.
- Skip when not in on-foot control (e.g. only when `ControlledCharacter.controller` is set); reuse the same `world_to_viewport` guard so it hides when off-screen/behind camera.

---

## 4. Car "park" mode (settle + sleep)

Cars are `RigidBody::Dynamic` with `SleepingDisabled` inserted at spawn ([spawn_car.rs:122](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs#L122)). The velocity controller `apply_car_steering_and_drive` overwrites `LinearVelocity`/`AngularVelocity` every frame ([driving_plugin/mod.rs:382-658](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L382), planar write at [mod.rs:655](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L655)), which is why the body never reaches zero. avian3d auto-wakes any body whose velocity/transform is written or that is hit — so parking must **stop the controller from writing** to a parked car.

- Add `parked: bool` and a `park_timer: f32` to `CarDriveState` ([mod.rs:154](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L154)).
- Inside `apply_car_steering_and_drive`, per car, before the velocity writes: compute `speed_xz` and check driver input via the existing `avg_accelerate/avg_brake/avg_steer` (already zeroed after the 0.06s input window, [mod.rs:421](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L421)).
  - **Enter park:** if no input AND `speed_xz < ~0.5 m/s` AND angular speed small, accumulate `park_timer += dt`; once `> ~0.4s`, set `parked = true`, set `lin_vel.0 = ZERO`, `ang_vel.0 = ZERO`, remove `SleepingDisabled`, and `commands.queue(avian3d SleepBody(entity))`.
  - **While parked:** `continue` (skip all steering/velocity writes) so the body stays asleep.
  - **Exit park:** if any nonzero input arrives (or a `Drive` event), set `parked = false`, reset `park_timer`, re-insert `SleepingDisabled`, `commands.queue(WakeBody(entity))`, and resume normal control. (Collisions from other awake bodies auto-wake via avian; on the next frame the controller sees the body moving and can clear `parked` if velocity exceeds threshold.)
- Add `mut commands: Commands` to the system for the `SleepingDisabled` insert/remove + `SleepBody`/`WakeBody` commands. No "support prop" is needed — `SleepBody` + zeroed velocity is sufficient.
- Applies to traffic cars too (they share `CarDriveState`), which is desirable (idle traffic settles instead of jittering).

---

## 5. Master volume slider (Options > Sound, default 60%)

Use Bevy's `GlobalVolume` resource as the single master multiplier — it scales all `AudioPlayer` output (one-shot emitters from [`play_sound_observer` (audio/mod.rs:198)](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/mod.rs#L198) and the looping engine/footstep sinks alike), so no edits to the individual playback sites are needed.

- Add `master_volume: f32` to `UiState` ([ui_egui.rs:21](crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs#L21)); default `0.6` in both `UiState::default()` ([ui_egui.rs:39](crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs#L39)) and `with_physics_debug()` ([ui_egui.rs:61](crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs#L61)).
- In `GameAudioPlugin::build` ([audio/mod.rs:79](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/mod.rs#L79)), insert `GlobalVolume::new(Volume::Linear(0.6))`.
- Add a **"Sound"** entry to the Options menu next to "Graphics" ([ui_egui.rs:237](crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs#L237)) with a `show_sound_settings` flag, and a small panel (mirror the existing Graphics `SidePanel`/slider at [ui_egui.rs:168](crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs#L168)) containing `egui::Slider::new(&mut ui_state.master_volume, 0.0..=1.0).text("Volume").suffix("%")` (or 0..=100 int scaled to 0..1).
- Sync: when `ui_state.master_volume` changes, write `GlobalVolume` (either in the egui system if it can access `ResMut<GlobalVolume>`, or a tiny dedicated system that copies `UiState.master_volume` into `GlobalVolume`).

---

## Verification

Build must pass for the wasm/native game crate:
- `cargo check -p demo_resolution_selector_web_bevy` (and clippy, per repo convention).

Then run the game (`/run` or the project's launch skill) and confirm end-to-end:
1. **Camera:** on foot, back the character into a wall / look up under an overhang — camera pulls in instead of clipping through ground/buildings; repeat while driving.
2. **Chat bubbles:** send a global-chat message (and have/simulate a remote peer send one) — a bubble with name + first 70 chars appears above the right character for ~3s.
3. **Self label:** own name + HP bar float above the local character; HP bar drops when taking damage.
4. **Car park:** stop the car and release input — it settles to a dead stop within ~0.5s (no 1 km/h jitter); pressing accelerate/brake immediately drives again.
5. **Volume:** Options ▸ Sound slider defaults to 60% and scales all SFX; 0% mutes, 100% full.

## Notes / risks
- `ChatEvent` gaining `node_id: PublicKey` requires importing the `PublicKey` type into `network/mod.rs` (already used in `multiplayer_plugin.rs`).
- Confirm the exact avian3d sleep command names/paths (`SleepBody`/`WakeBody` as `Command`s, `SleepingDisabled`) against the pinned avian version before use.
- Bubbles/self-billboard are egui overlays drawn every frame — keep the per-frame map lookups cheap and drop expired bubble entries to bound memory.
