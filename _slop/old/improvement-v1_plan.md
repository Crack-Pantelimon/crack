# Plan: Future Improvement Steps (v1)

This document outlines the detailed implementation plan to address the prioritized improvement steps for the map LOD, pedestrian combat/locomotion, and camera systems.

---

## 1. Culled-Tile Feedback in the Minimap

### Problem Statement
When the BVH occluder culls nodes (visibility gate), the client has no visual feedback on what was rejected. The candidate split node is simply omitted, leaving it indistinguishable from budget-cull or other LOD constraints.

### Proposed Technical Solution
Collect nodes rejected by the visibility gate on the worker side during LOD recomputation, return them in the `LodComputeResponse`, and draw them in dark-blue on the egui 3D BVH Minimap.

### Detailed Changes

#### [game_logic/src/lod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/game_logic/src/lod.rs)
- Define a new serializable structure:
  ```rust
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct CulledNodeSummary {
      pub path: MapTreeNodePath,
      pub bbox: BBox,
  }
  ```
- Add `pub culled_nodes: Vec<CulledNodeSummary>` to `LodComputeResponse`.
- Inside `compute_lod_changes`:
  - When evaluating candidate splits, if a node satisfies the budget and split requirements (`new_budget <= budget as usize && is_valid_split(&node_path)`) but fails the visibility check (`!is_visible`), insert it into `culled_nodes`:
    ```rust
    if new_budget <= budget as usize && is_valid_split(&node_path) && !is_visible {
        culled_nodes.push(CulledNodeSummary {
            path: node_path.clone(),
            bbox: tile_bbox(&node_path),
        });
    }
    ```

#### [demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs)
- Add `pub culled_nodes: Vec<game_logic::lod::CulledNodeSummary>` to `TileSwapRequests`.

#### [demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs)
- In `poll_lod_task`, copy `response.culled_nodes` to `res_tiles.culled_nodes`.

#### [demo_resolution_selector_web_bevy/src/plugins/map_plugin/bvh_minimap.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/bvh_minimap.rs)
- Define a new color constant:
  ```rust
  const COLOR_CULLED: egui::Color32 = egui::Color32::from_rgb(0, 80, 220); // Dark Blue
  ```
- In `bvh_minimap_window`, accept `res_tiles: Res<TileSwapRequests>` or `ResMut<TileSwapRequests>`.
- In the legend UI, add `culled` count:
  ```rust
  ("culled", COLOR_CULLED, res_tiles.culled_nodes.len())
  ```
- Render the culled bounding boxes as dark-blue wireframes:
  ```rust
  for culled in &res_tiles.culled_nodes {
      view.wire_box(&painter, culled.bbox.min, culled.bbox.max, COLOR_CULLED);
  }
  ```

---

## 2. Add `max_lod` / `tiles_per_diagonal` to the LOD Change Key

### Problem Statement
Changing the `max_lod` or `tiles_per_diagonal` sliders in the UI does not trigger an immediate LOD recompute. The change only registers after the camera moves at least 2 meters (satisfying the drift change-detection thresholds).

### Proposed Technical Solution
Extend `spawn_lod_task`'s local state `last` tuple to include `max_lod` and `tiles_per_diagonal` (as bits).

### Detailed Changes

#### [demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs)
- In `spawn_lod_task`, update the local state variable type:
  ```rust
  mut last: Local<Option<(BTreeSet<MapTreeNodePath>, Vec<Vec3>, u32, bool, i32, u32)>>
  ```
- Extract the slider values:
  ```rust
  let max_lod = lod_state.max_lod;
  let tiles_per_diagonal_bits = lod_state.tiles_per_diagonal.to_bits();
  ```
- Update the change-detection comparison:
  ```rust
  if let Some(last_val) = &*last {
      if nodes == last_val.0
          && quantized_refs == last_val.1
          && budget == last_val.2
          && cull == last_val.3
          && max_lod == last_val.4
          && tiles_per_diagonal_bits == last_val.5
      {
          return;
      }
  }
  *last = Some((nodes.clone(), quantized_refs, budget, cull, max_lod, tiles_per_diagonal_bits));
  ```

---

## 3. Spine/Hip Decoupling

### Problem Statement
Aiming currently snaps the whole character body (hips/legs/capsule) to face the crosshair. This looks rigid during strafing or lateral movement. We want to decouple hips and chest: legs/hips follow the movement direction, while the upper body/spine turns to face the aim direction, clamped to ±60°.

### Proposed Technical Solution
Update `face_aim` to only rotate the controller capsule if the aim yaw deviates from the current capsule rotation by more than 60°. Use the pre-existing procedural spine compensation in `apply_arm_ik` to rotate the spine bone within the 60° limit.

### Detailed Changes

#### [demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs)
- In `face_aim`, instead of snapping `transform.rotation` directly to `aim_yaw + AIM_BODY_YAW_OFFSET`:
  ```rust
  let current_yaw = transform.rotation.to_euler(EulerRot::YXZ).0;
  let target_yaw = aim_yaw + AIM_BODY_YAW_OFFSET;
  let mut delta_yaw = target_yaw - current_yaw;
  
  // Normalize delta_yaw to [-PI, PI]
  while delta_yaw > std::f32::consts::PI {
      delta_yaw -= 2.0 * std::f32::consts::PI;
  }
  while delta_yaw < -std::f32::consts::PI {
      delta_yaw += 2.0 * std::f32::consts::PI;
  }
  
  // If the aiming angle exceeds the comfortable spine limit (60°),
  // rotate the controller capsule just enough to keep it at the limit.
  let limit = 60.0f32.to_radians();
  if delta_yaw.abs() > limit {
      let correction = delta_yaw.signum() * (delta_yaw.abs() - limit);
      transform.rotation = Quat::from_rotation_y(current_yaw + correction);
  }
  // If delta_yaw is within 60°, we leave the capsule alone. Hips remain aligned to movement.
  ```

#### [demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs)
- Verify `apply_arm_ik`'s spine compensation remains active: it calculates the torso rotation offset with `torso_yaw_toward(char_forward, to_target, 60f32.to_radians())` and writes it to the spine bone, aligning the upper chest to the crosshair.

---

## 4. Bound the Occluder Cost

### Problem Statement
Raycasting culls can grow highly expensive. `is_node_visible` casts up to `cameras * 16 samples * 9 corners` rays per node. Additionally, the global `HEIGHTMAP_CACHE` never evicts, causing unbound memory growth over large map coordinates.

### Proposed Technical Solution
Cap the total rays cast per `compute_lod_changes` call, and prune `HEIGHTMAP_CACHE` based on LRU access times and manifest membership.

### Detailed Changes

#### [game_logic/src/visibility.rs](file:///home/p/VIDOEGAME/crack/crack_demo/game_logic/src/visibility.rs)
- Update `HEIGHTMAP_CACHE` value type to store access time:
  ```rust
  pub struct CachedHeightfield {
      pub hf: HeightField,
      pub last_access_ms: i64,
  }
  pub static HEIGHTMAP_CACHE: RwLock<Option<HashMap<MapTreeNodePath, CachedHeightfield>>> = RwLock::const_new(None);
  ```
- When looking up cached heightfields in `rebuild_bvh`:
  - If a hit is found, update `entry.last_access_ms = _crack_utils::get_timestamp_now_ms()`.
  - When inserting a newly constructed heightfield, enforce a size limit (e.g. 512 entries). If exceeded, evict the entry with the oldest `last_access_ms`.
  - If the map manifest cache is loaded, retain only cache entries that exist in `manifest.all_nodes`:
    ```rust
    if let Some(ref manifest) = manifest {
        hm_cache.retain(|path, _| manifest.all_nodes.contains_key(path));
    }
    ```
- Add a mutable `ray_counter: &mut usize` parameter to `is_node_visible` and `is_ray_occluded`.
- In `is_ray_occluded`, increment `*ray_counter` on call.
- In `is_node_visible`, if `*ray_counter >= ray_limit` (e.g., 2048 rays), immediately early-out returning `true` (fail-safe visible) to prevent performance stalls.

#### [game_logic/src/lod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/game_logic/src/lod.rs)
- Instantiate a `let mut ray_counter = 0;` at the beginning of `compute_lod_changes` and pass a mutable reference down into `is_node_visible`.

---

## 5. Ship Node BBoxes to the Client

### Problem Statement
The Bevy client does not know the exact bounding boxes of octree nodes. The `3D BVH Minimap` must wait until a tile's scene instantiates to query its mesh `Aabb`s. This prevents drawing tiles that are currently fetching or loading.

### Proposed Technical Solution
Ship node bounding boxes (`BBox`) along with the split and merge summary payloads returned by the worker. Save these bboxes on the client's `TreeMapTile` component at spawn.

### Detailed Changes

#### [game_logic/src/map.rs](file:///home/p/VIDOEGAME/crack/crack_demo/game_logic/src/map.rs)
- Add `pub bbox: BBox` to `MapRootNodeSummary` and `MapTileAssetInfoSummary`.

#### [game_logic/src/lod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/game_logic/src/lod.rs)
- Add `pub bbox: BBox` to `MergeRequestSummary`.
- In `compute_lod_changes`, fill in the `bbox` fields of summaries using node data retrieved from `data_res.all_nodes`.

#### [demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs)
- Add `pub bbox: game_logic::map::BBox` to `TreeMapTile`.
- Update `spawn_node_tiles` and calls to pass the `bbox` and write it onto the `TreeMapTile` component at entity spawn.

#### [demo_resolution_selector_web_bevy/src/plugins/map_plugin/bvh_minimap.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/bvh_minimap.rs)
- Simplify `bvh_minimap_window`: read the bounding box directly from `TreeMapTile::bbox`.
- Completely remove `compute_tile_aabb`, `aabb_cache`, and references to mesh child traversal. This enables immediate wireframe drawing of fetching tiles.

---

## 6. Unstall the LOD Loop

### Problem Statement
1. If a tile fetch hangs, it sits in `PendingTileGroupFetch` indefinitely, filling up budget slots and blocking future splits.
2. In `do_split_requests` and `do_merge_requests`, if Bevy's `AssetServer` fails to load a tile's GLB asset, the request is logged but never despawned. It stalls the LOD flow permanently.

### Proposed Technical Solution
Introduce a 10-second request timeout on `PendingTileGroupFetch` components. Ensure failed asset loading requests despawn immediately.

### Detailed Changes

#### [demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs)
- Add a `start_time: f32` field to `PendingTileGroupFetch`. Populate it with `time.elapsed_secs()` at spawn.
- In `poll_tile_group_fetches`:
  - Read `Time` resource.
  - If `time.elapsed_secs() - fetch.start_time > 10.0`, log a warning and despawn the entity:
    ```rust
    if time.elapsed_secs() - fetch.start_time > 10.0 {
        tracing::warn!("Timeout fetching tile group {:?}, despawning.", fetch.purpose);
        commands.entity(entity).despawn();
        continue;
    }
    ```
- In `do_split_requests` and `do_merge_requests`, if `!asset_errors.is_empty()`, despawn the request entity:
  ```rust
  if !asset_errors.is_empty() {
      // log errors...
      commands.entity(req_ent).despawn(); // Unstalls loop on failure
  }
  ```

---

## 7. Weapon Wheel Unification

### Problem Statement
On-foot and driving weapon wheels have duplicate input polling, debouncing, and UI hover check logic.

### Proposed Technical Solution
Extract shared helpers for scroll steps and index cycling.

### Detailed Changes

#### [demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs)
- Extract a scroll step helper:
  ```rust
  fn read_scroll_step(
      time: &Time,
      next_switch: &mut f32,
      wheel: &mut MessageReader<MouseWheel>,
      contexts: &mut EguiContexts,
  ) -> Option<i32> {
      let over_ui = contexts
          .ctx_mut()
          .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
          .unwrap_or(false);
      if over_ui {
          wheel.clear();
          return None;
      }
      let mut step = 0i32;
      for ev in wheel.read() {
          if ev.y > 0.0 {
              step += 1;
          } else if ev.y < 0.0 {
              step -= 1;
          }
      }
      let step = step.signum();
      if step == 0 {
          return None;
      }
      let now = time.elapsed_secs();
      if now < *next_switch {
          return None;
      }
      *next_switch = now + 0.15;
      Some(step)
  }
  ```
- Extract index cycling:
  ```rust
  fn cycle_weapon(
      manifest: &WeaponManifest,
      current_index: usize,
      step: i32,
      guns_only: bool,
  ) -> usize {
      if guns_only {
          let gun_indices: Vec<usize> = manifest
              .all
              .iter()
              .enumerate()
              .filter(|(_, w)| w.is_gun())
              .map(|(i, _)| i)
              .collect();
          if gun_indices.is_empty() {
              return current_index;
          }
          let n = gun_indices.len() as i32;
          let next_pos = match gun_indices.iter().position(|&i| i == current_index) {
              Some(pos) => (((pos as i32 + step) % n + n) % n) as usize,
              None => if step > 0 { 0 } else { gun_indices.len() - 1 },
          };
          gun_indices[next_pos]
      } else {
          let n = manifest.all.len() as i32;
          if n == 0 {
              return current_index;
          }
          (((current_index as i32 + step) % n + n) % n) as usize
      }
  }
  ```
- Refactor `weapon_wheel` and `driving_weapon_wheel` to leverage these shared helpers.

---

## 8. Camera Identity

### Problem Statement
systems select cameras using `Query<..., With<Camera3d>>::single()`. If a secondary camera is spawned (e.g. for mirrors, picture-in-picture, or rendering target minimaps), these systems will panic or silently fail because `single()` expects exactly one entity.

### Proposed Technical Solution
Define a `MainCamera` marker component. Apply it to the primary game camera, and migrate all systems targeting the primary viewport to query `With<MainCamera>` instead of just `With<Camera3d>`.

### Detailed Changes

#### [demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs)
- Define the marker component:
  ```rust
  #[derive(Component)]
  pub struct MainCamera;
  ```

#### [demo_resolution_selector_web_bevy/src/plugins/main_scene_plugin.rs](file:///home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/main_scene_plugin.rs)
- Attach the `MainCamera` component when spawning the default camera:
  ```rust
  commands.spawn((
      Transform::from_xyz(0.0, 10.5, -30.0).looking_at(Vec3::ZERO, Vec3::Y),
      Camera { .. },
      Camera3d::default(),
      MainCamera, // <--- Add marker
      // ...
  ));
  ```

#### Migration
Update all queries in files referencing `With<Camera3d>` to select `With<MainCamera>`:
- `lod_flow.rs`
- `bvh_minimap.rs`
- `weapon_shooting.rs`
- `weapon_attach.rs`
- `traffic/spawn.rs`, `pedestrian_traffic.rs`, `despawn.rs`, `debug_ui.rs`
- `interaction_ui.rs`
- `controller.rs`
- `camera.rs`
- `audio/mod.rs`
- `camera_follow.rs`
- `multiplayer_plugin.rs`

---

## Verification Plan

### Automated Tests
- Run compilation checks across targets:
  - `cargo check --workspace`
  - `cargo check --package game_logic --features worker`
- Execute unit tests in `game_logic`:
  - `cargo test --package game_logic`

### Manual Verification
- Launch the Bevy client and test:
  1. Open **Debug > 3D BVH Minimap** window. Verify culled nodes render in dark-blue.
  2. Modify `max_lod` and `tiles_per_diagonal` sliders. Confirm LOD adapts instantly.
  3. Aim and strafe on foot. Confirm leg orientation follows movement while torso follows aim (clamped to 60°).
  4. Cycle weapon wheels on foot and while driving. Verify identical behavior and gun-filtering.
