The game is in folder `crack_demo/demo_resolution_selector_web_bevy`.

Base rust packages are under `rust_pkg`. 

Data/asset generation and pre-procesing is in `_data`.

## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## deps
```
_data/3d_data_v2/_blend_build_map.py ← mathutils, bmesh, bpy, numpy
_data/3d_data_v2/_blend_render_postprocess.py ← __future__, mathutils, bpy
_data/3d_data_v2/_blend_render_topdown.py ← __future__, mathutils, bpy
_data/3d_data_v2/_check_blend.py ← bpy, numpy
_data/3d_data_v2/osm_postprocess_batch.py ← octree, cv2, pyarrow, yolo_v8_obb_sat
_data/3d_data_v2/yolo_v7_sat.py ← __future__, cv2, numpy
_data/3d_data_v2/yolo_v8_obb_sat.py ← __future__, cv2, numpy
_data/3d_data_v2/osm_download.py ← octree, pyarrow, requests
```

## todos
```
crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs:439  # TODO: (upper/lower body decoupling): instead of snapping the whole controlle
rust_pkg/storage_crackhouse/src/models.rs:182  # TODO: ! Get existing model SQLs from the DB and only drop/create if changed
rust_pkg/api_asscrack/src/crack_worker/api_worker.rs:43  # TODO: get which is missing...
```

## changes (last 5 commits — 10 seconds ago)
```
_data/3d_data_v2/_blend_build_map.py          +weld_terrain_mesh  +build_terrain_bvh  +_hsv_saturation  +_sample_ground_palette
_data/3d_data_v2/_blend_render_postprocess.py +pick_render_engine  +convert_materials_to_emission  +make_cage_material  +show_car_wrappers_as_cage
_data/3d_data_v2/_blend_render_topdown.py     ~setup_render_settings
_data/3d_data_v2/_check_blend.py              +_find_base_color_image  +check_blend
```

## _data

### _data/3d_data_v2/_blend_build_map.py
```
def clear_scene()  :28-30  # Wipe the current scene so the next tile imports into a clean
def weld_terrain_mesh(obj: bpy.types.Object, dist: float) → None  :33-48
def measure_terrain_bbox() → dict  :51-81  # Compute axis-aligned bounds of all mesh objects in Blender c
def latlon_to_xy(lon: float, lat: float, latlon_bbox: dict, terrain_bbox: dict) → tuple[float, float]  :84-99  # Map lat/lon to Blender (east, north) via bilinear extrapolat
def build_terrain_bvh(terrain_objs: list[bpy.types.Object] | None) → BVHTree | None  :102-131  # Build a world-space BVH over all terrain meshes once, up fro
def raycast_hit(x: float, y: float, top: float, bvh: BVHTree | None) → tuple[float, Vector] | None  :134-135  # Cast downward from above the terrain bbox; return (hit z, hi
def raycast_height(x: float, y: float, top: float, bvh: BVHTree | None) → float | None  :148-151  # Cast downward from above the terrain bbox; return hit z or N
def resolve_heights(heights: list[float | None]) → list[float] | None  :154-178  # Fill ray-cast misses from nearest chain neighbor with a hit
def get_or_create_collection(name: str) → bpy.types.Collection  :181-186
def create_road_object(feature_id, coords_xy: list[tuple[float, float]], heights: list[float]) → bpy.types.Object  :189-192  # Create a mesh polyline named road_<feature_id> in the roads 
def resolve_corner_heights(raw_zs: list[float | None], top: float) → list[float]  :208-212  # Fill corner ray misses with the average z of the corners tha
def build_collider_mesh(corners_latlon: list[list[float]], center_latlon: list[float], latlon_bbox: dict, terrain_bbox: dict, top: float, bvh: BVHTree | None) → dict  :215-221  # Build a closed box collider molded onto the terrain, enlarge
def create_car_object(car_index: int, verts: list[tuple[float, float, float]], faces: list[list[int]]) → bpy.types.Object  :299-302  # Link a pre-built collider mesh into the cars collection
def build_fill_material(mesh: bpy.types.Object, index: int) → tuple[int, int] | None  :371-372
def cut_car_from_terrain(mesh_obj: bpy.types.Object, car_obj: bpy.types.Object, z_range: tuple[float, float], mark_mat: bpy.types.Material, n_colors: int, fill_slot: int) → tuple[int, int, list]  :589-595
def log(msg: str) → None  :774-775
def process_item(item: dict) → None  :778-904
def main()  :907-936
```

### _data/3d_data_v2/_blend_render_postprocess.py
```
def pick_render_engine() → str  :30-38
def convert_materials_to_emission() → None  :41-66  # Flatten every textured material to an unlit emission of its 
def make_cage_material() → bpy.types.Material  :69-98  # A translucent red-orange tint for the car wrappers: emission
def show_car_wrappers_as_cage() → None  :101-116  # Tint every object in the 'cars' collection translucent red s
def compute_mesh_bbox(objects) → dict | None  :119-144
def setup_world_black(scene: bpy.types.Scene) → None  :147-153
def render_blend(blend_path: str) → bool  :156-211
def main() → None  :214-230
```

### _data/3d_data_v2/_blend_render_topdown.py
```
def enable_gpu_rendering() → list[str]  :25-55  # Enable GPU compute devices for Blender rendering
def pick_render_engine() → str  :58-66
def ensure_gpu_rendering() → None  :69-74
def clear_scene() → None  :77-96
def convert_materials_to_emission() → None  :99-123
def compute_mesh_bbox() → dict | None  :126-154
def resolve_resolution(tile: dict) → tuple[int, int]  :157-161
def setup_render_settings(scene: bpy.types.Scene, *, width: int, height: int) → None  :164-187
def render_tile(tile: dict) → bool  :190-253
def main() → None  :256-287
```

### _data/3d_data_v2/_check_blend.py
```
def check_blend(blend_path: str) → None  :17-115
```

### _data/3d_data_v2/osm_postprocess_batch.py
```
def run_blender_batch(script: str, batch_json_path: str) → str  :52-80  # Run a Blender -P script over a batch JSON file, streaming it
def glb_path_for_tile(tile: str) → Path  :83-87  # Return the on-disk GLB path for an octant path (matches main
def tile_sidecar_paths(tile: str) → dict[str, Path]  :90-97
def pixel_to_latlon(px: float, py: float, meta: dict) → tuple[float, float]  :100-120  # Map render pixel to lat/lon using ortho camera + mesh-bbox a
def obb_pixel_to_latlon_corners(corners_pixel: list[list[float]], meta: dict) → tuple[list[list[float]], li...  :123-124
def node_inside_bbox(lon: float, lat: float, bbox) → bool  :133-135  # Half-open containment: south <= lat < north, west <= lon < e
def trim_road_feature(feature: dict, bbox) → dict | None  :138-175  # Keep coordinate indices inside bbox or adjacent to an inside
def has_lanes(feature: dict) → bool  :178-180
def lookup_manifest_row(manifest_dataset, tile: str) → dict | None  :183-189
def query_road_feature_ids(octtree_dataset, candidate: str) → list  :192-203
def load_road_features(features_dataset, feature_ids: list[int]) → list[dict]  :206-223
def find_roads_for_tile(tile: str, octtree_dataset, features_dataset) → tuple[str | None, list[dict]]  :226-229  # Walk up the parent chain from tile until qualifying roads ar
def build_work_item(tile: str, manifest_row: dict, road_source_path: str | None, roads: list[dict], sidecars: dict[str, Path]) → dict  :254-259
def load_sample_tiles() → list[str]  :288-290
def run_render_stage(tile_specs: list[dict]) → None  :293-307
def run_detect_stage(tile_records: list[dict], net) → None  :310-358
def run_blend_stage(items: list[dict]) → None  :361-375
def main()  :378-469
```

### _data/3d_data_v2/pyproject.toml
```
table [project]
key name
key version
key description
key readme
key requires-python
key dependencies
```

### _data/3d_data_v2/yolo_v7_sat.py
```
def load_net(onnx_path: Path | str) → cv2.dnn.Net  :14-18
def detect_cars(net: cv2.dnn.Net, image_bgr: np.ndarray, *, conf: float, nms: float) → list[dict]  :21-26  # Return car detections as pixel bboxes in the source image
```

### _data/3d_data_v2/yolo_v8_obb_sat.py (used by: _data/3d_data_v2/osm_postprocess_batch.py)
```
def load_net(onnx_path: Path | str) → cv2.dnn.Net  :29-33
def detect_cars(net: cv2.dnn.Net, image_bgr: np.ndarray, *, conf: float, nms: float) → list[dict]  :43-48  # Return vehicle detections as rotated pixel quads in the sour
```

### _data/3d_data_v2/osm_download.py
```
def format_eta(seconds: float) → str  :90-100  # Format seconds into a human-readable ETA string
def download_category_with_retry(query_part: str, bbox_str: str, proxies: dict, headers: dict) → dict | None  :103-105  # Download OSM data for a query part from public Overpass API 
def download_all()  :171-285  # Stage 1: download OSM layers into data_osm/original/
def load_octree_index()  :288-321  # Load octree tile bboxes and parent/child relationships from 
def iter_lonlat(geometry)  :335-343  # Yield (lon, lat) from any GeoJSON geometry
def collect_octant_paths(points, path, bboxes, children)  :346-356  # Recursively assign points to octant paths using half-open bb
def assign_feature_paths(geometry, bboxes, children, roots)  :359-368  # Return the set of octant paths that contain any point of the
def build_feature_manifests()  :371-440  # Stage 2: build features
def main()  :443-445
```

## crack_demo

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_plugin_ui.rs
```
pub fn configure_map_extent_gizmo(mut store: ResMut<GizmoConfigStore>)
pub fn draw_tree_bboxes(_gizmos: Gizmos, _data_res: Res<MapTree>, _lod_state: Res<MapLODState>, _tiles_query: Query<&TreeMapTile>, _ui_state: Option<Res<crate::ui_egui::UiState>>,)
pub fn draw_map_extent_gizmo(mut gizmos: Gizmos<MapExtentGizmoGroup>, data_res: Res<MapTree>, ui_state: Option<Res<crate::ui_egui::UiState>>,)
pub fn tree_navigator_ui(mut contexts: EguiContexts, data_res: Res<MapTree>, mut lod_state: ResMut<MapLODState>, tiles_query: Query<&TreeMapTile>, ui_state: Option<ResMut<crate::ui_egui::UiState>>,)
pub fn draw_reference_points_gizmos(mut gizmos: Gizmos, data_res: Res<MapTree>, lod_state: Res<MapLODState>, camera_query: Query<&Transform, With<Camera>>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/main_game_plugin.rs
```
pub struct MainGamePlugin
impl MainGamePlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/camera_follow.rs
```
pub struct DrivingAim
pub fn update_driving_aim(mouse: Res<ButtonInput<MouseButton>>, mut contexts: EguiContexts, capture_state: Res<crate::plugins::states::MouseCaptureState>, mut aim: ResMut<DrivingAim>,)
pub fn camera_follows_car(time: Res<Time>, mut camera_query: Query<&mut Transform, (With<MainCamera>, Without<ActivePlayerVehicle>)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs
```
pub struct DrivingPlugin
pub struct WheelContactData
pub struct CarWheelsContactData
pub struct Drive
pub struct SimState
pub struct CarDriveState
pub struct CosmeticWheel
pub enum GamePhysicsLayer
impl DrivingPlugin
impl WheelContactData
impl CarDriveState
pub fn configure_gizmo_depth(mut gizmo_store: ResMut<GizmoConfigStore>)
pub fn cap_car_velocities(mut q_car: Query<(&mut LinearVelocity, &mut AngularVelocity, &CarDriveState)
pub fn car_drive_observer(trigger: On<Drive>, mut query: Query<&mut CarDriveState>, time: Res<Time>,)
pub fn update_vehicle_physics_from_tuning(q_car: Query<(Entity, &CarDriveState)
pub fn apply_car_steering_and_drive(mut q_car: Query< ( &Transform, &mut CarDriveState, &CarWheelsContactData, Option<&CarSpeculativeContactData>, &mut LinearVelocity, &mut AngularVelocity,)
pub fn detect_gear_shifts(mut last_gears: Local<std::collections::HashMap<Entity, usize>>, query: Query<(Entity, &Transform, &CarDriveState)
pub fn init_cosmetic_wheels_system(mut q_wheels: Query<(Entity, &Transform, &mut CosmeticWheel)
pub fn update_cosmetic_wheels(mut commands: Commands, mut q_wheels: Query<(Entity, &mut Transform, &mut CosmeticWheel)
pub fn update_wheel_contact_normals(spatial_query: SpatialQuery, mut q_cars: Query< ( Entity, &Transform, &CarDriveState, &mut CarWheelsContactData,)
pub fn draw_car_gizmos(mut gizmos: Gizmos, q_car: Query< ( &Transform, &CarDriveState, &CarWheelsContactData, Option<&CarSpeculativeContactData>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs
```
pub struct CameraKinematics
pub fn track_camera_kinematics(time: Res<Time>, q_camera: Query< &GlobalTransform, With<crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera>, >, mut kin: ResMut<CameraKinematics>,)
pub fn spawn_lod_task(map_tree: Res<MapTree>, lod_state: Res<MapLODState>, q_merge: Query<&TileShouldMerge>, q_split: Query<&TileShouldSplit>, q_pending: Query<&PendingTileReveal>, q_nodes: Query<&TreeMapTile>, mut last: Local< Option<( BTreeSet<MapTreeNodePath>, Vec<Vec3>, u32, bool, i32, (u32, u32)
pub fn poll_lod_task(mut tasks: ResMut<CrackTasks>, mut res_tiles: ResMut<TileSwapRequests>)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/manifest_flow.rs
```
pub fn spawn_manifest_task(map_tree: Res<MapTree>, mut tasks: ResMut<CrackTasks>, client: Res<CrackClient>,)
pub fn poll_manifest_task(mut tasks: ResMut<CrackTasks>, mut map_tree: ResMut<MapTree>, mut lod_state: ResMut<MapLODState>, mut camera_query: Query<&mut Transform, With<Camera>>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/mod.rs
```
pub struct CrackClient
pub struct CrackClientSlot
pub struct CrackRuntime
pub struct CrackTasks
pub struct CrackPlugin
impl CrackPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/debug_picker.rs
```
pub struct DebugPickerPlugin
pub struct DebugPickerState
pub struct PickResult
pub enum PickKind
impl DebugPickerPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/main_scene_plugin.rs
```
pub struct MainScenePlugin
pub struct SkyboxState
impl MainScenePlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_lod.rs
```
pub struct TreeMapTile
pub struct TileShouldMerge
pub struct TileShouldSplit
pub struct TileSwapRequests
pub struct PendingTileReveal
pub struct PendingTileGroupFetch
pub enum TileGroupFetchPurpose
pub fn spawn_root_map_tiles(mut commands: Commands, data_res: Res<MapTree>, client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,)
pub fn start_tile_swap_requests(mut commands: Commands, mut res_tiles: ResMut<TileSwapRequests>, client: Option<Res<crate::plugins::crack_plugin::CrackClient>>, q_split: Query<&TileShouldSplit>, q_merge: Query<&TileShouldMerge>, q_fetch: Query<&PendingTileGroupFetch>,)
pub fn poll_tile_group_fetches(mut commands: Commands, mut q_fetches: Query<(Entity, &mut PendingTileGroupFetch)
pub fn do_split_requests(mut commands: Commands, q_split: Query<(&TileShouldSplit, Entity)
pub fn do_merge_requests(mut commands: Commands, q_merge: Query<(&TileShouldMerge, Entity)
pub fn reveal_pending_tiles(mut commands: Commands, mut q_pending: Query<(Entity, &mut PendingTileReveal)
pub fn check_map_loaded_status(tiles_query: Query<&TreeMapTile>, lod_state: Res<MapLODState>, loading_status: Option<ResMut<crate::plugins::geojson::GameLoadingStatus>>, mut commands: Commands, mut next_state: ResMut<NextState<InitialMapLoadFinished>>, mut osm_state: ResMut<NextState<OsmDatabaseLoadFinished>>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/mod.rs
```
pub struct MapPlugin
pub struct MapTree
pub struct MapLODState
impl MapPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs
```
pub fn character_input(keys: Res<ButtonInput<KeyCode>>, camera: Query<&GlobalTransform, With<MainCamera>>, controlled: Res< crate::plugins::pedestrians::pedestrian_controller_plugin::spawn::ControlledCharacter, >, mut query: Query<(&mut LocomotionInput, &mut MovementModifiers)
pub fn update_grounded(mut commands: Commands, mut query: Query<(Entity, &GroundDetection, &GlobalTransform)
pub fn movement(time: Res<Time>, mut controllers: Query< ( &mut LocomotionInput, &CharacterMovementSettings, &mut LinearVelocity, Has<Grounded>,)
pub fn apply_gravity(time: Res<Time>, mut controllers: Query< (&CharacterMovementSettings, &mut LinearVelocity)
pub fn apply_movement_damping(mut query: Query<(&CharacterMovementSettings, &mut LinearVelocity)
pub fn apply_speed_cap(time: Res<Time>, mut query: Query< (&mut MovementModifiers, &mut LinearVelocity, Has<Rolling>)
pub fn move_and_slide(mut query: Query< ( Entity, Option<&GroundDetection>, Option<&mut CharacterCollisions>, &mut Transform, &mut LinearVelocity, &Collider,)
pub fn apply_forces_to_dynamic_bodies(characters: Query<(&ComputedMass, &CharacterCollisions)
pub fn face_movement(time: Res<Time>, mut query: Query< (&LinearVelocity, &mut Transform)
pub fn face_aim(rig: Option<Res<CameraRig>>, controlled: Option<Res<ControlledCharacter>>, camera: Query<&GlobalTransform, With<MainCamera>>, combat_states: Query<&CombatState>, mut query: Query<&mut Transform, (With<CharacterController>, Without<CarPassenger>)
pub fn respawn_if_fallen(mut query: Query< (&mut Transform, &mut LinearVelocity)
pub fn jump_or_climb(keys: Res<ButtonInput<KeyCode>>, spatial_query: SpatialQuery, mut commands: Commands, controlled: Res< crate::plugins::pedestrians::pedestrian_controller_plugin::spawn::ControlledCharacter, >, map: Option<Res<MapTree>>, tiles: Query<()
pub fn update_climb(time: Res<Time>, mut commands: Commands, mut query: Query< (Entity, &mut Transform, &mut LinearVelocity, &mut Climbing)
pub fn update_roll(time: Res<Time>, mut commands: Commands, mut query: Query< (Entity, &Transform, &mut LinearVelocity, &mut Rolling)
pub fn detect_fallen_off_map(map: Option<Res<MapTree>>, tiles: Query<()
```

### crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs
```
pub struct UiEguiPlugin
pub struct UiState
impl UiEguiPlugin
impl UiState
impl UiState
  pub fn with_physics_debug() → Self
impl UiState
pub fn web_set_loading_status(_show: bool, _message: &str)
```

### crack_demo/game_logic/src/api.rs
```
pub struct FetchArgs
```

### crack_demo/game_logic/src/lod.rs
```
pub struct Score
pub struct CameraReference
pub struct LodComputeRequest
pub struct SplitRequestSummary
pub struct MergeRequestSummary
pub struct CulledNodeSummary
pub struct LodComputeResponse
impl Score
impl Score
impl Score
pub fn compute_distance_to_aabb(bbox: &BBox, p: Vec3) → f32
pub async fn compute_lod_changes(data_res: &MapTreeData, req: &LodComputeRequest,) → LodComputeResponse
```

### crack_demo/game_logic/src/visibility.rs
```
pub struct OccluderWorld
impl OccluderWorld
  pub fn new_empty() → Self
  pub fn insert_occluder(&mut self, path: &MapTreeNodePath, bbox: &BBox, trimesh: Arc<TriMesh>)
  pub fn remove_node(&mut self, path: &MapTreeNodePath)
  pub fn retain_paths(&mut self, keep: &BTreeSet<MapTreeNodePath>)
  pub fn is_ray_occluded(&self, origin: Vector, target: Vector, exclude_path: &MapTreeNodePath, exclude_bbox: &BBox,) → bool
  pub fn is_node_visible(&self, node_bbox: &BBox, node_path: &MapTreeNodePath, cameras: &[CameraReference],) → bool
pub fn verdict_should_refresh(path_key: &str, now_ms: i64) → bool
pub fn build_trimesh_from_mesh(vertices: &[[f32; 3]], indices: &[[u32; 3]]) → Option<TriMesh>
pub async fn get_or_build_trimesh(path: &MapTreeNodePath, assets: &[(String, String) → Option<Arc<TriMesh>>
pub async fn get_cached_trimesh(path: &MapTreeNodePath) → Option<Arc<TriMesh>>
```

### crack_demo/game_logic/src/worker/osm_impl.rs
```
pub async fn fetch_osm_data(args: FetchArgs) → anyhow::Result<OsmDataResult>
```

## rust_pkg

### rust_pkg/net_crackpipe/src/_random_word.rs
```
pub fn get_nickname_from_pubkey(pubkey: PublicKey) → String
```

### rust_pkg/net_crackpipe/src/chat/chat_const.rs
```
pub fn get_relay_domain() → (String, String)
```

### rust_pkg/net_crackpipe/src/chat/chat_controller.rs
```
pub struct ChatController
pub struct ChatSender
pub struct ChatReceiver
pub enum ChatMessage
pub trait IChatController
pub trait IChatSender
pub trait IChatReceiver
pub trait IChatRoomRaw
impl ChatController
impl ChatController
impl IChatController
impl IChatSender
impl ChatSender
impl IChatReceiver
```

### rust_pkg/net_crackpipe/src/chat/chat_presence.rs
```
pub struct ChatPresence
pub struct PresenceList
pub struct PresenceListItem
pub enum PresenceFlag
impl PresenceFlag
  pub fn from_instant(instant: i64) → Self
impl PresenceList
impl ChatPresence
  pub fn new() → Self
  pub fn notified(&self) → tokio::sync::futures::Notif...
  pub async fn add_presence(&self, identity: &NodeIdentity, payload: &Option<T::P>) → bool
  pub async fn update_ping(&self, identity: &NodeIdentity, rtt: u16)
  pub async fn get_presence_list(&self) → PresenceList<T::P>
  pub async fn remove_presence(&self, identity: &NodeIdentity)
impl ChatPresenceData
```

### rust_pkg/net_crackpipe/src/chat/chat_ticket.rs
```
pub struct ChatTicket
impl ChatTicket
  pub fn new_str_bs(topic_id: &str, bs: BTreeSet<NodeId>) → Self
```

### rust_pkg/net_crackpipe/src/chat/direct_message.rs
```
pub struct ChatDirectMessage
pub struct DirectMessageProtocol
impl DirectMessageProtocol
  pub async fn shutdown(&self)
  pub async fn new(received_message_broadcaster: async_broadcast::Sender<(PublicKey, T) → Self
  pub async fn send_direct_message(&self, iroh_target: PublicKey, payload: T,) → anyhow::Result<()>
impl DirectMessageProtocol
impl MessageDispatchers
  pub fn new(endpoint: Endpoint) → Self
  pub async fn shutdown(&self)
  pub async fn drop_dispatcher(&self, target: PublicKey)
  pub async fn send_message(&self, target: PublicKey, payload: T) → anyhow::Result<()>
impl MessageDispatcher
  pub fn new(target: PublicKey, endpoint: Endpoint) → Self
  pub async fn send_message(&self, payload: T) → anyhow::Result<()>
```

### rust_pkg/net_crackpipe/src/chat/global_chat.rs
```
pub struct GlobalChatRoomType
pub struct GlobalChatPresence
pub enum GlobalChatMessageContent
pub enum GlobalChatBootstrapQuery
pub enum MatchHandshakeType
impl GlobalChatRoomType
```

### rust_pkg/net_crackpipe/src/chat/room_raw.rs
```
pub struct GossipChatRoom
impl GossipChatRoom
  pub async fn new(node: &MainNode, ticket: &ChatTicket) → Result<Self>
impl GossipChatRoom
```

### rust_pkg/net_crackpipe/src/echo.rs
```
pub struct Echo
impl Echo
  pub fn new(own_endpoint_node_id: NodeId, sleep_manager: SleepManager) → Self
impl Echo
impl Echo
```

### rust_pkg/net_crackpipe/src/global_matchmaker.rs
```
pub struct GlobalMatchmaker
pub struct BootstrapNodeInfo
impl GlobalMatchmakerInner
  pub async fn shutdown(&mut self) → Result<()>
impl GlobalMatchmaker
impl GlobalMatchmaker
  pub async fn sleep(&self, duration: Duration)
  pub async fn shutdown(&self) → Result<()>
  pub fn user_secrets(&self) → std::sync::Arc<UserIdentity...
  pub fn own_node_identity(&self) → NodeIdentity
  pub fn user(&self) → UserIdentity
  pub async fn global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn bs_global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn display_debug_info(&self) → Result<String>
```

### rust_pkg/net_crackpipe/src/lib.rs
```
pub fn timestamp_micros() → u128
pub fn datetime_now() → DateTime<Utc>
```

### rust_pkg/net_crackpipe/src/main_node.rs
```
pub struct MainNode
impl MainNode
  pub async fn spawn(node_identity: Arc<NodeIdentity>, node_secret_key: Arc<SecretKey>, own_endpoint_node_id: Option<NodeId>, user_secrets: Arc<UserIdentitySecrets>, sleep_manager: SleepManager,) → Result<Self>
  pub fn user(&self) → &NodeIdentity
  pub fn endpoint(&self) → &Endpoint
  pub fn node_id(&self) → NodeId
  pub fn remote_info(&self) → Vec<RemoteInfo>
  pub fn node_identity(&self) → &NodeIdentity
  pub async fn shutdown(&self) → Result<()>
  pub async fn join_chat(&self, ticket: &ChatTicket) → Result<ChatController<T>> w...
```

### rust_pkg/net_crackpipe/src/network_manager.rs
```
pub struct NetworkManagerConfig
pub struct NetworkManager
impl NetworkManager
  pub async fn init(secrets: Arc<UserIdentitySecrets>, config: NetworkManagerConfig,) → Result<Self>
  pub fn matchmaker(&self) → GlobalMatchmaker
  pub async fn global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn join_room(&self, topic_id: &str) → Result<ChatController<T>>
  pub async fn shutdown(&self) → Result<()>
pub async fn run_standalone_bootstrap_if_needed(extra_topics: Vec<String>) → Result<()>
```

### rust_pkg/net_crackpipe/src/signed_message.rs
```
pub struct SignedMessage
pub struct MessageSigner
pub struct WireMessage
pub struct ReceivedMessage
pub enum ChatMessage
pub trait AcceptableType
pub trait IChatRoomType
impl SignedMessage
  pub fn verify_and_decode(bytes: &[u8]) → Result<WireMessage<T>>
impl MessageSigner
  pub fn sign_and_encode(&self, message: T,) → Result<(Vec<u8>, WireMessag...
```

### rust_pkg/net_crackpipe/src/sleep.rs
```
pub struct SleepManager
impl SleepManager
  pub fn new() → Self
  pub async fn sleep(&self, duration: Duration)
  pub fn wake_up(&self)
impl SleepManagerInner
```

### rust_pkg/net_crackpipe/src/user_identity.rs
```
pub struct UserIdentity
pub struct UserIdentitySecrets
pub struct NodeIdentity
impl UserIdentity
  pub fn nickname(&self) → String
  pub fn user_id(&self) → &PublicKey
  pub fn html_color(&self) → String
  pub fn rgb_color(&self) → (u8, u8, u8)
impl UserIdentitySecrets
impl UserIdentitySecrets
  pub fn user_identity(&self) → &UserIdentity
  pub fn secret_key(&self) → &SecretKey
  pub fn generate() → Self
impl NodeIdentity
  pub fn nickname(&self) → String
  pub fn html_color(&self) → String
  pub fn rgb_color(&self) → (u8, u8, u8)
  pub fn user_id(&self) → &PublicKey
  pub fn node_id(&self) → &PublicKey
  pub fn user_identity(&self) → &UserIdentity
  pub fn bootstrap_idx(&self) → Option<u32>
  pub fn new(user_identity: UserIdentity, node_id: PublicKey, bootstrap_idx: Option<u32>,) → Self
```

### rust_pkg/storage_crackhouse/src/api.rs
```
pub async fn execute_sql2(sql: String) → anyhow::Result<SqlResultSet>
pub async fn execute_sql_params(req: SQLAndParams) → anyhow::Result<SqlResultSet>
```

### rust_pkg/storage_crackhouse/src/impl_rusqulite.rs
```
pub async fn sql_query(sql: SQLAndParams) → anyhow::Result<SqlResultSet>
```

### rust_pkg/storage_crackhouse/src/lib.rs
```
pub async fn install_opfs_sahpool() → anyhow::Result<()>
pub async fn install_relaxed_idb() → anyhow::Result<()>
```

### rust_pkg/storage_crackhouse/src/models.rs
```
pub struct ModelColumnImpl
pub trait ModelGroup
pub trait ModelDef
pub trait ModelSerial
pub trait DbTypeMapping
impl i64
impl String
impl f64
impl Vec
impl Option
pub async fn run_migrate_tables(groups: impl Iterator<Item = Arc<dyn ModelGroup>>,) → anyhow::Result<()>
```

### rust_pkg/storage_crackhouse/src/types.rs
```
pub struct SQLAndParams
pub struct SqlResultSet
pub struct SqlResultRow
pub enum DbValueType
pub enum DbValue
impl DbValueType
  pub fn to_sql_str(&self) → &'static str
impl DbValue
  pub fn fold_option(value: Option<DbValue>) → DbValue
impl TryFrom
impl String
impl i64
impl f64
impl Vec
```

### rust_pkg/web_serviceworker_crackslave/src/lib.rs
```
pub async fn _js_init_dedicated_worker() → Result<(), JsValue>
pub async fn _js_compute_payload_reply(msg: JsValue) → Result<JsValue, JsValue>
pub async fn web_worker_registration(mapping: Arc<ApiImplMapping>,) → std::result::Result<(), JsV...
```

### rust_pkg/web_serviceworker_crackloader/src/lib.rs
```
pub struct WebWorkerFactory
impl WebWorkerFactory
```

### rust_pkg/api_asscrack/src/api/api_client.rs
```
pub struct ApiClient
pub struct MessageLater
impl ApiClient
  pub fn new(pipe: WorkerPipe) → Self
  pub async fn call(&self, arg: T::Arg) → anyhow::Result<T::Ret>
```

### rust_pkg/api_asscrack/src/api/api_method_macros.rs
```
pub struct ApiGroupDeclStatic
pub struct ApiMethodInfo
pub struct ApiMethodImpl
pub trait ApiGroupDecl
pub trait ApiGroupMethods
pub trait ApiGroupImpls
pub trait ApiMethodDecl
impl ApiMethodImpl
  pub fn fullname(&self) → String
impl ApiMethodInfo
  pub fn fullname(&self) → String
```

### rust_pkg/api_asscrack/src/api/api_worker_declarations.rs
```
pub async fn worker_ping(_x: () → anyhow::Result<()>
```

### rust_pkg/api_asscrack/src/crack_worker/api_worker.rs
```
pub struct ApiImplMapping
pub fn make_api_mapping(groups: Vec<Arc<dyn ApiGroupImpls>>) → Arc<ApiImplMapping>
pub async fn compute_response_message(_request: WorkerMessage, mapping: Arc<ApiImplMapping>,) → WorkerMessage
```

### rust_pkg/api_asscrack/src/crack_worker/mod.rs
```
pub struct WorkerPipe
pub struct WorkerMessage
pub trait WorkerLoaderFactory
```

### rust_pkg/thread_crackworker/src/lib.rs
```
pub struct ThreadWorkerFactory
impl ThreadWorkerFactory
```

### rust_pkg/_crack_utils/src/lib.rs
```
pub fn get_timestamp_now_ms() → i64
pub fn spawn(f: F) → n0_future::task::JoinHandle...
pub fn random_u32() → u32
pub async fn sleep_ms(dt_ms: u32)
```
