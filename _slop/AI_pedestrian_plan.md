# Pedestrian AI: factions, perception, combat behaviors + `turf_war` test bin

## Context & goal

Add a new `pedestrian_ai` game plugin that turns spawned pedestrians into autonomous
combatants: each has HP, a weapon held in hand, an optional faction, and a behavior
state machine (idle → hunt → fight → flee) driven by line-of-sight perception. Add a
`turf_war.rs` binary that drops several factions into an arena full of obstacles, sets
them all at war, and lets them fight while logging AI actions.

All paths below are relative to `crack_demo/demo_resolution_selector_web_bevy/src/`
unless noted.

### Key facts discovered from the codebase

- **The player controller is single-agent.** `controller::movement`
  ([`plugins/pedestrians/pedestrian_controller_plugin/controller.rs:88`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs#L88))
  reads a **global** `MovementAction` message and applies it to **every**
  `CharacterController`. With more than one controller alive (player + AI peds), one
  agent's input moves all of them. This must be fixed before multi-agent AI works in the
  main game (see **Phase 0**).
- The locomotion systems (`update_grounded`, `apply_gravity`, `movement`,
  `apply_movement_damping`, `apply_speed_cap`, `move_and_slide`,
  `apply_forces_to_dynamic_bodies`, `face_movement`, `update_climb`, `update_roll`,
  `respawn_if_fallen`) are all **per-entity over all `CharacterController`s**, but the
  functions live in a private module and the FixedUpdate chain is gated by
  `run_if(in_state(GameControlState::ControllingPedestrian))`
  ([`mod.rs:323`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L323)).
  AI peds must run this chain regardless of the player's control state.
- **No health/damage system exists.** `fire_gun_observer` only logs `"{} dmg"`
  ([`weapon_shooting.rs:194`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L194));
  `PendingMeleeHit` only plays a sound. We add HP + damage application.
- `fire_gun_observer` fires from the **camera** (`camera.forward()`), so it is unusable
  for NPCs. AI needs a directed-shot path (**Phase 4**).
- Weapons attach on the **character entity**: `EquipWeaponEvent { character, weapon }`
  inserts `EquippedWeapon` (+ `GunState` for guns), and `reconcile_weapon_model` finds
  the wrist by searching descendants for `PedestrianSkeleton`
  ([`weapon_attach.rs:111`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L111)).
  Equipping on the AI capsule controller (with the ped model as a child, exactly like the
  player) works unchanged.
- AI ped animation is easy: peds spawned via `SpawnPedestrianEvent` are driven by the
  shared `play_animations_system`, which honors `TargetAnimation`
  ([`pedestrians/animation.rs`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs)).
  Triggering `PedestrianAnimationControlEvent { ped, animation, speed }` switches a ped's
  whole-body clip. **We do NOT need the player's dual-layer `drive_character_animation`**
  — AI switches full clips (idle/walk/run/shoot/sword/punch). The AI ped model must
  therefore **not** get the `ManualAnimation` marker (player's `adopt_pedestrian` adds it;
  our AI adopt path must not).
- `is_person_entity` (walks `ChildOf` up to a `CharacterController`/`ModelRoot`/skeleton)
  is private in `weapon_shooting.rs:58` — reuse it by making it `pub(crate)`.
- `PedestrianManifest.urls` and `WeaponManifest { guns, melee, all, loaded }` are the
  spawn sources. `random_u32()` from `_crack_utils` is the wasm-safe RNG already used for
  weapon sound picks.
- Debug ground collides with `Car`/`Wheel` layers only; the player capsule is spawned on
  `GamePhysicsLayer::Car` and collides with `Map|Car|Wheel`
  ([`spawn.rs:96`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs#L96)).
  AI capsules must use the **same** layers so they touch the ground and each other.

---

## Phase 0 — Make locomotion multi-agent safe (shared refactor)

**Why:** the global `MovementAction` broadcast is a latent bug the moment a second
`CharacterController` exists. We replace it with a per-entity input component and split
the reusable physics chain into its own plugin that both the player and AI depend on.

### 0.1 New component: per-entity locomotion input

In `pedestrian_controller_plugin/mod.rs`:

```rust
/// Per-entity desired locomotion, written by whoever drives this controller
/// (keyboard for the player, the AI brain for NPCs). Consumed by `movement`.
#[derive(Component, Default)]
pub struct LocomotionInput {
    /// Planar move direction, avian convention: `x -> +x`, `y -> -z`. Zero = no input.
    pub move_dir: Vec2,
    /// Set true for one frame to request a jump; `movement` consumes and clears it.
    pub jump: bool,
}
```

Add `LocomotionInput` to the `#[require(...)]` list on `CharacterController` (so every
controller has one) and to the player spawn bundle in `spawn.rs`.

### 0.2 Rewrite `controller::movement` to read the component

Replace the `MessageReader<MovementAction>` loop with a per-entity loop over
`(&mut LocomotionInput, &CharacterMovementSettings, &mut LinearVelocity, Has<Grounded>)`:
accelerate `linear_velocity` by `move_dir` (same math, `× acceleration × dt`), apply the
jump impulse when `jump && grounded`, then clear `jump`. Keep it in `FixedUpdate`.
`MovementAction` and its `MessageWriter`/`add_message` registration are removed.

- `character_input` ([`controller.rs:12`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs#L12))
  now writes `LocomotionInput.move_dir` + `MovementModifiers` **only on the controlled
  entity** (query `With<CharacterController>`; the player is the only keyboard-driven one,
  and it stays gated to `ControllingPedestrian`, so it never touches AI peds).
- `jump_or_climb` ([`controller.rs:385`](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs#L385))
  sets `LocomotionInput.jump = true` (or inserts `Climbing`/`Rolling`) instead of writing
  the message.

### 0.3 Extract `CharacterLocomotionPlugin`

New `pedestrian_controller_plugin/locomotion.rs` exposing:

```rust
pub struct CharacterLocomotionPlugin;
```

It registers the **un-gated** per-entity chain for all controllers:

- `FixedUpdate` (chained, `run_if(no_one_climbing)` kept):
  `update_grounded, apply_gravity, movement, apply_movement_damping, apply_speed_cap,
   move_and_slide, apply_forces_to_dynamic_bodies`.
- `Update`: `face_movement, update_climb, update_roll, respawn_if_fallen,
   detect_fallen_off_map`.

Make the needed systems `pub` (they already are `pub fn`; just re-export via the module).
`PedestrianControllerPlugin` keeps only the **player-specific** systems (input, camera,
player animation `drive_character_animation`, interaction UI, spawn/adopt, weapon wheel),
gated on `ControllingPedestrian`, and adds `CharacterLocomotionPlugin` behind a guard:

```rust
if !app.is_plugin_added::<CharacterLocomotionPlugin>() {
    app.add_plugins(CharacterLocomotionPlugin);
}
```

`PedestrianAiPlugin` adds it the same guarded way, so the main game (both plugins present)
registers it exactly once.

**Verification of Phase 0 in isolation:** existing `pedestrian_controller` bin still walks/
jumps/climbs/rolls identically; nothing else changes yet.

---

## Phase 1 — Faction & health model (`plugins/pedestrian_ai/faction.rs`)

```rust
/// Static faction roster. `Neutral` never fights and is never targeted.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Faction { Neutral, Red, Green, Blue, Yellow }

impl Faction {
    pub const COMBATANTS: [Faction; 4] = [Faction::Red, Faction::Green, Faction::Blue, Faction::Yellow];
    pub fn color(self) -> Color { /* red/green/blue/yellow/gray for gizmos + tint */ }
    pub fn label(self) -> &'static str { /* "Red", ... */ }
}

/// Static war matrix resource. `wars: Vec<(Faction, Faction)>` (unordered pairs).
#[derive(Resource, Default)]
pub struct WarMatrix { pub wars: Vec<(Faction, Faction)> }

impl WarMatrix {
    pub fn all_out_war() -> Self { /* every distinct COMBATANTS pair */ }
    pub fn at_war(&self, a: Faction, b: Faction) -> bool {
        if a == Faction::Neutral || b == Faction::Neutral || a == b { return false; }
        self.wars.iter().any(|&(x, y)| (x==a && y==b) || (x==b && y==a))
    }
}

/// Hit points. Death handled centrally when `current <= 0`.
#[derive(Component)]
pub struct Health { pub current: f32, pub max: f32 }
impl Health { pub fn full(max: f32) -> Self { Self { current: max, max } } }
```

`DEFAULT_HP = 100.0`. Enemy test = `war_matrix.at_war(my_faction, their_faction)`.

---

## Phase 2 — AI components & the plugin skeleton (`plugins/pedestrian_ai/mod.rs`)

```rust
/// Marks an AI-driven pedestrian (present on the capsule controller entity).
#[derive(Component)]
pub struct AiPedestrian;

/// Current behavior state. Logged on every transition.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum AiState {
    Idle,
    Hunt,        // has a visible enemy: move to engage + attack per weapon
    Reposition,  // gun only: break contact to reload behind cover
    Flee,        // low HP, or gun-enemy inside PANIC_RANGE
}

/// Live perception result, refreshed each tick by the perception system.
#[derive(Component, Default)]
pub struct AiPerception {
    pub target: Option<Entity>, // nearest visible enemy controller
    pub target_pos: Vec3,       // enemy head pos (LOS endpoint) when visible
    pub target_dist: f32,
    pub visible: bool,
}

/// Attack pacing + reload bookkeeping.
#[derive(Component, Default)]
pub struct AiCombatTimers {
    pub attack_cooldown: f32, // gun burst / melee swing cadence
    pub reload_timer: f32,    // >0 while "reloading" (Reposition)
    pub repath_timer: f32,    // jitter for flank/flee direction recompute
}

/// Cached steering direction (recomputed on `repath_timer`), so flank/flee paths are stable.
#[derive(Component, Default)]
pub struct AiSteer { pub desired: Vec3 } // world-space planar dir
```

`PedestrianAiPlugin::build`:

- guarded `add_plugins(CharacterLocomotionPlugin)` (Phase 0.3),
- `init_resource::<WarMatrix>()` (default empty; binary/game fills it),
- `init_resource::<AiDebug>()` (Phase 7),
- `add_observer(spawn_ai_pedestrian_observer)` (Phase 3),
- `Update` systems, ordered:
  `adopt_ai_pedestrian` → `ai_perception` → `ai_brain` →
  `ai_movement` → `ai_combat` → `apply_damage_and_death`,
- `Update`: `draw_ai_gizmos` (Phase 7),
- `EguiPrimaryContextPass`: `ai_debug_ui` (Phase 7).

AI systems are **not** state-gated (they run in the main game whatever the player is
doing, and in the headless `turf_war` bin which has no `GameControlState` transitions).

---

## Phase 3 — Spawning AI pedestrians (`plugins/pedestrian_ai/spawn_ai.rs`)

Mirror the player spawn (`spawn.rs:47`) but self-contained (no `ControlledCharacter`, no
state change):

```rust
#[derive(Event)]
pub struct SpawnAiPedestrianEvent {
    pub position: Vec3,
    pub faction: Faction,
    pub url: Option<PedestrianUrl>,   // None = random from manifest
    pub weapon: Option<WeaponId>,     // None = random from WeaponManifest.all
}
```

`spawn_ai_pedestrian_observer`:

1. Pick `url` from `PedestrianManifest.urls` (random if `None`); bail with `warn!` if the
   manifest is empty.
2. Spawn the **capsule controller** exactly like the player (`CharacterController`,
   `CharacterScale`, `CharacterMovementSettings::default()`, `CharacterCollisions`,
   `MovementModifiers`, `LocomotionInput`, `GroundDetection`, capsule `Collider`, the same
   `CollisionLayers::new(Car, [Map, Car, Wheel])`, `Transform`, `Visibility`) **plus** the
   AI bundle: `AiPedestrian`, `faction`, `Health::full(DEFAULT_HP)`, `AiState::Idle`,
   `AiPerception::default()`, `AiCombatTimers::default()`, `AiSteer::default()`. Do **not**
   add `AnimState`/`CombatState` (those are player-anim only).
3. Spawn the intermediate scale node child (offset `-CAPSULE_HALF_HEIGHT`, scale).
4. Track the pending adoption: insert a small `PendingAiAdopt { controller, scale_node,
   weapon }` resource entry (a `Resource(Vec<..>)` or a component on the controller) and
   trigger `SpawnPedestrianEvent { url, position }`.

`adopt_ai_pedestrian` (mirrors `adopt_pedestrian` but multi-agent): for each
`Added<ModelRoot>` not already owned by the player's `ControlledCharacter`, match it to
the oldest pending AI adopt, then:

- `ChildOf(scale_node)`, `Transform::IDENTITY` (**no `ManualAnimation`** — shared
  `play_animations_system` drives the clip),
- store the model root entity as `AiModel(Entity)` on the controller (needed to target
  `PedestrianAnimationControlEvent`),
- trigger `EquipWeaponEvent { character: controller, weapon }` (random weapon → held in
  hand via the existing reconcile/attach path).

**Ambiguity note:** matching `Added<ModelRoot>` to the right pending controller relies on
spawn order (peds appear in trigger order). This is the same assumption the player path
makes; with a FIFO pending queue it is deterministic enough for the arena. If the main
game ever spawns AI + player peds in the same frame, gate `adopt_ai_pedestrian` to skip
the entity the player's `adopt_pedestrian` claimed (check `controlled.ped`).

---

## Phase 4 — Perception (`plugins/pedestrian_ai/perception.rs`)

`ai_perception(SpatialQuery, Query<(Entity, &GlobalTransform, &Faction, &AiPerception..)>,
war: Res<WarMatrix>, parents, is-person queries)`:

For each live AI ped, find the nearest enemy that is **both** within `SIGHT_RANGE = 50.0`
**and** has line of sight:

- Candidate set: other `AiPedestrian`s (and, in the main game, the player controller — any
  entity with `Faction`) whose faction `war.at_war(mine, theirs)`.
- Coarse cull by distance first (squared), sorted nearest-first.
- **LOS test:** cast a ray from **my head** (`pos + Vec3::Y * HEAD_OFFSET`, ≈ capsule top)
  toward **their head**, `range = dist`, filter excludes self. Visible iff the first hit's
  entity resolves (via `is_person_entity`, now `pub(crate)`) to the candidate's controller
  subtree. Store the first visible nearest as `target`, record `target_pos` (their head),
  `target_dist`, `visible = true`. If none, `visible = false`, keep `target = None`.

`HEAD_OFFSET ≈ CAPSULE_HALF_HEIGHT` (~0.85). Cache the LOS ray endpoints for gizmos
(Phase 7) in `AiPerception` (e.g. `pub last_los: Option<(Vec3, Vec3, bool)>`).

O(n²) over the arena's ped count is fine (tens of peds).

---

## Phase 5 — Brain / state machine (`plugins/pedestrian_ai/brain.rs`)

`ai_brain(time, Query<(Entity, &Health, &EquippedWeapon?, &GunState?, &AiPerception,
&mut AiState, &mut AiCombatTimers)>)`. Tick `attack_cooldown`, `reload_timer`,
`repath_timer` down by `dt`. Compute the desired state and **log on change**
(`info!("[AI {e}] {old:?} -> {new:?}")`):

Priority order:

1. **Flee** if `health.current <= FLEE_HP (30.0)`, **or** (gun equipped and a visible enemy
   is within `PANIC_RANGE = 6.0` — "flee if enemy too close").
2. **Reposition** if gun equipped, has a visible target, and `GunState.rounds == 0` (or
   `reload_timer > 0`): start a reload — set `reload_timer = RELOAD_TIME (2.0)`, and when it
   expires trigger `ReloadGunEvent { shooter }` (refills the clip + plays reload sound) and
   drop back to Hunt. During Reposition the ped **crouches** (`MovementModifiers.crouch =
   true`) and moves to cover (Phase 6).
3. **Hunt** if `perception.visible` (a live enemy target): engage per weapon (movement in
   Phase 6, attacks in Phase 4-combat below).
4. **Idle** otherwise (no target): stand, play idle clip, optional slow wander (out of
   scope — leave stationary).

The weapon class comes from `EquippedWeapon.0` (`is_gun`/`is_melee`/unarmed). Store nothing
extra; Phase 6/other combat read `AiState` + weapon + perception.

---

## Phase 6 — AI movement / steering (`plugins/pedestrian_ai/movement_ai.rs`)

`ai_movement` writes each ped's `LocomotionInput.move_dir` + `MovementModifiers`
(sprint/crouch) + `AiSteer`, from `AiState`, weapon class, and `AiPerception`. Convert the
chosen world planar dir `d` to the avian input convention `Vec2::new(d.x, -d.z)`.

Ray probes (all drawn as gizmos, Phase 7) use `SpatialQuery::cast_ray` excluding self.

- **Idle:** `move_dir = 0`.
- **Hunt, gun:** keep at a **standoff band** [`GUN_MIN = 12`, `GUN_MAX = 30`] from the
  target. If closer than `GUN_MIN`, back away (dir away from target); if farther than
  `GUN_MAX`, approach; else strafe/hold. When holding, **flank**: cast two horizontal
  probe rays perpendicular to the target direction (left/right, length `FLANK_PROBE = 4`);
  step toward whichever side is **clear** (no hit) while a probe toward the target is
  **blocked** (cover exists) — this seeks a flanking lane around cover. Recompute the
  flank side on `repath_timer` (`FLANK_REPATH = 1.5`) so it doesn't jitter. No sprint
  (aiming), face the target (override `face_movement` intent by nudging `move_dir` toward a
  small strafe; facing toward the enemy for shooting is handled in combat by aiming the
  raycast, not the body — acceptable).
- **Reposition (gun reload):** crouch = true; steer toward the **nearest cover point**:
  cast `COVER_DIRS = 8` rays radially; pick the shortest ray whose hit puts geometry
  **between me and the target** (hit closer than target and roughly opposite the target
  direction); move to just behind that hit point. If no cover, back straight away from the
  target.
- **Hunt, melee:** `sprint = true`, `move_dir` straight at the target. Terrain handling
  reuses the existing controller: forward a `jump`/climb request when blocked — cast a
  short forward ray at knee height; if blocked and a ledge is climbable, set
  `LocomotionInput.jump = true` (the shared `jump_or_climb` logic is player-gated, so for
  AI call a shared `try_climb_or_jump(entity)` helper extracted from `jump_or_climb`, or
  set `jump` and let `movement` jump; climbing over low obstacles falls out of the existing
  `detect_climb` if we invoke it for AI too — expose `detect_climb` as `pub(crate)` and
  call it here). If the forward ray is blocked with no climb, steer around (pick the cl
  earer of two ±45° side probes) — "avoid it".
- **Hunt, unarmed:** same as melee (sprint straight in) but no weapon; the punch is in
  combat.
- **Flee:** send `FLEE_DIRS = 8` rays in a fan of directions (biased away from the target,
  jittered by `repath_timer` `FLEE_REPATH = 0.75`); pick the direction with the **longest
  clear distance** (ray miss = max) and sprint there. Cache in `AiSteer.desired` so the
  path is stable between recomputes.

Constants collected at the top of the module. Movement is **velocity-space only** (writes
`LocomotionInput`, never teleports Transform) — consistent with the project's controller
design and the car-physics rule of thumb in memory.

---

## Phase 7 is combat — split into `plugins/pedestrian_ai/combat.rs`

### 7.1 Directed gun fire for NPCs

Do **not** reuse `fire_gun_observer` (camera-based). In `ai_combat`, when `AiState::Hunt`,
gun equipped, `GunState.rounds > 0`, target visible, and `attack_cooldown <= 0`:

- origin = my muzzle (weapon model `GlobalTransform` via `WeaponModelState.entity`,
  fallback head), dir = `(target_head - origin).normalize` with a small ±`AIM_SPREAD`
  (2–4°) jitter.
- `gun.rounds -= 1`, set `attack_cooldown = SHOT_INTERVAL (0.25)`.
- Cast ray `range = GunInfo.range`, filter excludes self. Push a `ShotTracer` to the
  shared `ShotTracers` resource (pub) and spawn sparks in `BulletSparks`, exactly like
  `fire_gun_observer` does, and trigger the `GunShot`/`BulletImpact` audio events
  (`gunshot_sound_idx` from `GunState`).
- If the hit resolves (via `is_person_entity`) to an entity carrying `Health`, apply
  `GunInfo.damage` (see 7.4). Play the shoot clip: trigger `PedestrianAnimationControlEvent
  { ped: my AiModel, animation: "Pistol_Shoot", speed: 1.0 }` (revert to locomotion clip
  next frame via the anim system, Phase 8).
- Log `info!("[AI {e}] SHOOT -> {target} ({dmg})")`.

Factor the shared raycast+tracer+sparks+audio into a helper
`spawn_shot(commands, spatial, resources.., origin, dir, range, shooter)` reused by both
the player observer and AI (nice-to-have; keeping a second copy in `combat.rs` is
acceptable to avoid churn in `weapon_shooting.rs`).

### 7.2 Melee (sword)

When Hunt + melee + target within `MELEE_RANGE (2.0)` + `attack_cooldown <= 0`: set
`attack_cooldown = SWING_INTERVAL (0.8)`, trigger `PedestrianAnimationControlEvent { ..
"Sword_Attack" }`, trigger the `MeleeWhoosh` audio, and apply `SWORD_DAMAGE (35.0)` to the
**currently targeted enemy's** `Health` directly (per the spec: "hits the currently
targeted enemy", so no separate hit raycast is needed for AI — proximity + facing gates
it). Log `MELEE HIT`.

### 7.3 Unarmed (punch)

Same as melee but `PUNCH_RANGE (1.5)`, `PUNCH_INTERVAL (0.6)`, `PUNCH_DAMAGE (12.0)`, clip
`"Punch_Jab"`/`"Punch_Cross"` (random), `PunchHit` audio.

### 7.4 Damage application & death (`apply_damage_and_death`)

Damage is applied by mutating `Health.current` in-place in 7.1–7.3 (all run in one
`ai_combat` system with a `Query<&mut Health>` param, or via a tiny `DamageEvent { target,
amount }` + observer if borrow conflicts arise — **recommended: a `DamageEvent` +
`apply_damage` observer** so combat systems don't need `&mut Health` on other entities,
avoiding query aliasing). On `current <= 0`:

- Log `info!("[AI {e}] DIED (faction {f})")`.
- Play a death/ragdoll-lite: trigger a `"Death"`/`"Idle_Loop"` clip if present, then
  `despawn` the controller after a short `DespawnTimer` (or despawn immediately for
  simplicity in v1). Despawning the controller removes the model child + weapon.
- Anyone whose `AiPerception.target` pointed at the dead entity re-perceives next tick
  (target becomes `None` → Idle/next enemy).

---

## Phase 8 — AI animation driver (`plugins/pedestrian_ai/anim_ai.rs`)

`ai_animation` picks a **base locomotion clip** per ped from its `LinearVelocity` +
`MovementModifiers` + `AiState`, and triggers `PedestrianAnimationControlEvent { ped:
AiModel, animation, speed }` **only when the clip changes** (store `last_clip` on a small
`AiAnim { last: Option<String> }` component to avoid re-triggering every frame):

- speed < `MOVE_ANIM_THRESHOLD` → `"Idle_Loop"` (or `"Sword_Idle"` if melee equipped),
- crouch → `"Crouch_Fwd_Loop"`/`"Crouch_Idle_Loop"`,
- walk/jog/sprint bands (reuse `WALK_MAX_SPEED`/`JOG_MAX_SPEED`) →
  `"Walk_Loop"`/`"Jog_Fwd_Loop"`/`"Sprint_Loop"`.

One-shot combat clips (shoot/sword/punch) are triggered directly by `ai_combat` (Phase 7)
and naturally get overwritten by the next base-clip change — good enough without the
player's dual-layer blending. `anim` speed scales by `1.0 / CharacterScale` like the player.

---

## Phase 9 — Debug gizmos + UI (`plugins/pedestrian_ai/debug_ui.rs`)

```rust
#[derive(Resource, Default)]
pub struct AiDebug { pub show_rays: bool }
```

- `ai_debug_ui` (EguiPrimaryContextPass): a small `egui::Window "Pedestrian AI"` with a
  `checkbox(&mut ai_debug.show_rays, "Show AI rays")` and a live count of peds per
  faction + alive totals.
- `draw_ai_gizmos` (Update, early-returns unless `show_rays`): draw, per ped, from cached
  data on `AiPerception`/`AiSteer`:
  - LOS ray head→target: **green** if ally/clear, **red** if a visible enemy,
  - flank/cover probe rays (Phase 6): **yellow**,
  - flee fan rays: **cyan**, chosen flee dir highlighted,
  - a small sphere at the ped tinted by `Faction::color`, and an HP bar line above the
    head (scaled by `health.current / max`).

Cache each frame's probe segments in a `pub last_probes: Vec<(Vec3, Vec3, Color)>` field on
a debug component so the gizmo system just replays them (systems that cast the rays record
them when `show_rays` is on).

---

## Phase 10 — `turf_war.rs` binary (`src/bin/turf_war.rs`)

Model on `bin/pedestrian_controller.rs` and `bin/pedestrian_v2.rs`:

```rust
make_basic_app("Turf War")
    .add_plugins(EguiPlugin::default())
    .add_plugins(PhysicsPlugins::default())
    .init_state::<GameControlState>()          // required by controller/weapon plugins
    .insert_resource(MapTree { parsed:true, bbox: big })  // so off-map guards no-op safely
    .add_plugins(PedestriansPlugin)
    .add_plugins(WeaponsPlugin)
    .add_plugins(GameAudioPlugin)              // optional: combat sounds
    .add_plugins(SetupDebugScenePlugin)        // ground + sun (no camera controller)
    .add_plugins(CharacterLocomotionPlugin)    // explicit (AI plugin also guards it)
    .add_plugins(PedestrianAiPlugin)
    .insert_resource(WarMatrix::all_out_war())
    .add_systems(Startup, (setup_overhead_camera, spawn_obstacles))
    .add_systems(Update, spawn_factions_once)  // waits for both manifests loaded
```

- **Overhead camera:** `SetupDebugScenePlugin` already spawns a `Camera3d`; override its
  transform in a `Startup` system that runs after (or spawn our own camera high up looking
  down, e.g. `Transform::from_xyz(0, 55, 40).looking_at(ZERO, Y)`), so the whole arena is
  visible. (Avoid two cameras — either edit the debug camera or don't add
  `SetupDebugScenePlugin`'s camera; simplest: add our own ground + camera + light and skip
  `SetupDebugScenePlugin`, copying its ground/collision-layer setup.)
- **Obstacles** (`spawn_obstacles`): a scatter of **static cubes** (varied sizes,
  `RigidBody::Static`, `Collider::cuboid`, `CollisionLayers::new(Map,[Car,Wheel])` so peds
  collide) as cover, plus a few **prop cars** reusing `spawn_random_cars`-style spawns from
  `pedestrian_controller.rs` (mesh + convex collider, dynamic or static). Lay some cubes as
  low walls to exercise flanking/cover and climbing.
- **Factions** (`spawn_factions_once`, guarded by a `Local<bool>` and
  `PedestrianManifest.loaded && WeaponManifest.loaded`): for each of the 4
  `Faction::COMBATANTS`, place ~4–6 peds clustered in one corner of the arena, each via
  `SpawnAiPedestrianEvent { position, faction, url:None, weapon:None }` (random model +
  random weapon). Spread weapons so guns/melee/unarmed all appear.
- Logging is already emitted by the AI systems (state changes, SHOOT/MELEE/DIED). The bin
  needs no extra logging beyond an initial `info!("Turf war: N peds across 4 factions")`.

---

## Files touched (summary)

**New:**
- `plugins/pedestrian_ai/mod.rs` — `PedestrianAiPlugin`, AI components, `AiState`.
- `plugins/pedestrian_ai/faction.rs` — `Faction`, `WarMatrix`, `Health`.
- `plugins/pedestrian_ai/spawn_ai.rs` — `SpawnAiPedestrianEvent`, spawn + adopt + equip.
- `plugins/pedestrian_ai/perception.rs` — `ai_perception`, LOS.
- `plugins/pedestrian_ai/brain.rs` — `ai_brain` state machine.
- `plugins/pedestrian_ai/movement_ai.rs` — `ai_movement`, flank/cover/flee probes.
- `plugins/pedestrian_ai/combat.rs` — directed fire, melee/punch, `DamageEvent`, death.
- `plugins/pedestrian_ai/anim_ai.rs` — `ai_animation` clip selection.
- `plugins/pedestrian_ai/debug_ui.rs` — `AiDebug`, gizmos, egui window.
- `src/bin/turf_war.rs` — arena test binary.
- `pedestrian_controller_plugin/locomotion.rs` — `CharacterLocomotionPlugin`.

**Edited:**
- `plugins/mod.rs` — register `pub mod pedestrian_ai;`.
- `pedestrian_controller_plugin/mod.rs` — add `LocomotionInput`, split out
  `CharacterLocomotionPlugin`, drop `MovementAction`, guard-add locomotion plugin.
- `pedestrian_controller_plugin/controller.rs` — per-entity `movement`; `character_input`
  and `jump_or_climb` write `LocomotionInput`; expose `detect_climb` as `pub(crate)`.
- `plugins/weapons/weapon_shooting.rs` — `is_person_entity` → `pub(crate)` (reused by AI);
  optionally extract the shared `spawn_shot` helper.
- `main_game_plugin.rs` — `add_plugins(PedestrianAiPlugin)` (locomotion plugin arrives
  guarded via the controller/AI plugins).

---

## Verification

1. `cargo check` / `cargo clippy` on the crate (native).
2. **Phase 0 regression:** run `cargo run --bin pedestrian_controller` — player still
   moves/jumps/climbs/rolls/shoots exactly as before (confirms the `LocomotionInput`
   refactor is behavior-preserving).
3. `cargo run --bin turf_war`:
   - four colored clusters spawn, each ped holding a random weapon; console prints the ped
     count and, as fights start, `state -> Hunt/Reposition/Flee/Idle`, `SHOOT`, `MELEE
     HIT`, `DIED` lines.
   - gunners keep their standoff distance, crouch + fall back to cover when out of ammo,
     then resume; meleers sprint in and climb/vault low cube walls; unarmed peds chase and
     punch; low-HP peds break and sprint away down the longest clear lane.
   - toggling **"Show AI rays"** draws LOS (green/red), flank/cover (yellow), and flee
     (cyan) rays plus faction-tinted markers and HP bars.
   - a faction wiped out stops producing targets; survivors go Idle or fight the remaining
     enemy faction.
4. **Main game:** `add_plugins(PedestrianAiPlugin)` present — spawn the player pedestrian
   and confirm the global-input bug is gone (player movement no longer perturbs AI peds)
   and no double-registration panic for `CharacterLocomotionPlugin`.

## Open decisions (pick during implementation)

- **Death presentation:** instant despawn (v1) vs. brief death clip + delayed despawn.
- **Player as a faction target:** whether the main-game player (a `CharacterController`
  with no `Faction`) should be attackable — add a `Faction` to the player to enable it,
  otherwise AI ignores the player. Default: ignore player in v1.
- **Cars as obstacles:** static prop cars vs. dynamic (dynamic cars get shoved by melee
  rushes — more chaotic/fun but can drift into clusters). Default: a few static + cubes.
