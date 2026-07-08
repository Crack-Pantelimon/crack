# Multiplayer v1 — Plan 2 (fixes & follow-ups)

Review of the v1 implementation (`_slop/multiplayer/PLAN.md`) as landed in
`crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs` +
`network/mod.rs`. The code compiles clean (`cargo check` passes). The separate
`global_gameplay` room with its own `GameplayChatMessageContent` is correctly implemented —
global chat (`GlobalChatMessageContent`) was left untouched, as required.

This plan covers: (A) the user-requested changes, (B) bugs found in review, (C) gaps vs the
original plan, with a file-by-file implementation order at the end.

---

## A. Requested changes

### A1. Traffic disabled on load

`TrafficConfig::default()` in `src/plugins/traffic/mod.rs` was changed for `ped_enabled:
false` but **`enabled: true` was left as-is**, so ambient traffic cars still spawn on load.

Fix: `enabled: false` in `impl Default for TrafficConfig`. The existing "Enabled" checkbox in
`traffic_debug_ui` becomes the opt-in (same pattern as peds). Also update the stale
`// default true` doc comments on both fields.

### A2. New `TooltipNotificationPlugin` — generic corner notifications

Today the bottom-left "tooltip" notifications are ad-hoc:

- `TooltipNotificationState` (in `src/plugins/geojson.rs:69`) holds **two hardcoded timers**
  (`map_loaded_timer`, `geojson_loaded_timer`);
- timers are set from `map_lod.rs:618` (`check_map_loaded_status`) and
  `geojson.rs` (`update_geojson_loading_finished`);
- timers are decremented in `geojson.rs` (`update_tooltip_timers`);
- rendering is inlined in `ui_egui.rs:315-358`.

Replace all of it with a dedicated plugin, new file
`src/plugins/notifications.rs`:

```rust
/// Fire via commands.trigger(...) from anywhere. The plugin owns display.
#[derive(Event, Clone, Debug)]
pub enum NotificationEvent {
    MapLoaded,
    GeoJsonLoaded,
    NetworkConnected,                                   // "network connected"
    GameNetworkOk,                                      // "game network ok"
    PlayerJoinedGame { nickname: String, color: (u8, u8, u8) },
    PlayerLeftGame { nickname: String, color: (u8, u8, u8) },  // cheap to add, symmetric
}

struct ActiveNotification {
    text: String,
    stroke: egui::Color32,   // border color, keeps the existing per-kind styling
    remaining: f32,          // seconds; default 3.0 like today
}

#[derive(Resource, Default)]
pub struct ActiveNotifications(Vec<ActiveNotification>);

pub struct TooltipNotificationPlugin;
// - add_observer(on_notification): maps NotificationEvent -> text+color, pushes entry
//   (cap the vec at ~6, drop oldest, so a join-flood can't fill the screen)
// - Update: tick_notifications (decrement remaining, retain > 0.0)
// - EguiPrimaryContextPass: render_notifications — the exact Frame/Area styling lifted
//   from ui_egui.rs:315-358 (bottom-left, egui::Order::Tooltip), stacking all active
//   entries vertically
```

Mapping for the existing two: `MapLoaded` → "map loaded." with the blue stroke
`(0,180,240)`; `GeoJsonLoaded` → "geojson loaded." with the green stroke `(0,220,80)`.
New ones: `NetworkConnected` → "network connected", `GameNetworkOk` → "game network ok",
`PlayerJoinedGame` → "player {nickname} has joined game" (stroke from the player's chat
color).

Call-site replacements (delete `TooltipNotificationState` entirely):

| File | Change |
|---|---|
| `geojson.rs` | remove `TooltipNotificationState` + `update_tooltip_timers`; `update_geojson_loading_finished` takes `Commands`, triggers `NotificationEvent::GeoJsonLoaded` |
| `map_plugin/map_lod.rs` (`check_map_loaded_status`, ~line 601/617) | drop the `tooltip_state` param; trigger `NotificationEvent::MapLoaded` via `Commands` (already has `commands`? if not, add it) |
| `ui_egui.rs` | delete the tooltip overlay block (lines ~314-358) and the `tooltip_state` system param |
| `main_game_plugin.rs` | `add_plugins(crate::plugins::notifications::TooltipNotificationPlugin)` |
| `plugins/mod.rs` | `pub mod notifications;` |

### A3. "network connected" on global chat join

`drain_chat_events` in `network/mod.rs` already handles `ChatEvent::Connected` (sets
`NetworkConnectionState::Connected`). Add `Commands` to the system and trigger
`NotificationEvent::NetworkConnected` there. No net-task change needed — `Connected` is
already sent after global chat `wait_joined()`.

### A4. "game network ok" on gameplay room join

The gameplay room join result currently never reaches Bevy (`gameplay_controller` is only
logged on error). Changes in `network/mod.rs`:

1. Add variant `ChatEvent::GameplayConnected`.
2. In `chat_main_task`, after `join_chat::<GameplaySyncRoomType>` succeeds, spawn a small
   task: `gameplay_c.wait_joined().await` → `incoming_tx.try_send(ChatEvent::GameplayConnected)`
   → `proxy.send_event(WakeUp)`. (Mirror what global chat does: joined = actually joined,
   not merely "join call returned".)
3. `drain_chat_events`: on `GameplayConnected` trigger `NotificationEvent::GameNetworkOk`,
   and set `MultiplayerStats.connected = true` (see B4).

### A5. "player X has joined game"

Message-based detection is sufficient (peers broadcast at 20 Hz, so first contact is
instant): in `receive_game_sync` (multiplayer_plugin.rs), when the
`remote_players.0.entry(...)` inserts a **new** entry, trigger
`NotificationEvent::PlayerJoinedGame { nickname, color }`. Use
`match remote_players.0.entry(...) { Entry::Vacant(v) => { commands.trigger(...); v.insert(...) } ... }`
— the system needs `Commands` added.

Symmetric: in `reconcile_remote_avatars`, when a peer is removed by the 5 s timeout, trigger
`PlayerLeftGame`.

---

## B. Bugs found in review

### B1. CRITICAL — remote one-shot events re-fire every frame

`apply_remote_events` reads `player.latest.events` each `Update` tick, but nothing ever
consumes them. A received `Shoot` therefore replays **every frame** until the next update
overwrites `latest` (~3× at 60 fps / 20 Hz send — more if the sender stalls, and the sender
only sends when its state snapshot succeeds). Consequences: triple damage per shot, triple
audio triggers, tracer spam. `prev` also retains a stale copy of the events (harmless but
sloppy).

Fix: move events out of the stored snapshot at receive time.

```rust
// RemotePlayer gains:
pub pending_events: Vec<PlayerEventMsg>,

// receive_game_sync: after decoding `update`:
player.pending_events.extend(update.events.drain(..));   // then store update in latest

// apply_remote_events: iterate std::mem::take(&mut player.pending_events)
// (needs ResMut<RemotePlayers> instead of Res)
```

Note: events from peers whose avatar entity hasn't spawned yet (or `Camera` peers, which have
no entity) are currently unreachable too, since `apply_remote_events` iterates avatar
entities. Iterating `remote_players` directly (and looking the avatar entity up for
muzzle/exclusion, falling back to the state position) fixes both problems at once.

### B2. CRITICAL — remote pedestrian avatars never animate

Two stacked causes:

1. `link_pedestrian_model` (spawn_pedestrian.rs, as modified) inserts `ManualAnimation` on
   **every** non-AI model — including remote avatars. `ManualAnimation` excludes the model
   from the shared `play_animations_system` (`Without<ManualAnimation>` in
   `pedestrians/animation.rs:115`), and the manual driver
   (`drive_character_animation`) only drives `ControlledCharacter.ped` — so remote models are
   driven by nothing. The `TargetAnimation` inserts from `update_remote_animations` are dead.
2. `update_remote_animations` queries `&LinearVelocity` on the remote root, but the OnFoot
   remote root is spawned **without** `LinearVelocity` (only the cosmetic car gets one), so
   the system matches nothing anyway. Same for the `q_vel.get_mut` writes in
   `interpolate_remote_avatars` — they silently no-op for pedestrians.

Fix:

- In `link_pedestrian_model`, insert `ManualAnimation` **only** when
  `controlled.controller == Some(controller)`; remote models stay on the shared
  `TargetAnimation` path. (AI branch is untouched.)
- Add `LinearVelocity::ZERO` to the OnFoot remote root spawn in `reconcile_remote_avatars`.
- Verify clip names used in `update_remote_animations` (`Idle_Loop`, `Walk_Loop`,
  `Jog_Fwd_Loop`, `Sprint_Loop`, `Death01`) against the catalog log
  (`print_animation_catalog`) at runtime; `node_for`-style fallback candidates would be more
  robust than exact names.
- While in there: scale anim playback `speed` with horizontal velocity like the AI does
  (optional polish), and use `Jump` / `Roll` / `ClimbStart` pending events (post-B1) to flash
  the matching one-shot clips instead of ignoring them.

### B3. Sending not gated on map load

The original plan gated `send_local_state` on `InitialMapLoadFinished::Finished`; the
implementation only gates on `NetworkConnectionState::Connected`. During loading the client
broadcasts freecam poses, and inbound peers spawn avatars into an unloaded world.

Fix: add `.run_if(in_state(InitialMapLoadFinished::Finished))` to `send_local_state`
(keep receive/reconcile running so state is warm when the map finishes — avatars parked at
their network positions are fine since tiles stream in around them).

### B4. `MultiplayerStats.connected` is fake

It's set to `true` the first time a message is sent or received, and rendered as
"Connected: Yes/No". Tie it to the real signal instead: set from
`ChatEvent::GameplayConnected` (A4) and initialize `false`. Bonus: show it in the debug
window as "game room: joined/joining" alongside `NetworkConnectionState`.

### B5. `Jump` event spams while the key is held

`collect_outbound_events` pushes `PlayerEventMsg::Jump` every frame `LocomotionInput.jump`
is true (it's held input state, not an edge). Climb/Roll/Melee use `Added<>` correctly.

Fix: edge-detect with a `Local<bool>` (prev jump state), or detect the actual jump the same
way the controller does (grounded→airborne transition). Low urgency while remote `Jump` is
unhandled, but it inflates every update's payload during a held key.

### B6. Cosmetic car wheels are chosen at random

`spawn_cosmetic_car` picks `wheels[0]` vs `wheels[last]` off `rand::random::<bool>()` —
the remote car's wheels won't match what the driver sees, and respawns reshuffle. Mirror
whatever `spawn_physics_car` does to pick the wheel for a `car_type` (deterministic from
`car_type`/`WheelAssets`); extract that selection into a small shared fn in
`car_info`/`spawn_car` and call it from both paths.

(Wheel cleanup is fine — `update_cosmetic_wheels` despawns wheels whose `parent_car` is
gone.)

### B7. One-shot events silently dropped on channel-full

In `send_local_state`, events are `mem::take`n into the update; if `try_send` fails they're
gone. Plan said events are "never dropped by rate limiting". Fix: on `try_send` error, push
the events back into `OutboundEvents` (the state snapshot itself is disposable, the events
aren't). Also skip the send entirely (and keep events) when `postcard::to_allocvec` fails.

### B8. Debug window & billboards gated on `Connected`

`multiplayer_debug_ui` runs only in `NetworkConnectionState::Connected`, so the new
"Online → Multiplayer Debug" menu toggle does nothing while connecting — confusing. Move
`multiplayer_debug_ui` out of the `run_if` (it should show "Connected: No" while
connecting). Billboards can keep the gate.

### B9. Reconnect/lifecycle leftovers (minor)

`SeenMsgIds`, `RemotePlayers`, and stats never reset. There is no disconnect path today
(`NetworkConnectionState` never leaves `Connected`), so just note it; if a reconnect flow is
added later these must be cleared on disconnect.

---

## C. Gaps vs the original plan (decide / defer)

- **Remote weapon display**: `weapon`, `ammo`, `aiming` are sent but ignored on receive.
  Remote on-foot avatars show no gun, and `find_muzzle_position` always falls back to
  `pos + 0.4Y` (no `WeaponModel` under the remote root), so tracers originate from the chest.
  Plan: insert `EquippedWeapon(WeaponId::from_label(...))` on the remote root and let
  `reconcile_weapon_model` handle the model — **verify** `reconcile_weapon_model` and
  `update_weapon_transforms` don't assume the local player (camera-relative aiming in
  `update_weapon_transforms` likely does; may need a `RemoteAvatarMarker` exclusion + a
  simple "attach to hand bone / fixed offset" path for remotes). Medium effort — schedule as
  its own step, after the criticals.
- **Camera peers have no nickname label**: `draw_remote_billboards` only iterates avatar
  entities; `Camera` avatars have none. Small fix: in the billboard system, also loop
  `remote_players` values whose avatar is `Camera` and project `latest` pose → draw the
  nickname (no health bar).
- **Presence-based despawn** ("gone from presence list"): not implemented; 5 s timeout
  covers it. Keep the timeout; optionally also set a `GameplayPresence` (currently never
  `set_presence` on the gameplay room) so future features (join notifications before first
  message, roster UI) have data. Low priority.
- **Rates in debug UI**: plan wanted msgs/s and updates/s per peer; current UI shows
  totals only. Nice-to-have: track a 1 s window in `MultiplayerStats`.
- **Interpolation `Camera` arm** in `interpolate_remote_avatars` is dead code (no entity) —
  delete the arm or leave; cosmetic.
- **Same-user multi-tab**: already handled — peers keyed by `node_id` (`PublicKey`), own
  messages filtered by node id in the net task. ✅

---

## D. Implementation order

1. **B1 event consumption** (multiplayer_plugin.rs) — pending_events on `RemotePlayer`,
   drain in `apply_remote_events`, iterate players not entities.
2. **B2 remote animation** (spawn_pedestrian.rs + multiplayer_plugin.rs) —
   `ManualAnimation` only for the controlled character; `LinearVelocity` on remote roots;
   verify clip names at runtime.
3. **A1 traffic off** (traffic/mod.rs) — `enabled: false`.
4. **A2 notifications plugin** (new `plugins/notifications.rs`; edits to geojson.rs,
   map_lod.rs, ui_egui.rs, plugins/mod.rs, main_game_plugin.rs) — port the two existing
   tooltips first, verify they still show, then wire the new events.
5. **A3/A4/A5 network notifications** (network/mod.rs + multiplayer_plugin.rs) —
   `ChatEvent::GameplayConnected`, triggers for NetworkConnected / GameNetworkOk /
   PlayerJoinedGame(+Left), B4 stats.connected.
6. **B3 send gating, B5 jump edge, B7 event re-queue, B8 debug window gate** — small,
   same-file batch in multiplayer_plugin.rs.
7. **B6 deterministic wheels** (spawn_car.rs/car_info.rs + multiplayer_plugin.rs).
8. **C remote weapon display + camera nickname labels** — separate pass, needs the
   `update_weapon_transforms` investigation.
9. **Verify**: `cargo check` + clippy + fmt; two native clients: chat connect → both
   tooltips fire once each; on-foot avatar walks/runs/idles remotely; one shot = one damage
   tick / one sound; traffic absent until enabled in the Traffic debug window; wasm build
   still compiles. Run `sigmap review-pr` before committing.
