

## Auto-generated signatures
<!-- Updated by gen-context.js -->
You are a coding assistant with complete knowledge of this codebase.
The following code signatures were extracted by SigMap v8.9.1 on 2026-07-08T13:09:31.217Z.
<!-- sigmap: version=8.9.1 -->

These signatures represent every public function, class, and type in the project.
Refer to them when answering questions about code structure, APIs, and implementation.
## Code Signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## todos
```
packages/api_asscrack/.github/copilot-instructions.md:22  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:24  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:25  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:26  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:27  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:28  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:29  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:30  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:31  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:32  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:33  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:34  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:35  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:36  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:37  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:38  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:39  # TODO: s
packages/api_asscrack/.github/copilot-instructions.md:40  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:41  # TODO: get which is missing...
packages/api_asscrack/.github/copilot-instructions.md:42  # TODO: s
```

## crack_demo

### crack_demo/chat_cli/Cargo.toml
```
table [package]
table [dependencies]
key name
key version.workspace
key edition.workspace
key authors.workspace
key futures
```

### crack_demo/demo_resolution_selector_web_bevy/Cargo.toml
```
table [package]
table [dependencies]
table [target.'cfg(not(target_family = "wasm"))'.dependencies]
table [target.'cfg(target_family = "wasm")'.dependencies]
table [dependencies.web-sys]
table [features]
key name
key version
key authors
key edition
key bevy_egui
key rand
key rand_chacha
key tracing
key postcard
key async-channel
key optional
```

### crack_demo/demo_resolution_selector_web_bevy/index.fane.html
```
title: Crack! - Fane
```

### crack_demo/demo_resolution_selector_web_bevy/index.html
```
title: Crack! - Pantelimon
```

### crack_demo/demo_resolution_selector_web_bevy/src/egui_theme.rs
```
pub struct EguiThemePlugin
impl EguiThemePlugin
pub fn draw_notification_badge(ui: &egui::Ui, anchor: egui::Rect, count: u32)
```

### crack_demo/demo_resolution_selector_web_bevy/src/main_game_plugin.rs
```
pub struct MainGamePlugin
impl MainGamePlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs
```
pub struct SpawnCarRequestEvent
pub struct WheelAssets
pub struct Car
pub struct CarHealth
pub struct DisabledCar
pub struct NeedCarBoundsCompute
pub struct ActivePlayerVehicle
pub fn preload_wheels(mut commands: Commands, asset_server: Res<AssetServer>)
pub fn select_car_wheel(car_type: &str, wheel_assets: &WheelAssets, asset_server: &AssetServer,) → Handle<WorldAsset>
pub fn spawn_physics_car(commands: &mut Commands, asset_server: &Res<AssetServer>, wheel_assets: &Res<WheelAssets>, pos: Vec3, car_rot: Quat, car_type: &str,) → Entity
pub fn spawn_car_request_event_observer(spawn_car_event: On<SpawnCarRequestEvent>, mut commands: Commands, current_state: Res<State<GameControlState>>, mut next_state: ResMut<NextState<GameControlState>>, spatial_query: avian3d::prelude::SpatialQuery, asset_server: Res<AssetServer>, wheel_assets: Res<WheelAssets>, q_active_cars: Query<Entity, With<ActivePlayerVehicle>>,)
pub fn init_cars_system(mut commands: Commands, query: Query<(Entity, &NeedCarBoundsCompute, &Children)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/speedometer_ui.rs
```
pub fn speedometer_ui(mut contexts: EguiContexts, mut ui_state: ResMut<UiState>, mut q_car: Query< (&avian3d::prelude::LinearVelocity, &mut CarDriveState)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/geojson.rs
```
pub struct GeoJsonPlugin
pub struct GeoJsonDatabase
pub struct GeoJsonSearchState
pub struct GeoJsonSelection
pub struct GameLoadingStatus
pub struct OsmOverlayState
pub struct Bus335Marker
pub struct MovingBus
impl GeoJsonPlugin
impl OsmOverlayState
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

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_plugin_ui.rs
```
pub fn draw_tree_bboxes(_gizmos: Gizmos, _data_res: Res<MapTree>, _lod_state: Res<MapLODState>, _tiles_query: Query<&TreeMapTile>, _ui_state: Option<Res<crate::ui_egui::UiState>>,)
pub fn tree_navigator_ui(mut contexts: EguiContexts, data_res: Res<MapTree>, mut lod_state: ResMut<MapLODState>, tiles_query: Query<&TreeMapTile>, ui_state: Option<ResMut<crate::ui_egui::UiState>>,)
pub fn draw_reference_points_gizmos(mut gizmos: Gizmos, data_res: Res<MapTree>, lod_state: Res<MapLODState>, camera_query: Query<&Transform, With<Camera>>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/mod.rs
```
pub struct MapPlugin
pub struct MapTree
pub struct MapLODState
impl MapPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/global_chat_ui.rs
```
pub struct GlobalChatUiState
pub struct GlobalChatPlugin
impl GlobalChatUiState
impl GlobalChatPlugin
impl GlobalChatPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/mod.rs
```
pub struct NetworkRuntime
pub struct ChatState
pub struct NetworkPlugin
pub enum ChatEvent
impl NetworkPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs
```
pub struct GameplaySyncRoomType
pub struct GameplayPresence
pub struct GameUpdate
pub struct GameSyncChannels
pub struct GameSyncInbound
pub struct MultiplayerConfig
pub struct OutboundEvents
pub struct SeenMsgIds
pub struct RemotePlayers
pub struct RemotePlayer
pub struct RemoteAvatarMarker
pub struct MultiplayerStats
pub struct MultiplayerPlugin
pub enum GameplayChatMessageContent
pub enum PlayerStateMsg
pub enum PlayerEventMsg
pub enum RemoteAvatar
impl GameplaySyncRoomType
impl MultiplayerConfig
impl SeenMsgIds
  pub fn is_new(&mut self, id: i64) → bool
impl MultiplayerPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/notifications.rs
```
pub struct ActiveNotification
pub struct ActiveNotifications
pub struct TooltipNotificationPlugin
pub enum NotificationEvent
impl TooltipNotificationPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/debug_ui.rs
```
pub struct AiDebug
pub fn ai_debug_ui(mut contexts: EguiContexts, mut ai_debug: ResMut<AiDebug>, mut ui_state: ResMut<crate::ui_egui::UiState>, query: Query<(&Faction, &Health)
pub fn draw_ai_gizmos(ai_debug: Res<AiDebug>, mut gizmos: Gizmos, query: Query< (&GlobalTransform, &Faction, &Health, &AiPerception, &AiSteer)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs
```
pub struct AnimationInfo
pub struct PedestrianAnimations
pub struct PedestrianAnimationControlEvent
pub struct ManualAnimation
pub struct ActiveOneShot
pub struct TargetAnimation
pub struct CurrentPlayingAnimation
impl PedestrianAnimations
  pub fn default_animation(&self) → Option<String>
pub fn pedestrian_animation_control_observer(trigger: On<PedestrianAnimationControlEvent>, mut commands: Commands, mut targets: Query<&mut TargetAnimation>,)
pub fn setup_animation_players_system(mut commands: Commands, anims: Res<PedestrianAnimations>, players: Query<Entity, (With<AnimationPlayer>, Without<AnimationGraphHandle>)
pub fn play_animations_system(mut commands: Commands, anims: Res<PedestrianAnimations>, gltf_assets: Res<Assets<bevy::gltf::Gltf>>, model_roots: Query< (&PedestrianGltf, Option<&TargetAnimation>)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/manifest.rs
```
pub struct PedestrianUrl
pub struct PedestrianManifest
pub struct PedestrianManifestTasks
pub struct TextAsset
pub struct TextAssetLoader
impl TextAssetLoader
pub fn start_manifest_load(mut commands: Commands)
pub fn spawn_pedestrian_manifest_task(mut tasks: ResMut<PedestrianManifestTasks>, manifest: Res<PedestrianManifest>, client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,)
pub fn poll_pedestrian_manifest_task(mut tasks: ResMut<PedestrianManifestTasks>, mut manifest: ResMut<PedestrianManifest>, client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,)
pub fn poll_pedestrian_first_glb_task(mut tasks: ResMut<PedestrianManifestTasks>, memory_dir: ResMut<MemoryDir>, asset_server: Res<AssetServer>,)
pub fn load_pedestrian_manifest_system(mut bootstrap: ResMut<PedestrianManifestTasks>, mut manifest: ResMut<PedestrianManifest>, mut anims: ResMut<PedestrianAnimations>, gltf_assets: Res<Assets<bevy::gltf::Gltf>>, clip_assets: Res<Assets<AnimationClip>>, mut graphs: ResMut<Assets<AnimationGraph>>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/mod.rs
```
pub struct PedestriansPlugin
impl PedestriansPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs
```
pub struct ControlledCharacter
pub struct SpawnChoicePopup
pub struct SpawnControlledPedestrianEvent
pub fn spawn_controlled_pedestrian_observer(trigger: On<SpawnControlledPedestrianEvent>, mut commands: Commands, manifest: Res<PedestrianManifest>, mut controlled: ResMut<ControlledCharacter>, mut next_state: ResMut<NextState<GameControlState>>,)
pub fn player_death_to_freecam(mut commands: Commands, mut controlled: ResMut<ControlledCharacter>, q_newly_dying: Query<()
pub fn escape_to_freecam(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut controlled: ResMut<ControlledCharacter>, mut next_state: ResMut<NextState<GameControlState>>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/spawn_pedestrian.rs
```
pub struct SpawnPedestrianEvent
pub struct ModelRoot
pub struct PedestrianGltf
pub struct NeedAlignment
pub struct ModelController
pub struct PedestrianSpawnCounter
pub struct PendingPedestrianGlbFetch
pub fn spawn_pedestrian_observer(trigger: On<SpawnPedestrianEvent>, mut commands: Commands, client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,)
pub fn poll_pedestrian_glb_fetches(mut commands: Commands, mut q_fetches: Query<(Entity, &mut PendingPedestrianGlbFetch)
pub fn link_pedestrian_model(mut commands: Commands, mut controlled: Option< ResMut<crate::plugins::pedestrians::pedestrian_controller_plugin::ControlledCharacter>, >, q_models: Query<(Entity, &ModelController)
pub fn init_pedestrians_system(mut commands: Commands, query: Query<(Entity, &NeedAlignment, &Children)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/states/mod.rs
```
pub struct GameStatesPlugin
pub enum InitialMapLoadFinished
pub enum OsmDatabaseLoadFinished
pub enum SoundManifestLoadFinished
pub enum GameControlState
pub enum NetworkConnectionState
impl GameStatesPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/mod.rs
```
pub struct TrafficConfig
pub struct TrafficCar
pub struct SpawnTrafficCarEvent
pub struct TrafficPedestrian
pub struct SpawnTrafficPedestrianEvent
pub struct TrafficPlugin
pub enum TrafficDriveMode
impl TrafficConfig
impl TrafficPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/spawn.rs
```
pub fn get_ground_y(pos: Vec3, map_tree: Option<&MapTree>, spatial_query: &avian3d::prelude::SpatialQuery,) → f32
pub fn traffic_network_spawner(time: Res<Time>, mut last_spawn: Local<f32>, config: Res<TrafficConfig>, graph: Res<TrafficRoadGraph>, q_camera: Query<(&Camera, &GlobalTransform)
pub fn spawn_traffic_car_observer(trigger: On<SpawnTrafficCarEvent>, mut commands: Commands, graph: Res<TrafficRoadGraph>, asset_server: Res<AssetServer>, map_tree: Option<Res<MapTree>>, spatial_query: avian3d::prelude::SpatialQuery, manifest: Option<Res<PedestrianManifest>>, wheel_assets: Res<WheelAssets>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/mod.rs
```
pub struct WeaponsPlugin
impl WeaponsPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs
```
pub struct EquippedWeapon
pub struct EquipWeaponEvent
pub struct WeaponGripOffset
pub struct WeaponModelState
pub struct WeaponModel
pub struct PendingWeaponExtents
pub struct WeaponExtents
pub struct PendingWeaponModelFetch
pub enum WeaponKind
impl WeaponGripOffset
pub fn equip_weapon_observer(trigger: On<EquipWeaponEvent>, mut commands: Commands, transforms: Query<&GlobalTransform>,)
pub fn reconcile_weapon_model(mut commands: Commands, client: Option<Res<crate::plugins::crack_plugin::CrackClient>>, mut characters: Query<(Entity, &EquippedWeapon, Option<&mut WeaponModelState>)
pub fn poll_weapon_model_fetches(mut commands: Commands, mut q_fetches: Query<(Entity, &mut PendingWeaponModelFetch)
pub fn finalize_weapon_extents(mut commands: Commands, pending: Query<(Entity, &Children)
pub fn update_weapon_transforms(grip: Res<WeaponGripOffset>, camera: Query<&GlobalTransform, With<Camera3d>>, spatial: SpatialQuery, parents: Query<&ChildOf>, global_transforms: Query<&GlobalTransform>, mut weapons: Query<(Entity, &mut Transform, &WeaponKind)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_manifest.rs
```
pub struct GunInfo
pub struct WeaponManifest
pub struct WeaponManifestTasks
pub enum WeaponId
impl WeaponId
  pub fn is_unarmed(&self) → bool
  pub fn is_gun(&self) → bool
  pub fn is_melee(&self) → bool
  pub fn path(&self) → Option<&str>
  pub fn gun_info(&self) → Option<&GunInfo>
  pub fn label(&self) → String
  pub fn from_label(label: &str, manifest: &WeaponManifest) → Self
pub fn start_weapon_manifest_load(mut commands: Commands)
pub fn spawn_weapon_manifest_task(mut tasks: ResMut<WeaponManifestTasks>, manifest: Res<WeaponManifest>, client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,)
pub fn poll_weapon_manifest_task(mut tasks: ResMut<WeaponManifestTasks>, mut manifest: ResMut<WeaponManifest>,)
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

### crack_demo/game_logic/src/worker/http.rs
```
pub async fn http_get_bytes(url: &str) → anyhow::Result<bytes::Bytes>
pub async fn http_get_text(url: &str) → anyhow::Result<String>
pub async fn http_get_bytes(url: &str) → anyhow::Result<bytes::Bytes>
pub async fn http_get_text(url: &str) → anyhow::Result<String>
```

### crack_demo/game_logic/src/worker/lru.rs
```
pub struct LruCache
impl LruCache
  pub fn new(max_entries: usize) → Self
  pub fn get(&mut self, key: &str) → Option<T>
  pub fn insert(&mut self, key: String, val: T)
```

### crack_demo/game_logic/src/worker/manifest_impl.rs
```
pub async fn get_manifest_cache() → anyhow::Result<Arc<MapTreeD...
pub async fn fetch_map_manifest(args: FetchArgs) → anyhow::Result<MapManifestR...
```

### crack_demo/game_logic/src/worker/osm_impl.rs
```
pub async fn fetch_osm_data(args: FetchArgs) → anyhow::Result<OsmDataResult>
```

### crack_demo/game_logic/src/worker/pedestrian_impl.rs
```
pub async fn fetch_pedestrian_manifest(args: FetchArgs,) → anyhow::Result<PedestrianMa...
pub async fn fetch_pedestrian_model(req: FetchGlbRequest) → anyhow::Result<FetchGlbResp...
```

### crack_demo/game_logic/src/worker/tile_impl.rs
```
pub async fn fetch_map_tile(req: FetchTileRequest) → anyhow::Result<FetchTileRes...
```

### crack_demo/game_logic/src/worker/weapon_impl.rs
```
pub async fn fetch_weapon_manifest(args: FetchArgs) → anyhow::Result<WeaponManife...
pub async fn fetch_weapon_model(req: FetchGlbRequest) → anyhow::Result<FetchGlbResp...
```

### crack_demo/thread_worker/Cargo.toml
```
table [package]
table [dependencies]
table [lints]
key name
key version.workspace
key authors.workspace
key edition.workspace
key rand
key n0-future
key workspace
```

### crack_demo/thread_worker/src/lib.rs
```
pub fn make_registered_mapping() → Arc<ApiImplMapping>
pub async fn spawn_in_process_worker() → anyhow::Result<WorkerPipe>
pub async fn run_bootstrap_if_needed() → anyhow::Result<()>
```

### crack_demo/web_worker/Cargo.toml
```
table [package]
table [lib]
table [dependencies]
table [lints]
key name
key version.workspace
key authors.workspace
key edition.workspace
key rand
key n0-future
key workspace
```

### crack_demo/web_worker/src/lib.rs
```
pub async fn run_bootstrap_if_needed() → anyhow::Result<()>
```

### crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs
```
pub struct MemoryDir
pub fn make_basic_app(title: &str) → App
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/car_info.rs
```
pub fn get_car_asset(car_type: &str, asset_server: &AssetServer) → Handle<WorldAsset>
pub fn get_wheel_asset(wheel_name: &str, asset_server: &AssetServer) → Handle<WorldAsset>
pub fn car_list() → &'static [&'static str]
pub fn get_random_car_type() → &'static str
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
pub fn spawn_lod_task(map_tree: Res<MapTree>, lod_state: Res<MapLODState>, q_merge: Query<&TileShouldMerge>, q_split: Query<&TileShouldSplit>, q_pending: Query<&PendingTileReveal>, q_nodes: Query<&TreeMapTile>, mut last: Local<Option<(BTreeSet<MapTreeNodePath>, Vec<Vec3>, u32)
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

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/combat.rs
```
pub struct DamageEvent
pub fn apply_damage_observer(trigger: On<DamageEvent>, mut commands: Commands, mut healths: Query<(&mut Health, Has<Dying>)
pub fn start_ai_death_animation(mut commands: Commands, newly_dead: Query<&AiModel, Added<Dying>>)
pub fn tick_dying(mut commands: Commands, time: Res<Time>, mut query: Query<(Entity, &mut Dying, Option<&mut LinearVelocity>)
pub fn ai_combat(mut commands: Commands, spatial_query: SpatialQuery, mut tracers: ResMut<ShotTracers>, mut sparks: ResMut<BulletSparks>, mut query: Query< ( Entity, &GlobalTransform, &AiState, &AiPerception, &AiThink, &mut AiCombatTimers, Option<&EquippedWeapon>, Option<&mut GunState>, Option<&WeaponModelState>, Option<&AiModel>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/faction.rs
```
pub struct WarMatrix
pub struct Health
pub struct Enemies
pub struct Dying
pub enum Faction
impl Faction
  pub fn color(self) → Color
  pub fn label(self) → &'static str
impl WarMatrix
  pub fn all_out_war() → Self
  pub fn gang_wars() → Self
  pub fn at_war(&self, a: Faction, b: Faction) → bool
impl Health
  pub fn full(max: f32) → Self
impl Enemies
  pub fn insert(&mut self, who: Entity)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/mod.rs
```
pub struct PedestrianDied
pub struct AiPedestrian
pub struct AiPerception
pub struct AiCombatTimers
pub struct AiSteer
pub struct AiModel
pub struct AiAnim
pub struct AiThink
pub struct PedestrianAiPlugin
pub enum AiState
impl AiThink
impl PedestrianAiPlugin
pub fn tick_ai_think(time: Res<Time>, mut q: Query<&mut AiThink>)
pub fn prune_enemies_on_death(mut deaths: MessageReader<PedestrianDied>, mut q_enemies: Query<&mut Enemies>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/movement_ai.rs
```
pub fn ai_movement(spatial_query: SpatialQuery, ai_debug: Res<AiDebug>, mut query: Query< ( Entity, &GlobalTransform, &AiState, &AiPerception, &mut AiCombatTimers, &mut AiSteer, &mut LocomotionInput, &mut MovementModifiers, &super::faction::Health, Option<&EquippedWeapon>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/perception.rs
```
pub fn ai_perception(spatial_query: SpatialQuery, war: Res<WarMatrix>, mut ai_query: Query< ( Entity, &GlobalTransform, &Faction, &Enemies, &AiThink, &mut AiPerception,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/spawn_ai.rs
```
pub struct SpawnAiPedestrianEvent
pub fn spawn_ai_pedestrian_observer(trigger: On<SpawnAiPedestrianEvent>, mut commands: Commands, manifest: Res<PedestrianManifest>, weapon_manifest: Res<WeaponManifest>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs
```
pub fn print_animation_catalog(anims: Res<PedestrianAnimations>, mut done: Local<bool>)
pub fn drive_character_animation(time: Res<Time>, anims: Res<PedestrianAnimations>, controlled: Res<ControlledCharacter>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut contexts: EguiContexts, mut controllers: Query< ( &LinearVelocity, Has<Grounded>, &MovementModifiers, &CharacterScale, Has<Climbing>, Has<Rolling>, Option<&EquippedWeapon>, Option<&GunState>, &mut AnimState, &mut CombatState, Option<&EnteringCarTimer>, Option<&WeaponModelState>, &GlobalTransform, Has<Dying>, Option<&super::EjectedDriver>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs
```
pub fn character_input(keys: Res<ButtonInput<KeyCode>>, camera: Query<&GlobalTransform, With<Camera3d>>, controlled: Res< crate::plugins::pedestrians::pedestrian_controller_plugin::spawn::ControlledCharacter, >, mut query: Query<(&mut LocomotionInput, &mut MovementModifiers)
pub fn update_grounded(mut commands: Commands, mut query: Query<(Entity, &GroundDetection, &GlobalTransform)
pub fn movement(time: Res<Time>, mut controllers: Query<( &mut LocomotionInput, &CharacterMovementSettings, &mut LinearVelocity, Has<Grounded>,)
pub fn apply_gravity(time: Res<Time>, mut controllers: Query<(&CharacterMovementSettings, &mut LinearVelocity)
pub fn apply_movement_damping(mut query: Query<(&CharacterMovementSettings, &mut LinearVelocity)
pub fn apply_speed_cap(time: Res<Time>, mut query: Query<(&mut MovementModifiers, &mut LinearVelocity, Has<Rolling>)
pub fn move_and_slide(mut query: Query< ( Entity, Option<&GroundDetection>, Option<&mut CharacterCollisions>, &mut Transform, &mut LinearVelocity, &Collider,)
pub fn apply_forces_to_dynamic_bodies(characters: Query<(&ComputedMass, &CharacterCollisions)
pub fn face_movement(time: Res<Time>, mut query: Query<(&LinearVelocity, &mut Transform)
pub fn respawn_if_fallen(mut query: Query<(&mut Transform, &mut LinearVelocity)
pub fn jump_or_climb(keys: Res<ButtonInput<KeyCode>>, spatial_query: SpatialQuery, mut commands: Commands, controlled: Res< crate::plugins::pedestrians::pedestrian_controller_plugin::spawn::ControlledCharacter, >, map: Option<Res<MapTree>>, tiles: Query<()
pub fn update_climb(time: Res<Time>, mut commands: Commands, mut query: Query<(Entity, &mut Transform, &mut LinearVelocity, &mut Climbing)
pub fn update_roll(time: Res<Time>, mut commands: Commands, mut query: Query<(Entity, &Transform, &mut LinearVelocity, &mut Rolling)
pub fn detect_fallen_off_map(map: Option<Res<MapTree>>, tiles: Query<()
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs
```
pub struct CarSeatOffset
pub struct EnteringCarTimer
pub struct DriverMesh
pub struct DriverMeshExit
pub struct EjectedDriver
pub struct WeaponSelection
pub enum EjectedStage
impl CarSeatOffset
pub fn handle_freecam_right_click(mouse_button: Res<ButtonInput<MouseButton>>, window_query: Query<&Window>, camera_query: Query<(&Camera, &GlobalTransform)
pub fn spawn_choice_popup_ui(mut commands: Commands, mut contexts: EguiContexts, mut popup: ResMut<SpawnChoicePopup>,)
pub fn detect_car_interaction(keys: Res<ButtonInput<KeyCode>>, q_player: Query< (Entity, &GlobalTransform)
pub fn tick_entering_car(mut commands: Commands, time: Res<Time>, mut q_player: Query<( Entity, &mut EnteringCarTimer, &mut Transform, &CharacterScale,)
pub fn tick_ejected_driver_system(mut commands: Commands, time: Res<Time>, mut q_ejected: Query<(Entity, &mut EjectedDriver)
pub fn eject_driver_as_ai(commands: &mut Commands, car_gt: &GlobalTransform, driver_mesh_entity: Entity, driver_faction: Faction, driver_health: Health, scale: f32,)
pub fn drive_driver_mesh_animation(anims: Res<PedestrianAnimations>, mut q_driver: Query<(Entity, &mut DriverMesh, Has<DriverMeshExit>)
pub fn apply_seat_offset(seat: Res<CarSeatOffset>, mut q_driver: Query<&mut Transform, (With<DriverMesh>, Without<DriverMeshExit>)
pub fn car_seat_debug_ui(mut contexts: EguiContexts, mut seat: ResMut<CarSeatOffset>, q_driver: Query<()
pub fn handle_exit_car(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, q_active_car: Query<(Entity, &GlobalTransform)
pub fn tick_driver_mesh_exit(mut commands: Commands, time: Res<Time>, mut q_exit: Query<(Entity, &mut Transform, &mut DriverMeshExit)
pub fn weapon_hud_ui(mut contexts: EguiContexts, controlled: Res<ControlledCharacter>, equipped: Query<(&EquippedWeapon, Option<&GunState>, &Health)
pub fn equip_on_new_character(mut commands: Commands, controlled: Res<ControlledCharacter>, manifest: Res<WeaponManifest>, mut selection: ResMut<WeaponSelection>, mut last: Local<Option<Entity>>,)
pub fn weapon_wheel(mut commands: Commands, mut wheel: MessageReader<MouseWheel>, mut contexts: EguiContexts, controlled: Res<ControlledCharacter>, manifest: Res<WeaponManifest>, mut selection: ResMut<WeaponSelection>,)
pub fn crosshair_ui(mut contexts: EguiContexts, controlled: Res<ControlledCharacter>, guns: Query<&GunState>, state: Res<State<GameControlState>>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/locomotion.rs
```
pub struct CharacterLocomotionPlugin
impl CharacterLocomotionPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs
```
pub struct LocomotionInput
pub struct CharacterController
pub struct CharacterScale
pub struct MovementModifiers
pub struct CharacterMovementSettings
pub struct GroundDetection
pub struct Grounded
pub struct Climbing
pub struct Rolling
pub struct CharacterCollisions
pub struct CharacterCollision
pub struct AnimState
pub struct CombatState
pub struct PedestrianControllerPlugin
pub enum JumpPhase
pub enum CombatKind
impl CharacterMovementSettings
impl GroundDetection
impl AnimState
impl PedestrianControllerPlugin
pub fn no_one_climbing(q: Query<() → bool
pub fn character_physics_bundle(scale: f32, transform: Transform) → impl Bundle
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/common.rs
```
pub struct TrafficAgentState
impl TrafficAgentState
  pub fn new(path: Vec<Vec3>, current_seg: usize) → Self
pub fn walk_up_to_root(hit_entity: Entity, root_entity: Entity, q_parent: &Query<&ChildOf>,) → bool
pub fn update_visibility(camera: &Camera, cam_gt: &GlobalTransform, spatial_query: &SpatialQuery, root_entity: Entity, probe_point: Vec3, q_parent: &Query<&ChildOf>,) → bool
pub fn should_despawn(dist_to_camera: f32, spawn_radius: f32, state: &TrafficAgentState) → bool
pub fn build_path_from(graph: &TrafficRoadGraph, pos: Vec3,) → Option<( usize, Vec<Vec3>, )>
pub fn pick_spawn_candidate(graph: &TrafficRoadGraph, camera: &Camera, cam_gt: &GlobalTransform, radius: f32, min_dist: f32, spacing: f32, existing: &[Vec3], fast_fill: bool,) → Option<Vec3>
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/debug_ui.rs
```
pub fn traffic_debug_ui(mut contexts: EguiContexts, mut config: ResMut<TrafficConfig>, ui_state: Option<ResMut<UiState>>, q_traffic: Query<Entity, With<TrafficCar>>, q_traffic_peds: Query<Entity, With<TrafficPedestrian>>, graph: Res<TrafficRoadGraph>, q_camera: Query<&GlobalTransform, With<Camera3d>>, mut commands: Commands,)
pub fn draw_traffic_gizmos(mut gizmos: Gizmos, graph: Res<TrafficRoadGraph>, config: Res<TrafficConfig>, q_cars: Query<(&Transform, &TrafficCar)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/despawn.rs
```
pub fn despawn_traffic_cars(time: Res<Time>, config: Res<TrafficConfig>, mut q_cars: Query<(Entity, &Transform, &CarDriveState, &mut TrafficCar)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/driver.rs
```
pub fn drive_traffic_cars(time: Res<Time>, config: Res<TrafficConfig>, graph: Res<TrafficRoadGraph>, mut q_cars: Query< ( Entity, &Transform, &LinearVelocity, &mut CarDriveState, &mut TrafficCar,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/pedestrian_traffic.rs
```
pub struct PendingTrafficPeds
pub struct PendingTrafficPedEntry
pub fn traffic_pedestrian_spawner(time: Res<Time>, mut last_spawn: Local<f32>, config: Res<TrafficConfig>, graph: Res<TrafficRoadGraph>, q_camera: Query<(&Camera, &GlobalTransform)
pub fn spawn_traffic_pedestrian_observer(trigger: On<SpawnTrafficPedestrianEvent>, mut commands: Commands, graph: Res<TrafficRoadGraph>, map_tree: Option<Res<MapTree>>, spatial_query: avian3d::prelude::SpatialQuery, mut pending_traffic: ResMut<PendingTrafficPeds>,)
pub fn adopt_traffic_pedestrians(mut commands: Commands, mut pending: ResMut<PendingTrafficPeds>, q_new_ai: Query<(Entity, &Transform)
pub fn drive_traffic_pedestrians(time: Res<Time>, graph: Res<TrafficRoadGraph>, mut q_peds: Query<( Entity, &GlobalTransform, &AiState, &mut TrafficPedestrian, &mut LocomotionInput,)
pub fn despawn_traffic_pedestrians(time: Res<Time>, config: Res<TrafficConfig>, mut q_peds: Query<(Entity, &Transform, &mut TrafficPedestrian)
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/road_graph.rs
```
pub struct TrafficRoadGraph
pub struct RoadSegment
pub enum RerouteMode
pub fn quantize(p: Vec3) → IVec2
pub fn build_road_graph(database: Res<crate::plugins::geojson::GeoJsonDatabase>, mut graph: ResMut<TrafficRoadGraph>,)
pub fn pick_continuation(graph: &TrafficRoadGraph, node: IVec2, from_seg: usize, mode: RerouteMode,) → Option<(usize, Vec<Vec3>)>
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs
```
pub struct GunState
pub struct FireGunEvent
pub struct ReloadGunEvent
pub struct ShotTracer
pub struct ShotTracers
pub struct BulletSpark
pub struct BulletSparks
pub struct PendingMeleeHit
pub fn fire_gun_observer(trigger: On<FireGunEvent>, mut shooters: Query<(&mut GunState, &EquippedWeapon, Option<&WeaponModelState>)
pub fn reload_gun_observer(trigger: On<ReloadGunEvent>, mut shooters: Query<(&mut GunState, &GlobalTransform)
pub fn draw_shot_tracers(time: Res<Time>, mut gizmos: Gizmos, mut tracers: ResMut<ShotTracers>)
pub fn draw_bullet_sparks(time: Res<Time>, mut gizmos: Gizmos, mut sparks: ResMut<BulletSparks>)
pub fn tick_pending_melee_hits(mut commands: Commands, time: Res<Time>, mut query: Query<(Entity, &GlobalTransform, &mut PendingMeleeHit)
```

### crack_demo/game_logic/Cargo.toml
```
table [package]
table [features]
table [dependencies]
table [target.'cfg(target_family = "wasm")'.dependencies]
key name
key version.workspace
key edition.workspace
key authors.workspace
```

### crack_demo/game_logic/src/api.rs
```
pub struct FetchArgs
```

### crack_demo/game_logic/src/geo.rs
```
pub struct GeoBBox
pub struct ProjectionRef
impl GeoBBox
  pub fn contains(&self, lat: f64, lon: f64) → bool
pub fn octant_path_to_geobbox(path: &str) → Option<GeoBBox>
pub fn find_tile_for_lat_lon(lat: f64, lon: f64, map_tree: &'a MapTreeData,) → Option<&'a MapTreeNodeInfo>
pub fn get_enu_rotation_matrix(ref_point: Vec3) → [Vec3
pub fn lat_lon_to_ecef(lat_deg: f32, lon_deg: f32) → Vec3
pub fn lat_lon_to_bevy(lat_deg: f32, lon_deg: f32, ref_point: Vec3, rot_matrix: &[Vec3; 3],) → Vec3
pub fn parse_bbox_from_txt(text: &str) → Option<(f32, f32)>
pub fn project_point(lat: f64, lon: f64, map_tree: &MapTreeData, coord_res: &ProjectionRef,) → Vec3
```

### crack_demo/game_logic/src/glb.rs
```
pub struct FetchGlbRequest
pub struct FetchGlbResponse
```

### crack_demo/game_logic/src/lod.rs
```
pub struct Score
pub struct LodComputeRequest
pub struct SplitRequestSummary
pub struct MergeRequestSummary
pub struct LodComputeResponse
impl Score
impl Score
pub fn compute_distance_to_aabb(bbox: &BBox, p: Vec3) → f32
pub fn compute_lod_changes(data_res: &MapTreeData, req: &LodComputeRequest) → LodComputeResponse
```

### crack_demo/game_logic/src/map.rs
```
pub struct BBox
pub struct MapTileAssetId
pub struct MapTreeNodePath
pub struct MapTreeAssetInfo
pub struct MapTreeNodeInfo
pub struct MapTreeData
pub struct MapTileAssetInfoSummary
pub struct MapRootNodeSummary
pub struct MapManifestResult
impl MapTileAssetId
  pub fn get_octant_path(&self) → MapTreeNodePath
impl MapTreeNodePath
  pub fn get_parent(&self) → Option<MapTreeNodePath>
```

### crack_demo/game_logic/src/pedestrian.rs
```
pub struct AnimationMeta
pub struct PedestrianManifestResult
```

### crack_demo/game_logic/src/tile.rs
```
pub struct MeshColliderData
pub struct FetchTileRequest
pub struct FetchTileResponse
```

### crack_demo/game_logic/src/weapon.rs
```
pub struct WeaponEntry
pub struct WeaponManifestResult
```

### crack_demo/game_logic/src/worker/models.rs
```
pub async fn run_game_migrations(_: () → anyhow::Result<()>
```

## packages

### packages/_crack_utils/.github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/_crack_utils/.github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/_crack_utils/AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/_crack_utils/CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/api_asscrack/.github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api\api_client.rs
h3 src\api\api_method_macros.rs
h3 src\api\api_worker_declarations.rs
h3 src\crack_worker\api_worker.rs
h3 src\crack_worker\mod.rs
code-fence plain
```

### packages/api_asscrack/.github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api\api_client.rs
h3 src\api\api_method_macros.rs
h3 src\api\api_worker_declarations.rs
h3 src\crack_worker\api_worker.rs
h3 src\crack_worker\mod.rs
code-fence plain
```

### packages/api_asscrack/AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api\api_client.rs
h3 src\api\api_method_macros.rs
h3 src\api\api_worker_declarations.rs
h3 src\crack_worker\api_worker.rs
h3 src\crack_worker\mod.rs
code-fence plain
```

### packages/api_asscrack/CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api\api_client.rs
h3 src\api\api_method_macros.rs
h3 src\api\api_worker_declarations.rs
h3 src\crack_worker\api_worker.rs
h3 src\crack_worker\mod.rs
code-fence plain
```

### packages/consensus_crackhead/.github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
code-fence plain
```

### packages/consensus_crackhead/.github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
code-fence plain
```

### packages/consensus_crackhead/AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
code-fence plain
```

### packages/consensus_crackhead/CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
code-fence plain
```

### packages/game/Cargo.toml
```
table [package]
table [dependencies]
key name
key version
key edition
```

### packages/net_crackpipe/Cargo.toml
```
table [package]
table [dependencies]
table [target.'cfg(target_family = "wasm")'.dependencies]
key name
key version.workspace
key authors.workspace
key edition
key anyhow
key hex
key n0-future
key postcard
key rand
key tracing
key async-channel
key async-broadcast
key chrono
key web-time
key futures
key serde_json
key async-trait
key paste
```

### packages/net_crackpipe/src/_random_word.rs
```
pub fn get_nickname_from_pubkey(pubkey: PublicKey) → String
```

### packages/net_crackpipe/src/api/api_declarations.rs
```
pub struct MatchRow2
pub struct GameStateRow2
pub struct FriendInfo
pub struct UserProfileListItem
```

### packages/net_crackpipe/src/api/api_method_macros.rs
```
pub struct ServerInfo
pub struct ApiMethodImpl
pub trait ApiMethod
```

### packages/net_crackpipe/src/api/client_api_manager.rs
```
pub struct ClientApiManager
impl ClientApiManager
impl ClientApiManager
  pub async fn call_method(&self, arg: M::Arg) → anyhow::Result<M::Ret>
pub async fn connect_api_manager(mm: GlobalMatchmaker) → anyhow::Result<ClientApiMan...
```

### packages/net_crackpipe/src/api/join_chat.rs
```
pub struct ServerChatRoomType
pub struct ServerChatPresence
pub enum ServerChatMessageContent
impl ServerChatRoomType
pub async fn server_join_server_chat(mm: GlobalMatchmaker,) → anyhow::Result<ChatControll...
```

### packages/net_crackpipe/src/chat/chat_const.rs
```
pub fn get_relay_domain() → (String, String)
```

### packages/net_crackpipe/src/chat/chat_controller.rs
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

### packages/net_crackpipe/src/chat/chat_presence.rs
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

### packages/net_crackpipe/src/chat/chat_ticket.rs
```
pub struct ChatTicket
impl ChatTicket
  pub fn new_str_bs(topic_id: &str, bs: BTreeSet<NodeId>) → Self
```

### packages/net_crackpipe/src/chat/direct_message.rs
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

### packages/net_crackpipe/src/chat/global_chat.rs
```
pub struct GlobalChatRoomType
pub struct GlobalChatPresence
pub enum GlobalChatMessageContent
pub enum GlobalChatBootstrapQuery
pub enum MatchHandshakeType
impl GlobalChatRoomType
```

### packages/net_crackpipe/src/chat/room_raw.rs
```
pub struct GossipChatRoom
impl GossipChatRoom
  pub async fn new(node: &MainNode, ticket: &ChatTicket) → Result<Self>
impl GossipChatRoom
```

### packages/net_crackpipe/src/echo.rs
```
pub struct Echo
impl Echo
  pub fn new(own_endpoint_node_id: NodeId, sleep_manager: SleepManager) → Self
impl Echo
impl Echo
```

### packages/net_crackpipe/src/global_matchmaker.rs
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

### packages/net_crackpipe/src/lib.rs
```
pub fn timestamp_micros() → u128
pub fn datetime_now() → DateTime<Utc>
```

### packages/net_crackpipe/src/main_node.rs
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

### packages/net_crackpipe/src/signed_message.rs
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

### packages/net_crackpipe/src/sleep.rs
```
pub struct SleepManager
impl SleepManager
  pub fn new() → Self
  pub async fn sleep(&self, duration: Duration)
  pub fn wake_up(&self)
impl SleepManagerInner
```

### packages/net_crackpipe/src/user_identity.rs
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

### packages/storage_crackhouse/.github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api.rs
h3 src\impl_rusqulite.rs
h3 src\lib.rs
h3 src\models.rs
h3 src\types.rs
code-fence plain
```

### packages/storage_crackhouse/.github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api.rs
h3 src\impl_rusqulite.rs
h3 src\lib.rs
h3 src\models.rs
h3 src\types.rs
code-fence plain
```

### packages/storage_crackhouse/AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api.rs
h3 src\impl_rusqulite.rs
h3 src\lib.rs
h3 src\models.rs
h3 src\types.rs
code-fence plain
```

### packages/storage_crackhouse/CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 todos
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\api.rs
h3 src\impl_rusqulite.rs
h3 src\lib.rs
h3 src\models.rs
h3 src\types.rs
code-fence plain
```

### packages/thread_crackworker/.github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/thread_crackworker/.github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/thread_crackworker/AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/thread_crackworker/CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
code-fence plain
```

### packages/web_serviceworker_crackloader/.github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackloader/.github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackloader/AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackloader/CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackloader/src/lib.rs
```
pub struct WebWorkerFactory
impl WebWorkerFactory
```

### packages/web_serviceworker_crackslave/.github/copilot-instructions.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .cargo
h3 .cargo\cargo.toml
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackslave/.github/gemini-context.md
```
h2 Auto-generated signatures
h2 Code Signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .cargo
h3 .cargo\cargo.toml
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackslave/AGENTS.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .cargo
h3 .cargo\cargo.toml
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackslave/CLAUDE.md
```
h2 Auto-generated signatures
h1 Code signatures
h2 SigMap commands
h2 .
h3 AGENTS.md
h3 Cargo.toml
h3 CLAUDE.md
h2 .cargo
h3 .cargo\cargo.toml
h2 .github
h3 .github\copilot-instructions.md
h3 .github\gemini-context.md
h2 src
h3 src\lib.rs
h3 src\old.rs
code-fence plain
```

### packages/web_serviceworker_crackslave/src/lib.rs
```
pub async fn _js_init_dedicated_worker() → Result<(), JsValue>
pub async fn _js_compute_payload_reply(msg: JsValue) → Result<JsValue, JsValue>
pub async fn web_worker_registration(mapping: Arc<ApiImplMapping>,) → std::result::Result<(), JsV...
```

### packages/api_asscrack/src/api/api_client.rs
```
pub struct ApiClient
pub struct MessageLater
impl ApiClient
  pub fn new(pipe: WorkerPipe) → Self
  pub async fn call(&self, arg: T::Arg) → anyhow::Result<T::Ret>
```

### packages/api_asscrack/src/api/api_method_macros.rs
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

### packages/web_serviceworker_crackslave/Cargo.toml
```
table [package]
table [dependencies]
table [dependencies.web-sys]
table [lints]
key name
key version.workspace
key authors.workspace
key edition.workspace
key anyhow.workspace
key thiserror.workspace
key tracing.workspace
key dioxus-logger
key serde.workspace
key serde-wasm-bindgen
key workspace
```
