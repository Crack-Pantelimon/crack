# Multiplayer v1 — Plan 3 (disconnect fix + remote-player physics)

Status review of the previous plans, the root cause of the ~30s "everyone disconnects"
bug, and the next implementation slice: physics bodies for remote players so cars (and
pedestrians) of different players collide on every simulation.

---

## A. Status of PLAN.md / PLAN_2.md

Verified in code — both plans are **complete**:

- v1 protocol, gameplay room (`global_gameplay`), send/receive/reconcile/interpolate,
  events, victim-authoritative damage, debug window: all landed in
  `multiplayer_plugin.rs` + `network/mod.rs`.
- PLAN_2 A1 (traffic off by default) ✅ `TrafficConfig { enabled: false, ped_enabled: false }`.
- A2 notifications plugin ✅ (`plugins/notifications.rs`), A3/A4/A5 network + join/leave
  notifications ✅.
- B1 pending_events drain ✅, B2 `ManualAnimation` only for local player ✅ +
  `LinearVelocity` on remote roots ✅, B3 send gated on `InitialMapLoadFinished` ✅,
  B4 real `stats.connected` ✅, B5 jump edge-detect ✅, B6 deterministic wheels
  (`select_car_wheel`) ✅, B7 event re-queue on channel-full ✅, B8 debug window ungated ✅.
- C: remote weapon display ✅ (`EquippedWeapon` on remote root), camera-peer nickname
  labels ✅, msgs/s rates in debug UI ✅.

---

## B. The preset-time disconnect (root cause + fix)

Symptom: multiplayer works, then at a fixed moment (~30 s after the gameplay room joins,
right after `wait_until_joined: found 1/4, attempts: 97…` stops logging) all peers drop.

**Root cause — two stacked bugs:**

1. `ChatController::wait_joined()` (chat_controller.rs) hardcodes "3 nodes present" as
   the join criterion. The gameplay room's ticket lists the 4 bootstrap nodes, but
   **bootstrap nodes never subscribe to the gameplay topic**, so presence stays at 1
   (ourselves) and the loop spins its full 100 × `CONNECT_TIMEOUT/10` ≈ 30 s.
2. The fatal part: in `chat_main_task` (network/mod.rs), the task spawned to run
   `gameplay_c.wait_joined()` holds the **last live clone of the gameplay
   `ChatController`**. The controller owns the room's `_dispatch_task` and
   `_presence_task` as `AbortOnDropHandle`s; the `ChatSender`/`ChatReceiver` clones used
   by the sync tasks do *not* keep those alive. When `wait_joined` finally gives up, the
   controller drops → dispatch + presence heartbeat abort → we stop receiving GameSync
   and stop heartbeating → 10 s later every peer times out on both sides. Hence
   "disconnects after the retry gives up".

**Fix:**

1. `wait_joined(min_nodes: usize)` — parameterized join criterion: the room counts as
   joined once `presence ∪ {self}` contains ≥ `min_nodes` distinct nodes.
   - gameplay room (`network/mod.rs`): `wait_joined(1)` — bootstrap nodes don't join
     this room, we only need ourselves; returns immediately.
   - global chat (`network/mod.rs`, `chat_cli`): `wait_joined(2)` — us + a bootstrap node.
   - server chat (`api/join_chat.rs`): `wait_joined(2)` — us + the server node.
2. Keep the gameplay `ChatController` owned by `chat_main_task` for its whole lifetime
   (borrow it in the spawn block instead of moving it), so room tasks can never be
   aborted by a helper task finishing. This fix matters independently of (1): with (1)
   alone the controller would drop *instantly* and break the room even faster.

---

## C. Remote-player physics (this plan's feature)

Today `spawn_cosmetic_car` and the remote OnFoot root spawn are visual-only — no
`RigidBody`, no `Collider`. Consequences: players' cars drive through each other, the
local car drives through remote pedestrians, and the local player walks through remote
avatars.

Each client keeps its own simulation authoritative for its own car; remote players'
vehicles/bodies become **kinematic obstacles** driven by the interpolated network pose
(same pattern as the local `CharacterController`, which is already
`RigidBody::Kinematic` + manual `Transform` writes + `LinearVelocity`). The local
dynamic car then collides and gets a proper impulse response; the remote car is
unaffected on our screen (its owner's simulation stays authoritative, and its pose keeps
coming from the network).

### C1. Remote cars (multiplayer_plugin.rs, `spawn_cosmetic_car`)

Add to the car root:

- `RigidBody::Kinematic`
- `ColliderConstructorHierarchy::new(ColliderConstructor::ConvexDecompositionFromMesh)`
  with layers `([Car], [Car])` — same mesh-accurate collider pipeline as
  `spawn_physics_car`, so car-vs-car contacts match the visible bodywork and local
  bullet raycasts hit the real shape. Filter is `[Car]` only (not `Map`): a kinematic
  body has no response anyway, so map pairs would be wasted broadphase work.

`interpolate_remote_avatars` already writes `Transform` + `LinearVelocity` every frame;
with a kinematic body avian gets both an authoritative pose and a contact velocity, so
impacts on the local (dynamic) car resolve with believable momentum.

### C2. Remote pedestrians (reconcile_remote_avatars, OnFoot root spawn)

Mirror the local character's physics footprint:

- `RigidBody::Kinematic`
- `Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH)` (root is the capsule center,
  matching what the sender transmits)
- `CollisionLayers::new([Car], [Car])` — the same membership the local character uses,
  so local car ↔ remote ped and local ped ↔ remote ped pairs both exist.

### C3. Shot replay must exclude the shooter's whole avatar subtree

`apply_remote_events` currently excludes only the avatar **root** entity from the
`Shoot` raycast. The new car colliders live on GLB child entities
(`ColliderConstructorHierarchy`), so a remote player's replayed shots could impact their
own car right at the muzzle. Fix: collect root + descendants (`Query<&Children>`) into
`SpatialQueryFilter::from_excluded_entities`.

### Non-goals / deferred

- No ownership transfer or authority negotiation on collision — each car's pose stays
  owner-authoritative; a hard ram shoves only the rammer's car on the rammer's screen
  until the victim's own simulation reacts (acceptable for v1.x).
- Collision *damage* from remote impacts (victim-side, like bullets) — later.
- `DisabledCar` visuals / smoke on remote cars, steering-angle on remote front wheels —
  cosmetic, later.
- Reconnect lifecycle reset of `SeenMsgIds`/`RemotePlayers` — still noted in code.

## D. Implementation order

1. net_crackpipe: `wait_joined(min_nodes)` + all four call sites.
2. network/mod.rs: gameplay controller lifetime fix (+ `wait_joined(1)`).
3. multiplayer_plugin.rs: C1 remote car body/collider, C2 remote ped capsule, C3 subtree
   exclusion.
4. `cargo check` + clippy + fmt; two-client test: cars collide, disconnect no longer
   happens at ~30 s (soak > 2 min).
