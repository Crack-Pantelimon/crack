# Improvement Review v1

Review of the code analysed while fixing the July batch (clouds, aim-facing, car weapon
wheel, BVH minimap), plus future improvement steps. Written 2026-07-10.

## What was changed in this batch

### 1. Cloud VFX not visible on the main map
`setup_clouds` spawned the 10 km cloud quad at a hard-coded world `y = 120` at `Startup`,
before the map manifest exists. The real map bbox sits at whatever elevation the ENU
projection produces, so on the main map the plane ended up below the terrain (or hopelessly
off-center) and was never seen. New `position_clouds_over_map` system
(`visual_fx/clouds.rs`) re-anchors the plane when `MapTree` parses: centered on the map
bbox, `CLOUD_HEIGHT_ABOVE_MAP` (150 m) above the bbox *top*, and scales the quad to twice
the map extent. Demo/sim binaries without `MapTree` keep the old placement.

### 2. Player body not facing the aim/shoot direction
Two compounding bugs:

- `face_aim` (controller.rs) only ran while RMB was held and *slerped* the yaw at
  `TURN_SPEED`, so un-aimed LMB shots never squared the body and aimed shots lagged the
  crosshair. It now runs while aiming **or** while a combat overlay is active
  (`CombatState.kind != None`) and **snaps** the yaw (no slerp).
- `apply_arm_ik` (arm_ik.rs) used `rotation * Vec3::NEG_Z` as character forward, while the
  controller convention everywhere else (`face_movement`, `face_aim`) is model-forward =
  `+Z`. The spine "compensation" (`torso_yaw_toward`) therefore saw the target ~180° behind
  the character, computed a huge excess yaw, and twisted the torso sideways — the past
  "spine realignment that didn't work out", and the sideways pose in the screenshot. Both
  branches (on foot, driving) now use `+Z`.

Left as `TODO` (documented on `face_aim`): true upper/lower body decoupling — aim side
(chest/head/arms) tracks the crosshair, locomotion side (hips/legs) follows movement. For
now the whole mesh snaps.

### 3. Car weapon wheel cycles through non-guns
`driving_weapon_wheel` (interaction_ui.rs) indexed the full `WeaponManifest.all` (unarmed,
melee, guns). It now cycles through gun entries only; if the driver currently has a
non-gun equipped, the first scroll snaps to the first/last gun depending on direction. If
the manifest has no guns, scrolling does nothing.

### 4. BVH occluder observability — Debug > 3D BVH Minimap
New `map_plugin/bvh_minimap.rs`: an egui corner window with a 3D wireframe view of every
spawned map tile's cubic bbox, projected from a virtual camera high above the map (whole
bbox always in frame). Colors encode LOD state:

| Color  | State |
|--------|-------|
| green  | active (visible) |
| yellow | loaded, pending reveal |
| orange | split in flight (children fetching/loading) |
| purple | merge in flight (parent fetching/loading) |
| red    | about to be dropped by a pending reveal |

The main camera is drawn as a white dot with a heading tick. A **"BVH occluder
(visibility cull)"** checkbox (default on — set from the manifest in
`poll_manifest_task`) toggles `MapLODState.enable_visibility_cull`; the flag is now part
of `spawn_lod_task`'s change-detection key (`lod_flow.rs`), so flipping it forces a fresh
LOD recompute immediately. Opening the minimap also opens the LOD configurator so the
split/merge churn can be watched while moving.

**Deliberate design choice:** the minimap is *not* a second `Camera3d` with a viewport.
~15 gameplay systems do `Query<..., With<Camera3d>>::single()` (follow camera, character
input, weapon transforms, LOD flow, traffic…); a second real camera makes every one of
them silently bail. The boxes are hand-projected and painted with the egui painter
instead — zero render-graph interaction.

## Review of the code analysed

### Occluder / LOD pipeline (`game_logic/visibility.rs`, `lod.rs`, `lod_flow.rs`)
- The occluder world is rebuilt worker-side from 64x64 heightfields per tile. Reasonable,
  but there is **no client-side signal of what got culled** — the client only sees fewer
  split requests. The minimap shows spawned-tile states, but a culled *candidate* never
  appears at all. See improvement 1 below.
- `is_node_visible` casts up to `cameras × 16 samples × 9 corners` rays per node. The
  Fibonacci-sphere "camera drift" sampling is clever, but ray count grows quickly with
  the number of camera references; there is no time budget or early-out ordering
  (e.g. test the center point first — it does test corners in fixed order).
- `spawn_lod_task`'s change key (nodes, quantized refs, budget, cull flag) omits
  `max_lod` and `tiles_per_diagonal` — moving those sliders only takes effect after the
  camera moves 2 m. Same class of bug as the cull-flag one just fixed.
- `HEIGHTMAP_CACHE` is never evicted; on a long session over a big map this grows
  unboundedly (64x64 f32 ≈ 16 KB per node, plus parry structures).

### Map tile flow (`map_lod.rs`)
- The split/merge machinery uses five entity-component "queues" (`PendingTileGroupFetch`,
  `TileShouldSplit`, `TileShouldMerge`, `PendingTileReveal`, plus `TileSwapRequests`).
  It works, but state is smeared across components and the LOD task refuses to run while
  *any* of them is non-empty — one slow tile fetch stalls all LOD progress globally.
- Tile world AABBs are not stored on `TreeMapTile`; the minimap has to reconstruct them
  from mesh `Aabb`s. The server knows the exact node bbox (`MapTreeNodeInfo.bbox`) —
  shipping it with the split/merge summaries (or in the root manifest) would remove the
  reconstruction and let the minimap draw *fetching* tiles before their meshes exist.

### Character controller / combat (`pedestrian_controller_plugin`)
- Forward-axis convention (+Z) is implicit and was already violated once (arm_ik). Worth
  a single `pub const MODEL_FORWARD: Vec3` (or `Dir3`) next to `MODEL_FORWARD_OFFSET`
  that every consumer uses.
- `face_movement` and `face_aim` both run in `Update` while `move_and_slide` runs in
  `FixedUpdate`; rotation is written on the physics body outside the physics step. Fine
  for a kinematic capsule, but worth revisiting if rotation ever affects collision.
- `interaction_ui.rs` is ~1400 lines mixing car interaction, weapon wheels, HUDs,
  crosshairs, and driver-mesh lifecycle. The on-foot and driving weapon wheels are
  near-duplicates (reader loop, debounce, over-UI check) — extract a
  `read_scroll_step(&mut wheel, &mut contexts) -> i32` helper and a shared cycle fn.

### Clouds / VFX
- `sync_cloud_uniforms` + `position_clouds_over_map` both react to change detection;
  clouds are still a single scrolling-noise quad. Good enough, but the quad is scaled up
  to 2× map extent while the noise frequency (`cloud_scale`) is world-space, so nothing
  changes visually with scale — correct, just non-obvious (documented by constants now).
- `VfxSettings` is one flat struct consumed by everything; per-category sub-structs would
  cut the `is_changed()` fan-out.

## Future improvement steps (prioritized)

1. **Culled-tile feedback in the minimap.** Return the set of nodes rejected by the
   visibility gate in `LodComputeResponse` (paths + bboxes) and draw them dark-blue in
   the minimap. That directly answers "is the BVH occluder working" — today it can only
   be inferred from split behavior.
2. **Add `max_lod` / `tiles_per_diagonal` to the LOD change key** so slider changes
   recompute immediately (one-line each, same pattern as the cull flag).
3. **Spine/hip decoupling** (the TODO): drive a chest/spine yaw offset post-animation
   (same slot as `apply_arm_ik`, which already rotates the spine bone) from the delta
   between aim yaw and movement yaw, clamped to ±60°, instead of snapping the whole
   controller. The `PedestrianSkeleton` classification already exposes the spine chain.
4. **Bound the occluder cost:** cap total rays per `compute_lod_changes` call and evict
   `HEIGHTMAP_CACHE` entries for nodes absent from the manifest-reachable set (or LRU).
5. **Ship node bboxes to the client** with split/merge summaries; store on
   `TreeMapTile`. Simplifies the minimap, enables client-side frustum debug, and removes
   the per-entity AABB reconstruction cache.
6. **Unstall the LOD loop:** allow `spawn_lod_task` to run when queues are non-empty but
   stale (e.g. per-request timeout), so one hung fetch doesn't freeze LOD adaptation.
7. **Weapon wheel unification:** shared gun-filtering + scroll-step helper for the
   on-foot and driving wheels; consider also filtering the on-foot wheel HUD order so
   guns group together.
8. **Camera identity:** introduce a `MainCamera` marker component and migrate the ~15
   `With<Camera3d>` `single()` queries to it. That unlocks real picture-in-picture
   cameras (a true rendered minimap, kill-cam, mirrors) without breaking gameplay
   systems.
