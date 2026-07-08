# Multiplayer v1 — Plan

First iteration of player-to-player sync, piggybacking on the existing p2p chat layer
(`net_crackpipe` global chat room). Every client broadcasts its own state at a configurable
rate (default 20 Hz); every client renders the other players from those updates. No server
authority, no pedestrian/traffic sync, no entity ownership negotiation.

## Scope

**In:**
- New `GameSync` chat message type carrying a game update + random `i64` message id.
- Broadcast own state: control mode (freecam / on foot / in car), transforms, velocities,
  models, weapon state, and one-shot events (shoot, jump, climb, roll, reload).
- Render remote players: camera gizmo for freecam, pedestrian avatar on foot, car model in car.
- Apply remote shots as damage to the local player avatar (victim-authoritative).
- "Multiplayer Networking" egui debug window with send-rate slider (5–30 Hz, default 20).
- Ambient pedestrians **off by default** (opt-in via existing "Peds Enabled" checkbox).

**Out (explicitly not v1):** syncing traffic/AI pedestrians or cars, collision between remote
avatars and local physics, lag compensation, cheat resistance, reconciliation, voice, rooms
other than the global chat room.

## Architecture overview

```
Bevy (main thread)                      async net task (chat_main_task)
──────────────────                      ────────────────────────────────
send_local_state ──(i64, GameUpdate)──► game_out_rx ──► sender.broadcast_message(
   every 1/hz s        async channel                     GlobalChatMessageContent::GameSync)

apply_remote_updates ◄─(PeerId, GameUpdate)── game_in_tx ◄── recv.next_message()
   spawn/update/despawn      async channel                   (dedup by i64 id happens Bevy-side)
   remote avatars
```

The chat room is already joined on startup (`NetworkPlugin` → `start_network` →
`chat_main_task`), so "players join game sync when they join the main game" is free: we just
add a second message variant flowing through the same controller.

## 1. Wire protocol — `packages/net_crackpipe`

`packages/net_crackpipe/src/chat/global_chat.rs`, extend the existing enum:

```rust
pub enum GlobalChatMessageContent {
    TextMessage { text: String },
    SpectateMatch { ... },
    BootstrapQuery(...),
    // NEW: opaque game-state update. `id` is a random i64 used by receivers to
    // drop duplicate gossip deliveries. `payload` is a postcard-serialized
    // GameUpdate defined game-side (net layer stays game-agnostic).
    GameSync { id: i64, payload: Vec<u8> },
}
```

Keeping the payload as `Vec<u8>` means `net_crackpipe` never depends on game types and the
game can evolve the struct freely. Old clients ignore the unknown variant (verify serde of the
enum tolerates unknown variants across versions; if not, acceptable for v1 — everyone updates).

## 2. Game-side protocol structs — `crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs` (new file)

Note: the user-facing spec said `src/plugins/networking/` (nonexistent), but the existing directory is
`src/plugins/network/` — the new plugin goes there as a `multiplayer_plugin` submodule (of
`network`), optionally split into `multiplayer_plugin/{mod,protocol,remote_avatar,debug_ui}.rs`
if it grows past ~500 lines.

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GameUpdate {
    /// Sender's monotonic time in seconds, for interpolation/extrapolation.
    pub t: f64,
    pub state: PlayerStateMsg,
    /// One-shot events accumulated since the previous update (never dropped by rate limiting).
    pub events: Vec<PlayerEventMsg>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlayerStateMsg {
    /// GameControlState::MapFreecam — show a camera gizmo at this pose on other clients.
    Camera { pos: Vec3, rot: Quat },
    /// GameControlState::ControllingPedestrian
    OnFoot {
        model_url: String,       // PedestrianUrl.0 of the controlled character
        scale: f32,              // CharacterScale
        pos: Vec3,
        rot: Quat,
        vel: Vec3,               // LinearVelocity, for extrapolation + anim speed
        grounded: bool,
        aiming: bool,            // crosshair/aim state (GameControlState + weapon raised)
        weapon: String,          // WeaponId label/serialized id
        ammo: u32,               // GunState clip, for HUD-over-head later; cheap to include
        health: f32,             // current HP (victim-authoritative, see §6)
    },
    /// GameControlState::DrivingCar
    InCar {
        car_type: String,        // car_info car type name → resolves glb via get_car_asset
        pos: Vec3,
        rot: Quat,
        vel: Vec3,
        speed_kmh: f32,
        steer: f32,              // CarDriveState.current_steer_integrated → front wheel pose
        health: f32,             // CarHealth.current
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum PlayerEventMsg {
    /// A gunshot: replayed on remote clients for tracer VFX + victim-side hit test.
    Shoot { origin: Vec3, dir: Vec3, damage: f32 },
    Reload,
    Jump,
    ClimbStart,
    Roll,
    Melee,
}
```

Serialization: `postcard` (add to demo crate deps if absent; `serde` already required by
net_crackpipe types). ~100–250 bytes per update → at 20 Hz with N players this is
N×~4 KB/s inbound, fine for gossip in small rooms.

`Vec3`/`Quat`: serialize as `[f32; 3]` / `[f32; 4]` (glam serde feature or manual tuples) to
avoid depending on bevy math serde features in the protocol.

## 3. Plumbing — `crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/mod.rs`

Current flow only handles `TextMessage`. Changes:

1. New resource created in `start_network` (both native and wasm variants):
   ```rust
   #[derive(Resource)]
   pub struct GameSyncChannels {
       pub outgoing_tx: async_channel::Sender<Vec<u8>>,          // payload; id generated net-side or here
       pub incoming_rx: async_channel::Receiver<GameSyncInbound>,
   }
   pub struct GameSyncInbound {
       pub from_user_id: PublicKey,     // NodeIdentity::user_id — stable peer key
       pub nickname: String,
       pub color: (u8, u8, u8),
       pub id: i64,
       pub payload: Vec<u8>,
   }
   ```
   Bounded channels (e.g. 256); on `try_send` failure drop the oldest/newest update —
   state updates are disposable, but log a counter.
2. `chat_main_task`:
   - Spawn one more forwarding task: `game_outgoing_rx.recv()` →
     `sender.broadcast_message(GlobalChatMessageContent::GameSync { id: rand::random::<i64>(), payload })`.
     (Reuse the existing outgoing-task pattern; do **not** echo own game messages back into
     the incoming channel, unlike text messages.)
   - Incoming loop: on `GlobalChatMessageContent::GameSync { id, payload }` push
     `GameSyncInbound` into `game_incoming_tx` + `proxy.send_event(WakeUp)` (WakeUp matters:
     desktop is winit-reactive; without it remote players freeze when the local window is idle).
     Skip messages where `msg.from.user_id() == own user id` (gossip may loop back).
3. Keep `ChatState`/text behavior untouched. Register `MultiplayerPlugin` from
   `NetworkPlugin::build` (or `crack_demo/demo_resolution_selector_web_bevy/src/lib.rs` next to it — follow however `GlobalChatPlugin` is added).

## 4. MultiplayerPlugin — resources & systems

```rust
pub struct MultiplayerPlugin;

#[derive(Resource)]
pub struct MultiplayerConfig {
    pub send_hz: f32,          // default 20.0, slider 5.0..=30.0
    pub window_open: bool,     // debug window toggle
}

#[derive(Resource, Default)]
pub struct OutboundEvents(Vec<PlayerEventMsg>);   // drained each send tick

#[derive(Resource, Default)]
pub struct SeenMsgIds(/* small LRU ring, e.g. VecDeque<i64> + HashSet, cap 1024 */);

#[derive(Resource, Default)]
pub struct RemotePlayers(HashMap<PublicKey, RemotePlayer>);

pub struct RemotePlayer {
    pub nickname: String,
    pub color: (u8, u8, u8),
    pub last_rx: f64,                    // local time of last accepted update
    pub prev: Option<GameUpdate>,        // for interpolation
    pub latest: Option<GameUpdate>,
    pub avatar: RemoteAvatar,            // what's currently spawned for this peer
}

pub enum RemoteAvatar {
    None,
    Camera,                              // pure gizmo, no entity
    OnFoot { root: Entity, model_url: String },
    InCar  { root: Entity, car_type: String },
}
```

Systems (all `run_if(in_state(NetworkConnectionState::Connected))`, and sending additionally
gated on `InitialMapLoadFinished::Finished`):

| System | Schedule | Job |
|---|---|---|
| `collect_outbound_events` | `Update` | Observers/hooks record local one-shot events into `OutboundEvents` (see §6). |
| `send_local_state` | `Update`, self-timed via `Local<f32>` accumulator against `1.0 / send_hz` | Snapshot own state per `GameControlState`, drain `OutboundEvents`, postcard-encode, `try_send` on `GameSyncChannels.outgoing_tx`. |
| `receive_game_sync` | `Update` | Drain `incoming_rx`, dedup via `SeenMsgIds`, decode payload, shift `latest → prev`, store in `RemotePlayers`. |
| `reconcile_remote_avatars` | `Update` | Spawn/despawn/swap avatar entities when a peer's state kind or model changed; despawn peers silent > 5 s or gone from presence list. |
| `interpolate_remote_avatars` | `Update` (after reconcile) | Lerp/slerp between `prev`→`latest` (render ~1.5 update intervals behind newest), extrapolate with `vel` up to ~200 ms, write `Transform`. |
| `apply_remote_events` | `Update` | Play tracers/sfx/anims for received events; victim-side hit test (§6). |
| `draw_camera_gizmos` | `Update` | For `Camera` peers: gizmo frustum/axes + nickname text at the pose. |
| `multiplayer_debug_ui` | `EguiPrimaryContextPass` | §7. |

State snapshot sources:
- **Camera**: `Query<&GlobalTransform, With<Camera3d>>` (freecam).
- **OnFoot**: `ControlledCharacter` resource → controller entity → `Transform`,
  `LinearVelocity`, `CharacterScale`, `Grounded`, `EquippedWeapon`/`WeaponSelection` +
  `GunState`, `Health`, and the model url used at spawn (store `PedestrianUrl` on the
  controller entity at spawn time if not already there — check `spawn_controlled_pedestrian_observer`).
- **InCar**: the `ActivePlayerVehicle` car → `Transform`, `LinearVelocity`, `CarDriveState`
  (speed/steer), `CarHealth`, and its `car_type` (store the `String` from
  `SpawnCarRequestEvent` as a component on the car root if not already kept).

## 5. Remote avatar representation

Remote avatars are **visual-only**: no `CharacterController`, no AI, no dynamic physics.
Kinematic/no rigid body + the rendered model; a simple collider only if needed for local
bullet raycasts to be able to *hit* them (see §6 — v1 victim-side hit test does not require it,
so skip colliders entirely in v1 and hit-test against the *local* player instead).

- **OnFoot**: spawn root entity with `RemoteAvatarMarker { peer }` + fire the existing
  `SpawnPedestrianEvent { url, position, controller: root, parent: root }` to reuse the whole
  GLB fetch/animation pipeline. Drive animations by writing a synthetic `LocomotionInput` /
  `AnimState`-compatible signal derived from received `vel`/`grounded`/events, so the existing
  locomotion animation systems pick walk/run/idle/jump clips. Nickname + health bar billboard
  above the head (egui painter or world-space text like existing debug UI does).
  ⚠ Verify `link_pedestrian_model` / animation systems don't assume the entity is the
  locally-controlled one (`ControlledCharacter`); add a marker check if they do.
- **InCar**: spawn the car GLB via `car_info::get_car_asset(car_type, …)` + wheels
  (`WheelAssets`), but **not** the drivable physics bundle from `spawn_car` — a slimmed
  "cosmetic car" spawn path (new function in `multiplayer_plugin` or a flag on the existing
  spawn path). Steer angle applied to front wheel meshes from `steer`; optionally reuse the
  driver-mesh seating visuals later.
- **Camera**: no entity — gizmo lines each frame (pyramid frustum + up axis) colored with the
  peer's chat color, plus nickname label.

Model/state swaps (peer enters/exits car, picks new pedestrian model): despawn old avatar
subtree, spawn new — at 20 Hz and rare transitions this is fine for v1.

## 6. Shooting & damage (victim-authoritative)

Outbound: the local shot already flows through `FireGunEvent` → observer in
`crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs`. Add a hook (second observer on `FireGunEvent`, or a line in
`fire_gun_observer`) that — only when the shooter is the `ControlledCharacter` — pushes
`PlayerEventMsg::Shoot { origin, dir, damage }` into `OutboundEvents` using the same
muzzle origin/direction and the `GunInfo.damage` it resolved. Same for `Reload`, and the
locomotion systems (`jump_or_climb`, roll) push `Jump`/`ClimbStart`/`Roll`.

Inbound, per received `Shoot`:
1. **VFX on every client**: raycast the world (`SpatialQuery`) from `origin` along `dir`,
   draw `ShotTracer`/`BulletSpark` via the existing `ShotTracers`/`BulletSparks` resources.
2. **Damage only on the victim's client**: if the ray's first hit is the local player —
   segment-vs-capsule test against the local `ControlledCharacter` (on foot) or ray-vs-AABB of
   the `ActivePlayerVehicle` (in car), *and* nothing in the world occludes it (compare ray hit
   distance) — then `commands.trigger(DamageEvent { target: local_entity, amount, source: <a
   placeholder entity or the remote avatar root> })` / subtract `CarHealth`. Existing
   death flow (`player_death_to_freecam`, `Dying`) handles the rest.
3. The victim's next updates carry the reduced `health`, so the shooter sees the effect on the
   remote health bar. No kill credit/scoreboard in v1.

This needs no colliders on remote avatars and no entity-id mapping; the only trust issue is
the victim deciding it was hit, which is acceptable for v1.

## 7. Debug window — "Multiplayer Networking"

`multiplayer_debug_ui` in the plugin, registered on `EguiPrimaryContextPass`, following
`traffic_debug_ui`'s pattern (including the `Option<ResMut<UiState>>` gating if that's how
windows are toggled there — mirror it):

- Slider: **Network update rate** `5.0..=30.0` Hz, default **20**, writes `MultiplayerConfig.send_hz`.
- Read-only stats: connection status, own nickname/user-id, peers table (nickname, state kind,
  last-rx age, updates/s), counters: msgs sent/received per second, bytes/s, duplicate-id drops,
  decode errors, channel-full drops.

## 8. Pedestrians off by default

`crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/mod.rs` — `impl Default for TrafficConfig`: `ped_enabled: true` →
**`false`**. The existing "Peds Enabled" checkbox in `traffic_debug_ui` (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/debug_ui.rs:79`)
becomes the opt-in. Leave `max_peds`, car traffic, and everything else unchanged. (Grep for
any other place that force-enables peds — none known.)

## 9. Implementation order

1. **Protocol + plumbing** — `GameSync` variant in net_crackpipe; `GameSyncChannels`,
   forwarding tasks + incoming handling in the network plugin module (native + wasm paths). Smoke-test
   with two native clients logging raw payloads.
2. **Plugin skeleton** — `multiplayer_plugin.rs`: config, resources, `send_local_state`
   (Camera variant only) + `receive_game_sync` + dedup + camera gizmo rendering. Two clients
   see each other's freecams. ✅ first visible milestone.
3. **Debug window** — rate slider + stats (cheap, and makes the rest debuggable).
4. **OnFoot sync** — snapshot + remote pedestrian avatar spawn/interp/anim; nickname billboard.
5. **InCar sync** — car_type component on spawn, cosmetic car spawn path, wheel steer.
6. **Events & damage** — outbound event capture, tracer replay, victim-side hit test,
   health in updates + remote health bar.
7. **Peds default off** — one-line `TrafficConfig` change.
8. **Polish/verify** — timeout despawn, presence-leave despawn, wasm build
   (`postcard`/`rand` wasm-compat — `rand::random::<i64>` needs `getrandom` js feature, already
   used elsewhere in workspace: verify), `cargo fmt`, run `sigmap review-pr`.

## 10. Risks / open questions

- **Gossip broadcast semantics**: assumed at-least-once with possible duplicates and loop-back
  of own messages — the random `i64` id + own-user-id filter covers both. Verify actual
  behavior in `net_crackpipe` chat controller.
- **Rate**: 30 Hz × N players through a chat gossip layer may hit message-size/rate limits in
  `net_crackpipe` — check any rate limiting in the sender before raising defaults.
- **Enum evolution**: adding a `GlobalChatMessageContent` variant may break decode for peers on
  old builds (depends on serde format used by the chat layer — likely postcard = index-based,
  so *appending* the variant last is mandatory).
- **Animation reuse for remote avatars**: locomotion/anim systems may query
  `ControlledCharacter`-adjacent state; budget time to add `RemoteAvatarMarker` exclusions.
- **Same user, two tabs**: `user_id` keys the peer map; two nodes with one identity would fight
  over one avatar. Acceptable v1; could key by `(user_id, node_id)` if it bites. -- yes, key by node id so each tab has its own player. in the original code this was a struct called NodeIdentity, run a grep for that.
