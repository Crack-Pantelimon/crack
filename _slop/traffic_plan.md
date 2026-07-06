# Traffic Plugin + `traffic_test` Binary — Technical Plan

Goal: ambient road traffic (physics cars with a pedestrian driver inside) that follows OSM
road polylines, spawns just outside the camera's current view within a configurable radius,
and despawns when out of range / out of view / at path end. Plus a `traffic_test` binary with
a hardcoded intersection instead of real OSM data.

Paths are repo-relative. Everything under `src/plugins/traffic/` and `traffic_test.rs`
are **new files to create**; all other referenced files exist today.

---

## 1. Existing building blocks (reuse, don't reinvent)

| Need | Existing code |
|------|---------------|
| Road polylines | `GeoJsonDatabase.categories["roads"]` → `GeoJsonFeature.geometry: FeatureGeometry::LineString(Vec<Vec3>) / MultiLineString` (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/geojson.rs:431-459`). Points are already projected to world-space Vec3 (y is approximate; re-ground with raycast). |
| Load gating | States `OsmDatabaseLoadFinished::OsmFinished` and `InitialMapLoadFinished::Finished` (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/states/mod.rs`). |
| Path following precedent | `MovingBus` / `move_bus_system` (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/geojson.rs:2160-2259`): index-walk a `Vec<Vec3>`, ground each target with `query_point_ground_y`, lerp rotation. |
| Ground raycast | `query_point_ground_y(x, z, &MapTree, &SpatialQuery)` (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/geojson.rs:386`) — private today, make `pub(crate)`. Needs `MapTree`; the binary has none, so traffic code uses its own copy that takes `Option<Res<MapTree>>` and falls back to a fixed-height downward raycast. |
| Car physics entity | `spawn_car_request_event_observer` (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs`) shows the full bundle: `RigidBody::Dynamic`, `CarDriveState`, `CarWheelsContactData`, `CarSpeculativeContactData`, `NeedCarBoundsCompute`, `WorldAssetRoot(get_car_asset(...))`, collision layers, 4× `CosmeticWheel`. **Do not reuse the observer itself** — it forces `ActivePlayerVehicle` + `GameControlState::DrivingCar`. Extract the entity assembly into a shared helper. |
| Driving inputs | `CarDriveState.avg_accelerate / avg_brake / avg_steer` — the whole physics pipeline (`apply_car_steering_and_drive`) runs off these on any car entity. Traffic AI just writes these fields (velocity-space controller; never teleport the Transform). |
| Pedestrian in seat | `DriverMesh` + `CarSeatOffset` + `apply_seat_offset` (`crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:117-157`) — pattern for parenting a pedestrian model into a car with a sitting animation. Traffic uses its own lightweight variant. |
| Right-click popup | `handle_freecam_right_click` + `spawn_choice_popup_ui` (`interaction_ui.rs:19-106`) — add a third button. |
| Debug menu | `UiState` + Debug menu bar in `crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs` — add a `show_traffic_debug` toggle. |
| Binary skeleton | `crack_demo/demo_resolution_selector_web_bevy/src/bin/car_sim.rs`: `make_basic_app` + `EguiPlugin` + `UiState` + `PhysicsPlugin` + `GameStatesPlugin` + `CarsAndDrivingPlugin` + `SetupDebugScenePlugin`. Binaries in `src/bin/` are auto-discovered by cargo. |

---

## 2. New files

```
crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/
├── mod.rs            # TrafficPlugin, TrafficConfig, components, system registration
├── road_graph.rs     # TrafficRoadGraph resource + builder from GeoJsonDatabase
├── spawn.rs          # throttled network spawner, SpawnTrafficCarEvent, car+driver assembly
├── driver.rs         # path-following AI writing CarDriveState inputs
├── despawn.rs        # range / view-raycast / end-of-path / stuck despawn
└── debug_ui.rs       # egui "Traffic" window (sliders, counters, buttons, road gizmos)
crack_demo/demo_resolution_selector_web_bevy/src/bin/traffic_test.rs
```

Registration:
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/mod.rs`: `pub mod traffic;`
- `crack_demo/demo_resolution_selector_web_bevy/src/main_game_plugin.rs`: `.add_plugins(crate::plugins::traffic::TrafficPlugin)` (after
  `CarsAndDrivingPlugin`, since it spawns cars into that pipeline).

Refactor (small, mechanical):
- `spawn_car.rs`: extract `pub fn spawn_physics_car(commands, asset_server, pos, rot, car_type) -> Entity`
  containing the bundle + cosmetic-wheel spawning currently inlined in the observer. Observer
  calls it and then adds `ActivePlayerVehicle` + state switch; traffic calls it and adds
  `TrafficCar` instead.
- `geojson.rs`: `query_point_ground_y` → `pub(crate)`.

---

## 3. Data structures

```rust
// mod.rs
#[derive(Resource)]
pub struct TrafficConfig {
    pub enabled: bool,          // default true in main game, true in binary
    pub spawn_radius: f32,      // slider 50.0..=500.0, default 150.0 (meters from camera)
    pub max_cars: usize,        // slider 10..=100, default 30
    pub speed_kmh: f32,         // cruise speed target, default 30.0
    pub draw_road_gizmos: bool, // debug polyline rendering
}

/// Marker + path state on the car root entity.
#[derive(Component)]
pub struct TrafficCar {
    pub path: Vec<Vec3>,        // full resolved polyline (this segment + one continuation)
    pub next_idx: usize,        // next waypoint index
    pub stuck_timer: f32,       // secs below min speed
    pub out_of_view_timer: f32, // secs failing the visibility raycast
    pub half_height: f32,       // cached from CarDriveState for the view raycast target
}

/// Pedestrian model parented into a traffic car (visual only, no controller).
#[derive(Component)]
pub struct TrafficDriverMesh { pub car: Entity }

/// Trigger: spawn one traffic car whose path starts at/near `position`.
/// Fired by the right-click popup ("independent traffic": ignores throttle,
/// visibility and max_cars checks) and by the debug-UI "Spawn one" button.
#[derive(Event)]
pub struct SpawnTrafficCarEvent { pub position: Vec3 }
```

```rust
// road_graph.rs
#[derive(Resource, Default)]
pub struct TrafficRoadGraph {
    pub segments: Vec<RoadSegment>,       // one per LineString (MultiLineString flattened)
    /// endpoint -> segment indices touching it, key = position quantized to 1m grid
    pub node_index: HashMap<IVec2, Vec<usize>>,
    pub built: bool,
}
pub struct RoadSegment {
    pub points: Vec<Vec3>,                // projected world points (y approximate)
    pub length: f32,
}
fn quantize(p: Vec3) -> IVec2 { IVec2::new(p.x.round() as i32, p.z.round() as i32) }
```

Graph build (`build_road_graph` system):
- Runs in `Update`, guarded by `!graph.built && database.parsed`, plus the state run
  condition (§5). Iterates `database.categories.get("roads")`, pushes every
  `LineString` / each line of every `MultiLineString` with ≥ 2 points and length ≥ 20 m.
- Fills `node_index` from first/last point of each segment (quantized) so a path can
  continue onto a connected segment.
- Path resolution for a spawn: pick a segment + direction, walk to its end node, look up
  `node_index` for a different segment sharing that node, append its points (oriented
  away from the shared node). Exactly **one** continuation segment (matches the spec:
  "loads on a road segment and follows another segment"). Result is `TrafficCar.path`.

---

## 4. Systems

### 4.1 `spawn.rs` — throttled network spawner

`traffic_network_spawner(time: Res<Time>, mut last_spawn: Local<f32>, ...)`:

1. **Throttle**: `let now = time.elapsed_secs(); if now - *last_spawn < 0.1 { return; }`
   — the spec's "local f32 reading the bevy real timer"; set `*last_spawn = now` only when
   a car is actually spawned.
2. **Count gate**: `q_traffic.iter().count() >= config.max_cars` → return. (Count includes
   right-click-spawned cars; they just occupy budget once alive.)
3. **Candidate pick**: random segment, random point index within `config.spawn_radius`
   of the camera (`Query<(&GlobalTransform, &Camera), With<Camera3d>>`). Try up to ~10
   candidates per tick, first acceptable wins.
4. **"Visible if it rotated, but currently not"**: two checks against the active camera:
   - *In range*: `camera_pos.distance(point) <= spawn_radius` (that's the "would be
     visible if it rotated there").
   - *Currently not visible*: the point is outside the current frustum. Implement as
     `camera.world_to_ndc(cam_gt, point)` → reject candidate if NDC is inside
     x,y ∈ [-1,1] and z ∈ [0,1] (i.e. reject *visible* points). This is cheaper and more
     robust than angle math and handles FOV/aspect for free.
   - Also reject if closer than ~20 m to the camera (pop-in safety) or within ~8 m of an
     existing car (overlap).
5. **Spawn**: `commands.trigger(SpawnTrafficCarEvent { position: candidate })` — reuses the
   same observer as the right-click path.

`spawn_traffic_car_observer(On<SpawnTrafficCarEvent>, ...)`:
1. Snap `position` to the nearest point on the nearest `RoadSegment` (linear scan of
   segments' points is fine at OSM-neighborhood scale; keep a small early-out on bbox).
2. Resolve the full path (§3), choose travel direction toward the longer remaining side.
3. Ground the spawn point (raycast down; `Option<Res<MapTree>>`-aware helper), add +1.5 m.
4. `spawn_physics_car(...)` with rotation aligned to the first path direction
   (`Quat::from_rotation_arc(Vec3::NEG_Z, dir)`), random `car_list()` type; insert
   `TrafficCar { path, next_idx: 1, ... }` and give it initial `LinearVelocity` along the
   path (~`speed_kmh / 3.6`), like `set_car_initial_speed` in car_sim.
5. **Driver pedestrian**: pick a random URL from `PedestrianManifest`, spawn a model root
   (same asset spawn path as `spawn_pedestrian_observer`'s gltf child, but *without*
   physics/controller), insert `TrafficDriverMesh { car }`, `ChildOf(car)` with
   `CarSeatOffset::default()` local transform, and trigger the sitting animation via
   `TargetAnimation`/`PedestrianAnimationControlEvent` (same clip `DriverMesh` uses).
   If the manifest isn't loaded yet, skip the driver (car still spawns).

### 4.2 `driver.rs` — path following (pure pursuit on CarDriveState)

`drive_traffic_cars(time, mut q: Query<(&Transform, &LinearVelocity, &mut CarDriveState, &mut TrafficCar)>)`:
- Advance `next_idx` while `distance_xz(pos, path[next_idx]) < 4.0`.
- Lookahead target: first path point ≥ ~8 m ahead (or last point).
- Steering: signed angle between car forward (XZ) and direction-to-target →
  `avg_steer = (angle / max_steer_angle).clamp(-1, 1)` (reuse whatever normalization
  `keybinds_control_car` feeds; inspect `CarDriveState` fields when implementing).
- Throttle: proportional to `(target_speed - speed)`; `avg_brake` when overspeeding or
  when the turn angle is sharp. Target speed = `config.speed_kmh / 3.6`, halved near
  sharp corners.
- **Never** writes `Transform`/velocity directly — physics stays authoritative
  (velocity-space controller; Transform teleports are a known failed approach).
- Stuck tracking: if `speed < 0.5 m/s` while throttle > 0.3, accumulate
  `stuck_timer += dt`, else reset.

### 4.3 `despawn.rs`

`despawn_traffic_cars(...)`, cheap checks every frame, raycast check rate-limited to ~4 Hz
with the same `Local<f32>` elapsed-time pattern:

- **End of path**: `next_idx >= path.len()` (or within 4 m of last point) → despawn.
- **Out of range**: `distance(camera, car) > spawn_radius * 1.25` → despawn (hysteresis so
  cars don't flicker at the boundary).
- **Out of view** (rate-limited): raycast from camera position to
  `car_top = car_pos + Vec3::Y * (half_height * 2.0 * 0.95)` — i.e. top of the car minus
  5 % of its height. Using `SpatialQuery::cast_ray` toward that point with
  `max_distance = dist`, filtered to exclude the car entity itself (and its children):
  a hit ⇒ occluded. Also occluded-equivalent: point outside frustum (`world_to_ndc` check).
  Visible resets `out_of_view_timer`; not visible accumulates; `> 4.0 s` → despawn.
  (Right-click "independent" cars get the same treatment once alive — simplest and matches
  "out of distance or out of view ⇒ despawn".)
- **Stuck**: `stuck_timer > 6.0 s` → despawn (crashed into something, wedged, etc.).
- Despawn = `despawn()` on the car root (children incl. driver mesh go with it) **plus**
  its 4 `CosmeticWheel` entities — they are *not* children (see `spawn_car.rs:141-154`),
  so query `CosmeticWheel.parent_car == car` and despawn them explicitly.

### 4.4 `debug_ui.rs`

`traffic_debug_ui(contexts, config: ResMut<TrafficConfig>, ui_state: Option<ResMut<UiState>>, counts...)`
in `EguiPrimaryContextPass`:
- Window shown when `ui_state.map(|s| s.show_traffic_debug).unwrap_or(true)` — so binaries
  that don't tweak anything still get it, and the main game toggles it from the Debug menu.
  (The binary *does* have `UiState` for PhysicsPlugin; the binary sets
  `show_traffic_debug: true` at startup — see §6.)
- Contents: `enabled` checkbox, `spawn_radius` slider (50–500 m), `max_cars` slider
  (10–100, default 30), `speed_kmh` slider, live label "cars: N / max", buttons
  **Spawn one** (triggers `SpawnTrafficCarEvent` at a random on-road point in range,
  bypassing visibility) and **Despawn all**, checkbox `draw_road_gizmos`.
- `draw_traffic_gizmos` system: polylines of `TrafficRoadGraph.segments`
  (`gizmos.linestrip`), plus each car's remaining path and lookahead point.

---

## 5. Plugin wiring & run conditions

```rust
impl Plugin for TrafficPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrafficConfig>()
            .init_resource::<TrafficRoadGraph>()
            .add_observer(spawn::spawn_traffic_car_observer)
            .add_systems(Update, (
                road_graph::build_road_graph,
                spawn::traffic_network_spawner,
                driver::drive_traffic_cars,
                despawn::despawn_traffic_cars,
                debug_ui::draw_traffic_gizmos,
            ).chain().run_if(
                in_state(OsmDatabaseLoadFinished::OsmFinished)
                .and(in_state(InitialMapLoadFinished::Finished))
            ))
            .add_systems(EguiPrimaryContextPass, debug_ui::traffic_debug_ui);
    }
}
```

Notes:
- The two `in_state` conditions are exactly the requested gating ("wait for the osm_data
  plugin ... as well as the main map loaded state"). Both states already exist and are
  init'd by `GameStatesPlugin`; the binary drives them manually (§6).
- `traffic_network_spawner` additionally early-outs on `!config.enabled`.
- Egui UI runs ungated (it must render before/while loading, greyed out with a
  "waiting for OSM + map…" label when the states aren't Finished).
- All resource lookups that only exist in the main game (`MapTree`,
  `PedestrianManifest`, `UiState`) are `Option<Res<...>>` so the plugin is binary-safe.

### UI integration in the main game (`crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs`)
- Add `pub show_traffic_debug: bool` to `UiState` (+ both constructors, default `false`).
- Debug menu: add `if ui.button("Traffic").clicked() { ui_state.show_traffic_debug = !...; }`.

### Right-click popup (`interaction_ui.rs::spawn_choice_popup_ui`)
- Add button `"🚦 Traffic car"` → `commands.trigger(SpawnTrafficCarEvent { position: popup.world_pos }); close = true;`
- No new plumbing needed: the popup file already triggers cross-plugin events
  (`SpawnCarRequestEvent`). The observer snaps to the nearest road; if the road graph
  isn't built yet it logs a warn and does nothing.

---

## 6. `traffic_test.rs` binary (new, in `src/bin/`)

Modeled on `car_sim.rs`:

```rust
fn main() {
    make_basic_app("Traffic Test")
        .add_plugins(bevy_egui::EguiPlugin::default())
        .insert_resource(UiState { show_traffic_debug: true, ..UiState::with_physics_debug() })
        .add_plugins(PhysicsPlugin)
        .add_plugins(GameStatesPlugin)          // states + PedestriansPlugin (manifest, anims)
        .add_plugins(CarsAndDrivingPlugin)
        .add_plugins(SetupDebugScenePlugin)     // flat floor with Map collision layer
        .add_plugins(TrafficPlugin)
        .add_systems(Startup, (inject_hardcoded_intersection, force_loaded_states))
        .run();
}
```

- `inject_hardcoded_intersection`: builds a `GeoJsonDatabase` with
  `categories["roads"] = vec![feature_ns, feature_ew]`, `parsed: true` — two
  `FeatureGeometry::LineString`s crossing at the origin **in already-projected world
  coordinates** ("hardcoded into the osm 3d data"): N–S from `(0, 0.5, -200)` to
  `(0, 0.5, 200)` and E–W from `(-200, 0.5, 0)` to `(200, 0.5, 0)`, points every 10 m,
  y ≈ 0.5 above the debug floor (exact y comes from the ground raycast at spawn anyway).
  Tags: `{"highway": "residential"}`, names "NS Road" / "EW Road". Because the four
  endpoints don't share nodes, cars spawned on one road cross the intersection and
  **despawn at their end node** — exactly the requested binary behavior; cars spawned
  near the center still demonstrate the crossing.
- `force_loaded_states`: `next_osm.set(OsmDatabaseLoadFinished::OsmFinished);`
  `next_map.set(InitialMapLoadFinished::Finished);` — satisfies the plugin's run
  conditions without MapPlugin/GeoJsonPlugin.
- UI is always visible (no menu in binaries): `show_traffic_debug: true` above, and the
  traffic window itself provides spawn/despawn buttons and all sliders.
- No `MapTree` ⇒ ground helper falls back to raycasting down from y = +50.
- Pedestrian driver requires the manifest from `config::DATA_BASE_URL`; if it fails to
  load (offline), cars simply spawn driverless (already handled in §4.1 step 5).

---

## 7. Edge cases & risks

- **Roads category name**: confirm `"roads"` is the exact key (it's what
  `geojson.rs:1259` uses). Features may be `MultiLineString` — flatten each line into its
  own `RoadSegment`.
- **Projected y is unreliable** (`project_point` puts y at tile-bbox bottom + 2). Always
  re-ground via raycast at spawn and let car physics own y afterwards. If the ground tile
  under a candidate isn't loaded yet (no raycast hit), reject the candidate.
- **Cosmetic wheels leak** on despawn — must be despawned explicitly (§4.3).
- **`NeedCarBoundsCompute` timing**: `TrafficCar.half_height` should be read from
  `CarDriveState.car_half_height` after spawn; the view raycast can use the default until
  bounds compute finishes — good enough for a 5 % fudge.
- **Observer state guard**: do *not* copy `spawn_car_request_event_observer`'s
  `GameControlState::MapFreecam` guard — traffic must also spawn while the player drives
  or walks.
- **Performance**: 30–100 physics cars is the real cost driver (avian + convex
  decomposition colliders per car). If frame time tanks, follow-up option: simple cuboid
  collider for traffic cars instead of `ColliderConstructorHierarchy` (flag on the shared
  spawn helper). Note it in the debug UI (car count → FPS is visible there).
- **Spawner starvation**: if every candidate in range is inside the frustum (camera
  looking down from high up sees everything), the spawner simply doesn't spawn — correct
  per spec, but the debug UI should show "0 valid spawn candidates" to avoid confusion.
- **WASM**: no new deps, no threads, rand already used ⇒ web build unaffected; keep
  raycast counts bounded (≤ ~10 candidate + ~N/4 visibility rays per frame).

---

## 8. Implementation order

1. Refactor: extract `spawn_physics_car` helper in `spawn_car.rs`; make
   `query_point_ground_y` `pub(crate)`. `cargo check`.
2. `road_graph.rs` + `mod.rs` (plugin skeleton, config, components, run conditions).
3. `spawn.rs`: observer first (spawn a car on a road from a hardcoded event), then the
   throttled network spawner with the frustum/range checks.
4. `driver.rs` pure-pursuit + `despawn.rs` (end-of-path, range, stuck; view raycast last).
5. `debug_ui.rs` window + gizmos; `UiState.show_traffic_debug` + Debug-menu entry.
6. New binary `traffic_test.rs` with hardcoded intersection + forced states. Iterate on
   driving quality here (fast loop, flat floor, roads visible as gizmos).
7. Pedestrian driver mesh in the seat (sitting anim).
8. Right-click popup button in `interaction_ui.rs`.
9. Verify in main game (`trunk serve` / native run): traffic appears after "geojson
   loaded" tooltip, sliders work, cars despawn at range edge and when occluded long enough.
