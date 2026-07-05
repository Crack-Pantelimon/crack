# Weapons plugin + weapon-aware combat animations

## Context

The playable pedestrian (kinematic controller + combat animations) now works. We want to give it
**weapons**: a new `WeaponsPlugin` parses a weapon manifest, and in the `pedestrian_controller`
demo the user picks/cycles a weapon that gets attached to the character's right wrist. Combat
animations become **weapon-aware** (punch when unarmed, sword swing when melee, pistol shoot/aim
when armed with a gun). This is mostly a demo-facing feature, but the reusable pieces (manifest,
equip logic, weapon-aware animation) live in library plugins.

Manifest: `_data/3d_data/3d_weapons/out2/manifest.txt`, one relative path per line, e.g.
`gun/ak47.glb`, `melee/machete2.glb`. The first path segment is the class — **`gun` or `melee`**
(note: `gun`, not `guns`). Served at `{DATA_BASE_URL}/3d_data/3d_weapons/out2/<line>`.

**Weapon-local coordinate conventions (put in a doc comment):** grip point is at the origin
`(0,0,0)`. Guns are aimed toward **+X** (barrel along +X). Swords/melee have the blade pointing
straight **up (+Y)**. So `max(x)` of a gun ≈ its length; `max(y)` of a sword ≈ its length.

## Decisions (confirmed with user)
- **Default weapon:** a **random** weapon (gun or melee) is equipped on each character spawn.
- **Demo UI:** grip-offset slider at the top + a **weapon list only** (no character list); keep the
  existing auto-spawn. Mouse wheel cycles weapons.

## Key findings (from exploration)
- Armature bones are real ECS entities with `GlobalTransform`, in the `Children` hierarchy — a weapon
  parented under a bone entity follows the animation automatically.
- `classify_skeleton` (`plugins/pedestrians/skeleton.rs:70`) already returns the **right-wrist**
  entity, but `init_pedestrians_system` (`spawn_pedestrian.rs:217`) **discards it** (keeps only the
  `joint_labels: HashMap<Entity, BoneLabel>`). We will capture it.
- Extents pattern to reuse for weapons: wait until all mesh handles resolve in `Assets<Mesh>`, iterate
  `Mesh::ATTRIBUTE_POSITION`, transform each vertex by the mesh entity's `GlobalTransform` into the
  weapon-root's inverse frame, take min/max (`spawn_pedestrian.rs:124-173`).
- Models are spawned via `WorldAssetRoot(asset_server.load::<WorldAsset>(GltfAssetLabel::Scene(0)
  .from_asset(url)))` (the crate standard; `SceneRoot` is unused). Reuse it for weapons.
- `TextAsset` / `TextAssetLoader` (`plugins/pedestrians/manifest.rs`, both `pub`) are registered by
  `PedestriansPlugin`. Reuse `asset_server.load::<TextAsset>(...)`; do **not** re-register the loader
  (extension `txt` would conflict). `WeaponsPlugin` therefore requires `PedestriansPlugin`.

## New plugin: `src/plugins/weapons/`

### `mod.rs` — `WeaponsPlugin`
Registers `WeaponManifest`, `WeaponGripOffset`, the manifest-load systems, the equip observer, and
the reconcile/finalize systems. Re-exports `WeaponId`, `EquippedWeapon`, `EquipWeaponEvent`,
`WeaponManifest`, `WeaponGripOffset`. Add it to `plugins/mod.rs`. Deps: `plugins::pedestrians`
(skeleton + `TextAsset`).

### `weapon_manifest.rs` — parser + state
- `#[derive(Clone, PartialEq, Eq)] pub enum WeaponId { Unarmed, Melee(String), Gun(String) }` where
  the `String` is the manifest line (`"gun/ak47.glb"`). Helpers: `is_gun/is_melee/is_unarmed`,
  `path() -> Option<&str>`, `label() -> String`.
- `#[derive(Resource, Default)] pub struct WeaponManifest { pub guns: Vec<WeaponId>, pub melee:
  Vec<WeaponId>, pub all: Vec<WeaponId>, pub loaded: bool }` — `all` = `[Unarmed]` + guns + melee.
- Load system (modeled on `manifest.rs::start_manifest_load` + `load_pedestrian_manifest_system`):
  load `{base}/3d_data/3d_weapons/out2/manifest.txt` as `TextAsset`, split lines, classify by the
  first path segment (`gun`→`Gun`, `melee`→`Melee`), populate the resource, set `loaded`.

### `weapon_attach.rs` — equip + placement (bigger functions here)
- `#[derive(Component)] pub struct EquippedWeapon(pub WeaponId)` — logical equipped weapon, set
  immediately so animations react even before the model finishes loading.
- `#[derive(Event)] pub struct EquipWeaponEvent { pub character: Entity, pub weapon: WeaponId }` +
  observer that inserts/updates `EquippedWeapon` on `character`.
- `#[derive(Resource)] pub struct WeaponGripOffset(pub f32)` (default `0.15`, UI-clamped `0.05..=0.5`).
- `#[derive(Component)] struct WeaponModelState { spawned_for: Option<WeaponId>, entity: Option<Entity> }`.
- `reconcile_weapon_model` system: for each character with `EquippedWeapon`, when it differs from
  `WeaponModelState.spawned_for`: despawn the old model entity; if `Unarmed`, done; else find the
  **right-wrist bone** (traverse the character's descendants for `PedestrianSkeleton`, read the stored
  wrist entity) — if the skeleton isn't ready yet, retry next frame; otherwise spawn the weapon
  `WorldAssetRoot` as a `ChildOf(wrist)` with a `PendingWeaponExtents` marker + `WeaponId`, record it.
- `finalize_weapon_placement` system: for weapons with `PendingWeaponExtents`, wait for all meshes to
  load, compute extents (min/max over `Mesh::ATTRIBUTE_POSITION`), store `WeaponExtents { max_x,
  max_y }`, set the local `Transform` (grip offset from `WeaponGripOffset` along the wrist axis),
  remove the marker. Log the extents.

## Shared change: `plugins/pedestrians`
- `skeleton.rs`: add `pub right_hand: Option<Entity>` to `PedestrianSkeleton`.
- `spawn_pedestrian.rs:217`: bind the 7th tuple element (`right_wrist`) from `classify_skeleton` and
  store it in `PedestrianSkeleton { joint_labels, right_hand }`.

## Controller change: `pedestrian_controller_plugin/animation.rs` (weapon-aware combat)
Add `Option<&EquippedWeapon>` to the controller query (default `Unarmed` when absent). Determine kind:
- **Idle base clip:** Melee → `Sword_Idle`; otherwise `Idle_Loop` (unchanged for Unarmed/Gun).
- **Combat overlay** (replaces the current `CombatKind` gun-centric logic):
  - **Unarmed:** LMB → one-shot, randomly `Punch_Jab` or `Punch_Cross` (roll on the press). RMB unused.
  - **Melee:** LMB → one-shot `Sword_Attack`. RMB unused.
  - **Gun:** LMB → one-shot `Pistol_Shoot`; RMB held (no LMB) → loop `Pistol_Idle_Loop` (aim). LMB
    while RMB held → `Pistol_Shoot`.
  Keep the existing one-shot machinery (play from 0, no repeat, revert when `is_finished()`), the
  base-weight ducking while an overlay is active, and the `ManualAnimation` takeover. Re-export
  `ControlledCharacter` from the controller `mod.rs` (`pub use spawn::ControlledCharacter;`) so the
  demo can read the controller entity.

## Demo: `src/bin/pedestrian_controller.rs`
- Add `.add_plugins(WeaponsPlugin)`.
- `WeaponSelection { index: usize }` resource tracking the selected `WeaponManifest.all` index.
- On a **new** controlled character (detect `ControlledCharacter.controller` change) equip a **random**
  weapon: pick a random `all` index, `commands.trigger(EquipWeaponEvent { character, weapon })`.
- **Mouse wheel** (only `in_state(ControllingPedestrian)`, ignored over egui): step `index` ±1 with
  wrap, trigger `EquipWeaponEvent`.
- egui window (`EguiPrimaryContextPass`): grip-offset `Slider(0.05..=0.5)` bound to `WeaponGripOffset`
  at the top, then a scrollable weapon list (`selectable_label` per `all` entry) that sets `index` +
  triggers `EquipWeaponEvent`.

## Critical files
- New: `src/plugins/weapons/{mod,weapon_manifest,weapon_attach}.rs`; register in `plugins/mod.rs`.
- Edit: `src/plugins/pedestrians/{skeleton.rs, spawn_pedestrian.rs}` (store right wrist).
- Edit: `src/plugins/pedestrians/pedestrian_controller_plugin/{animation.rs, mod.rs}` (weapon-aware
  combat + re-export `ControlledCharacter`).
- Edit: `src/bin/pedestrian_controller.rs` (weapons plugin + UI + wheel + random default).
- Reference/reuse: `plugins/pedestrians/manifest.rs` (parser + `TextAsset`), `spawn_pedestrian.rs`
  (extents loop), `map_plugin/map_plugin_ui.rs` (egui list pattern).

## Verification
1. `cargo check -p demo_resolution_selector_web_bevy --all-targets` — clean.
2. Run `./start_pedestrian.sh` (needs the local data server serving `3d_data/3d_weapons/out2/`).
3. Observe:
   - console logs the weapon manifest (gun/melee counts) and each equipped weapon's extents;
   - character auto-spawns with a random weapon attached at the right wrist;
   - mouse wheel cycles weapons; the weapon model swaps (old despawned, new attached); the grip-offset
     slider moves the weapon along the wrist;
   - LMB unarmed → alternating jab/cross; LMB with a melee → sword swing and idle shows `Sword_Idle`;
     LMB with a gun → shoot, RMB-hold → aim loop.
