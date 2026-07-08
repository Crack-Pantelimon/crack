# Plan: Weapon RPM/auto-fire, escape→traffic-ped, camera bbox clamp, endless-horizon fake map

## Context

Five independent gameplay improvements requested for the Bevy driving/pedestrian demo
(`crack_demo/demo_resolution_selector_web_bevy`):

1. **Weapon fire-rate (RPM) debounce** for guns *and* swords, driven by a new `rpm` column in the
   weapon manifest. Today player fire is gated only by `just_pressed` + ammo — there is no rate cap,
   so click-spam fires arbitrarily fast, and sword swings play at a fixed 1× speed.
2. **Automatic weapons**: a new `automatic` manifest flag; automatic weapons fire continuously while
   the mouse button is held, at their RPM. AI is unaffected (AI already self-rate-limits).
3. **Escape while controlling a pedestrian currently despawns it.** Instead, convert it into a
   traffic/AI pedestrian so the world keeps its population.
4. **Camera can wander off the map.** Clamp the camera to the loaded world bbox in the `Last`
   schedule.
5. **Endless horizon ("fake map")**: after the real map loads, render coarse cosmetic-only tiles
   around it (vertices inside the real bbox pushed underground) to fill the horizon.

All facts below were verified against the live code during planning.

---

## Feature 1 + 2: Weapon RPM debounce & automatic fire

### Data: manifest file + parser
- **File `_data/3d_data/3d_weapons/out2/manifest.txt`** already has a header
  (`path,is_gun,clip_size,bullet_type,damage,range`) and the parser already skips it
  ([weapon_impl.rs:39-40](crack_demo/game_logic/src/worker/weapon_impl.rs#L39-L40)) — so "make sure
  the list has a header and it's skipped" is **already true**. The work is to **append two columns**:
  new header `path,is_gun,clip_size,bullet_type,damage,range,rpm,automatic` and add `rpm,automatic`
  to every row. Proposed values (tune freely — it's data):
  - Pistols/revolvers: `rpm` 90–450, `automatic` 0 (semi). e.g. revolvers ~150, glock/1911 ~400.
  - SMGs/rifles: uzi1/uzi2/skorpion1 ~900 `automatic` 1; mp5-mini/mp5-mini-2 ~700 auto 1;
    ak47/draco1/draco2 ~600 auto 1.
  - Melee (`is_gun=0`): `rpm` = swings/min (knife ~140, machete/axe ~90, hammer ~60,
    lightsaber ~110), `automatic` 0.
- **Parse the two new columns** in
  [weapon_impl.rs:50-57](crack_demo/game_logic/src/worker/weapon_impl.rs#L50-L57): `rpm = cols.get(6)`
  (f32, sane default e.g. `300.0`), `automatic = cols.get(7).parse::<u32>() == 1` (default `false`).
- **Add fields** `rpm: f32`, `automatic: bool` to `WeaponEntry`
  ([weapon.rs:1-8](crack_demo/game_logic/src/weapon.rs#L1)).

### In-engine weapon model
- Add `rpm: f32` + `automatic: bool` to `GunInfo`
  ([weapon_manifest.rs:7-14](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_manifest.rs#L7-L14)).
- Melee currently carries only a path (`WeaponId::Melee(String)`). Change it to carry rpm too — add a
  small `MeleeInfo { path: String, rpm: f32 }` and make it `WeaponId::Melee(MeleeInfo)`
  ([weapon_manifest.rs:17-22](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_manifest.rs#L17-L22)).
  Update `path()`, `from_label()`, the `poll_weapon_manifest_task` conversion
  ([weapon_manifest.rs:118-132](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_manifest.rs#L118-L132)),
  and any `WeaponId::Melee(p)` match arms (grep; a handful in `interaction_ui.rs`/`weapon_attach.rs`).
- Add helpers `WeaponId::rpm() -> f32` and `WeaponId::automatic() -> bool` (Unarmed → constants:
  punch ~110 rpm, non-automatic).

### Fire-rate cooldown (guns + swords + punches)
Mirror the AI pattern (`AiCombatTimers.attack_cooldown` gated in
`pedestrian_ai/combat.rs`), but for the player:
- Add a component `WeaponCooldown(pub f32)` (seconds until next allowed attack) on the controller.
- Add a tiny `Update` system in the weapons plugin
  ([weapons/mod.rs](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/mod.rs)) that
  decrements it by `time.delta_secs()` toward 0.
- **Player fire path** lives in `drive_character_animation`
  ([animation.rs:139-357](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L139-L357)).
  Add `Option<&WeaponCooldown>` (and `Commands` already present) to the query. Before firing
  (line ~307 onward): **skip if cooldown > 0**. On a successful attack, set
  `cooldown = 60.0 / weapon.rpm()`.

### Automatic fire while held
- At [animation.rs:139](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L139),
  replace the single `lmb = mouse.just_pressed(Left)` gate with:
  `fire_pressed = !over_ui && (mouse.just_pressed(Left) || (mouse.pressed(Left) && weapon.automatic()))`.
  Combined with the cooldown gate: automatic guns fire continuously at RPM while held; semi weapons
  and all melee require a fresh click but are still capped at RPM. AI path (`combat.rs`) is untouched.

### Sword animation speed-up to match RPM
- The player one-shot melee clip (`Sword_Attack`) is played at 1× at
  [animation.rs:385](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L385)
  (`player.play(n)` with no `.set_speed`). Compute `swing_secs = 60.0 / rpm` and set
  `active.set_speed(natural_swing_secs / swing_secs)` so the swing visually completes within the
  fire interval. Use the AI baseline `SWING_INTERVAL = 0.8s` (`pedestrian_ai/combat.rs:32`) as
  `natural_swing_secs` (i.e. speed = `rpm / 75.0`), clamped to a sane range (e.g. 0.5..=4.0).
- Shorten the melee **hit delay** to match: the `PendingMeleeHit { timer: 0.25, .. }` at
  [animation.rs:326](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L326)
  should scale with the sped-up swing (e.g. `0.25 * (natural_swing_secs / swing_secs).recip()`, or
  simply `swing_secs * 0.4`) so damage lands mid-swing.

---

## Feature 3: Escape converts controlled pedestrian → traffic/AI pedestrian

**Decision (confirmed): traffic ped with plain-AI fallback.**

Rewrite `escape_to_freecam`
([spawn.rs:210-241](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs#L210-L241)).
Instead of `commands.entity(controller).despawn()`, **convert in place** (mirroring the canonical
`eject_driver_as_ai` at
[interaction_ui.rs:404-470](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L404-L470)):

1. On the **model entity** (`controlled.ped`): `commands.entity(ped).remove::<ManualAnimation>()`
   so the shared AI animation system takes over.
2. On the **controller entity**, insert the AI component set (same as
   `pedestrian_ai/spawn_ai.rs:74-85`): `AiPedestrian, AiState::Idle, AiPerception::default(),
   AiCombatTimers::default(), AiSteer::default(), AiAnim::default(), AiThink::default(),
   Enemies::default()`, plus `AiModel(controlled.ped)`. Keep the existing `Health`, `Faction`,
   `LocomotionInput`, and physics bundle (player and AI share `character_physics_bundle`, so **no
   physics components are removed**).
3. **Traffic path**: add `Res<TrafficRoadGraph>` to the system. Get the controller's world position,
   call `build_path_from(&graph, pos)`
   ([traffic/common.rs](crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/common.rs)).
   If `Some((seg, path))`, insert
   `TrafficPedestrian { state: TrafficAgentState::new(path, seg), offset_sign: ±1 (random), last_pos: pos }`.
   If `None` (not near a road), skip it → the entity remains a plain wandering AI ped.
   (We insert `TrafficPedestrian` directly, and `adopt_traffic_pedestrians`'
   `Without<TrafficPedestrian>` filter prevents double-adoption.)
4. Clear `ControlledCharacter` fields **without despawning** (`controller`/`ped`/`scale_node = None`,
   `awaiting = false`) and `next_state.set(GameControlState::MapFreecam)`.

Keep the existing `capture_state.is_captured` early-return
([spawn.rs:231-233](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs#L231))
so the two-press (release-cursor-then-exit) timing is unchanged.

---

## Feature 4: Clamp camera to world bbox in `Last`

All three camera drivers (freecam `camera_controls.rs`, pedestrian `follow_camera`, car
`camera_follows_car`) write the single `Camera3d` `Transform` in the `Update` schedule. Add one
clamp system that runs after them.

- New system `clamp_camera_to_map_bbox(map_tree: Option<Res<MapTree>>, mut cam: Query<&mut Transform, With<Camera3d>>)`.
  Gate on `map_tree.parsed`
  ([map_plugin/mod.rs:52-57](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/mod.rs#L52-L57));
  `MapTree.bbox` is already in Bevy world coords (used directly as ray origins elsewhere).
- Clamp **X and Z only**: `t.translation.x = clamp(x, bbox.min.x, bbox.max.x)` and same for `z`.
  Leave `Y` free (freecam self-manages height vs ground; clamping Y would fight it). Optionally cap
  `y` to `<= bbox.max.y + headroom`.
- Register in `MapPlugin::build`
  ([map_plugin/mod.rs:20-44](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/mod.rs#L20)):
  `app.add_systems(Last, clamp_camera_to_map_bbox);` (`Last` is currently unused in the crate; this
  is the intended single choke-point after all `Update` camera writes and transform propagation).

**Camera clamp (4)**
- `.../plugins/map_plugin/mod.rs` — `clamp_camera_to_map_bbox` in `Last`
---

## Feature 5: Endless-horizon fake map

**Architecture note:** the client only receives the trimmed `MapManifestResult { bbox, roots,
lod_budget }`; the full tree is worker-side only, and coarse tiles (octree `depth < 14`) are
currently **discarded** at
[manifest_impl.rs:207](crack_demo/game_logic/src/worker/manifest_impl.rs#L207). Those discarded
coarse tiles — which have their own `glb_path` + `bbox` in the parquet — are exactly the
"toward-the-root" horizon tiles. So this feature needs a **new worker API** to expose them.

### Worker side
- In `build_map_tree` ([manifest_impl.rs:193](crack_demo/game_logic/src/worker/manifest_impl.rs#L193)),
  also collect the parsed nodes with `depth < 14` **that have a `glb_path`** into a new
  `MapTreeData.coarse_assets: Vec<MapTreeAssetInfo>` (keyed/sorted by original octree depth =
  `octant_path.len()`).
- Add a new API method (register alongside `FetchMapManifest`): `FetchFakeMapTiles` →
  `Vec<FakeMapTile { octant_path: String, glb_path: String, bbox: BBox, depth: i32 }>`, built from
  `coarse_assets` via the cached `MapTreeData`. Wire it through `api.rs`, `worker/mod.rs`, and the
  worker→client plumbing the same way `FetchMapManifest`/`FetchMapTile` are.
  (Representative files: [api.rs](crack_demo/game_logic/src/api.rs),
  [worker/tile_impl.rs](crack_demo/game_logic/src/worker/tile_impl.rs) as the fetch-shape reference.)

### Client side (new module, e.g. `map_plugin/fake_map.rs`)
Trigger on `OnEnter(InitialMapLoadFinished::Finished)`
([states/mod.rs:3-8](crack_demo/demo_resolution_selector_web_bevy/src/plugins/states/mod.rs#L3),
set by `check_map_loaded_status` at
[map_lod.rs:597-625](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs#L597)):

1. **Level selection** (matches the request): fetch coarse tiles; group by octree depth. Starting
   from the coarsest depth just above the real roots, walk **toward the root** (decreasing depth)
   until a level with **≤ 4 tiles**; that's the first horizon ring.
2. **Spawn cosmetic tiles**: reuse the tile-GLB fetch (`FetchMapTile` /
   `poll_tile_group_fetches` machinery) to load each tile's GLB, but spawn a **cosmetic bundle** —
   strip everything physical relative to the normal tile spawn
   ([map_lod.rs:34-59](crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs#L34-L59)):
   keep `WorldAssetRoot(handle)`, `Visibility`, an identity `Transform::from_xyz(0,0,0)` (Bevy needs
   a transform for the scene root; matches how real tiles place themselves), and a new
   `CosmeticMapTile` marker. **Drop** `RigidBody`, `CollisionMargin`, `Restitution`, `Friction`,
   `CollisionLayers`, and the `Collider`. Add `NotShadowCaster`/`NotShadowReceiver` to child meshes.
3. **Vertex lowering**: after the GLB scene spawns its mesh children, run a
   `PendingCosmeticVertexLower { bbox }` system that mirrors the mesh-read pattern in
   `init_cars_system`
   ([spawn_car.rs:228-304](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs#L228-L304)):
   walk `Children`, get each `Mesh3d`, **clone the mesh** (`meshes.get(h).cloned()` — meshes are
   shared `Assets<Mesh>` handles, so mutating in place would corrupt other tiles), and for every
   vertex whose world **XZ** falls inside the running bbox, set its **Y** (Blender is Y-up) to
   `bbox.min.y - 1.0`. `meshes.add(cloned)` and swap the child's `Mesh3d`.
4. **Expand & repeat**: after a ring is placed, expand the running bbox to the union of the placed
   tiles' bboxes, then repeat steps 1–3 for **up to 3 more coarser levels** (or until coarse levels
   are exhausted), so each ring's overlap with the previous (now-larger) bbox is sunk underground and
   only the outer fringe shows — producing the endless-horizon silhouette.
**Fake map (5)**
- `crack_demo/game_logic/src/map.rs` — `MapTreeData.coarse_assets`, `FakeMapTile`
- `crack_demo/game_logic/src/worker/manifest_impl.rs` — collect coarse assets
- `crack_demo/game_logic/src/api.rs` + `worker/mod.rs` — `FetchFakeMapTiles` API
- `.../plugins/map_plugin/fake_map.rs` (new) + register in `MapPlugin`

---

## Files to touch (summary)

**Weapons (1+2)**
- `_data/3d_data/3d_weapons/out2/manifest.txt` — add `rpm,automatic` header cols + row values
- `crack_demo/game_logic/src/weapon.rs` — `WeaponEntry.rpm/.automatic`
- `crack_demo/game_logic/src/worker/weapon_impl.rs` — parse cols 6/7
- `.../plugins/weapons/weapon_manifest.rs` — `GunInfo.rpm/.automatic`, `MeleeInfo`, helpers, conversion
- `.../plugins/weapons/mod.rs` — `WeaponCooldown` decrement system
- `.../pedestrians/pedestrian_controller_plugin/animation.rs` — cooldown gate, auto-fire, sword speed
- (grep) `WeaponId::Melee(_)` match arms in `interaction_ui.rs`, `weapon_attach.rs`

**Escape→ped (3)**
- `.../pedestrians/pedestrian_controller_plugin/spawn.rs` — rewrite `escape_to_freecam`



---

## Verification

Build: `cargo check -p demo_resolution_selector_web_bevy` (and `-p game_logic`). Then run the app
(via the `/run` skill or the project's native run path) and check each feature:

1. **RPM**: equip a semi pistol, hold-click fast → capped rate. Equip a sword → swing animation is
   visibly faster and damage lands mid-swing; can't spam faster than its RPM.
2. **Automatic**: equip the uzi, hold LMB → continuous fire at its RPM; a pistol does not auto-fire.
3. **Escape→ped**: control a pedestrian on a street, press Escape (twice, per the capture gotcha) →
   the pedestrian is **not** despawned; it remains and starts wandering/following the road as traffic.
   Off-road, it remains as a plain wandering AI.
4. **Camera clamp**: in freecam, fly toward the map edge → camera stops at the bbox in X/Z, vertical
   movement still works.
5. **Fake map**: after load, look toward the horizon → coarse tiles fill the distance with no
   collision (drive/walk through them; they're cosmetic) and no visible seam where they meet the real
   map (overlap sunk underground). No shadow artifacts.
