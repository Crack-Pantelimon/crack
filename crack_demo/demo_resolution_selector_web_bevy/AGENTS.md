

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

## todos
```
src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs:439  # TODO: (upper/lower body decoupling): instead of snapping the whole controlle
```

## src

### src/bin/car_sim.rs
```
impl SimLogTimer
```

### src/main_game_plugin.rs
```
pub struct MainGamePlugin
impl MainGamePlugin
```

### src/plugins/audio/audio_fx.rs
```
pub struct AudioFxEvent
pub struct EngineSoundEmitter
pub struct FootstepEmitter
pub enum AudioFxEventType
pub fn audio_fx_observer(trigger: On<AudioFxEvent>, manifest: Res<SoundManifest>, mut commands: Commands,)
pub fn spawn_car_engine_sounds(mut commands: Commands, query: Query< Entity, ( With<crate::plugins::cars_driving::driving_plugin::spawn_car::Car>, Without<EngineSoundEmitter>,)
pub fn manage_car_engine_sound_pitch_volume(query: Query<(&CarDriveState, &EngineSoundEmitter)
pub fn manage_footsteps_system(mut commands: Commands, query: Query< ( Entity, &LinearVelocity, Has<Grounded>, Option<&FootstepEmitter>,)
```

### src/plugins/audio/mod.rs
```
pub struct SoundEntry
pub struct SoundManifest
pub struct PlaySoundEvent
pub struct GameAudioPlugin
pub struct AudioDemoState
pub struct AudioDemoPlugin
impl SoundManifest
  pub fn get(&self, name: &str) → Option<&SoundEntry>
impl GameAudioPlugin
impl AudioDemoState
impl AudioDemoPlugin
```

### src/plugins/cars_driving/driving_plugin/camera_follow.rs
```
pub struct DrivingAim
pub fn update_driving_aim(mouse: Res<ButtonInput<MouseButton>>, mut contexts: EguiContexts, capture_state: Res<crate::plugins::states::MouseCaptureState>, mut aim: ResMut<DrivingAim>,)
pub fn camera_follows_car(time: Res<Time>, mut camera_query: Query<&mut Transform, (With<MainCamera>, Without<ActivePlayerVehicle>)
```

### src/plugins/cars_driving/driving_plugin/mod.rs
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

### src/plugins/cars_driving/driving_plugin/spawn_car.rs
```
pub struct CarPassenger
pub struct SpawnCarPassenger
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

### src/plugins/crack_plugin/lod_flow.rs
```
pub struct CameraKinematics
pub fn track_camera_kinematics(time: Res<Time>, q_camera: Query< &GlobalTransform, With<crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera>, >, mut kin: ResMut<CameraKinematics>,)
pub fn spawn_lod_task(map_tree: Res<MapTree>, lod_state: Res<MapLODState>, q_merge: Query<&TileShouldMerge>, q_split: Query<&TileShouldSplit>, q_pending: Query<&PendingTileReveal>, q_nodes: Query<&TreeMapTile>, mut last: Local< Option<( BTreeSet<MapTreeNodePath>, Vec<Vec3>, u32, bool, i32, (u32, u32)
pub fn poll_lod_task(mut tasks: ResMut<CrackTasks>, mut res_tiles: ResMut<TileSwapRequests>)
```

### src/plugins/crack_plugin/manifest_flow.rs
```
pub fn spawn_manifest_task(map_tree: Res<MapTree>, mut tasks: ResMut<CrackTasks>, client: Res<CrackClient>,)
pub fn poll_manifest_task(mut tasks: ResMut<CrackTasks>, mut map_tree: ResMut<MapTree>, mut lod_state: ResMut<MapLODState>, mut camera_query: Query<&mut Transform, With<Camera>>,)
```

### src/plugins/crack_plugin/mod.rs
```
pub struct CrackClient
pub struct CrackClientSlot
pub struct CrackRuntime
pub struct CrackTasks
pub struct CrackPlugin
impl CrackPlugin
```

### src/plugins/debug_picker.rs
```
pub struct DebugPickerPlugin
pub struct DebugPickerState
pub struct PickResult
pub enum PickKind
impl DebugPickerPlugin
```

### src/plugins/main_scene_plugin.rs
```
pub struct MainScenePlugin
pub struct SkyboxState
impl MainScenePlugin
```

### src/plugins/map_plugin/bvh_minimap.rs
```
impl TileState
impl MiniView
pub fn bvh_minimap_window(mut contexts: EguiContexts, ui_state: Option<ResMut<crate::ui_egui::UiState>>, mut lod_state: ResMut<MapLODState>, map_tree: Res<MapTree>, res_tiles: Res<TileSwapRequests>, q_tiles: Query<(&TreeMapTile, &Visibility)
```

### src/plugins/map_plugin/map_lod.rs
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

### src/plugins/map_plugin/map_material_edit.rs
```
pub struct MapMaterialEditPlugin
pub struct MapMaterialEditState
impl MapMaterialEditPlugin
impl MapMaterialEditState
```

### src/plugins/map_plugin/map_plugin_ui.rs
```
pub fn configure_map_extent_gizmo(mut store: ResMut<GizmoConfigStore>)
pub fn draw_tree_bboxes(_gizmos: Gizmos, _data_res: Res<MapTree>, _lod_state: Res<MapLODState>, _tiles_query: Query<&TreeMapTile>, _ui_state: Option<Res<crate::ui_egui::UiState>>,)
pub fn draw_map_extent_gizmo(mut gizmos: Gizmos<MapExtentGizmoGroup>, data_res: Res<MapTree>, ui_state: Option<Res<crate::ui_egui::UiState>>,)
pub fn tree_navigator_ui(mut contexts: EguiContexts, data_res: Res<MapTree>, mut lod_state: ResMut<MapLODState>, tiles_query: Query<&TreeMapTile>, ui_state: Option<ResMut<crate::ui_egui::UiState>>,)
pub fn draw_reference_points_gizmos(mut gizmos: Gizmos, data_res: Res<MapTree>, lod_state: Res<MapLODState>, camera_query: Query<&Transform, With<Camera>>,)
```

### src/plugins/map_plugin/mod.rs
```
pub struct MapPlugin
pub struct MapTree
pub struct MapLODState
impl MapPlugin
```

### src/plugins/network/multiplayer_plugin.rs
```
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
pub enum PlayerStateMsg
pub enum PlayerEventMsg
pub enum RemoteAvatar
impl MultiplayerConfig
impl SeenMsgIds
  pub fn is_new(&mut self, id: i64) → bool
impl MultiplayerPlugin
```

### src/plugins/pedestrians/animation.rs
```
pub struct NetworkDriven
pub struct AnimationInfo
pub struct PedestrianAnimations
pub struct PedestrianAnimationControlEvent
pub struct ManualAnimation
pub struct PlayOnceAnimation
pub struct ActiveOneShot
pub struct TargetAnimation
pub struct CurrentPlayingAnimation
impl PedestrianAnimations
  pub fn default_animation(&self) → Option<String>
pub fn locomotion_clip(speed: f32, crouch: bool, _sprint: bool) → &'static [&'static str]
pub fn pedestrian_animation_control_observer(trigger: On<PedestrianAnimationControlEvent>, mut commands: Commands, mut targets: Query<&mut TargetAnimation>,)
pub fn setup_animation_players_system(mut commands: Commands, anims: Res<PedestrianAnimations>, players: Query<Entity, (With<AnimationPlayer>, Without<AnimationGraphHandle>)
pub fn play_animations_system(mut commands: Commands, anims: Res<PedestrianAnimations>, model_roots: Query< ( &PedestrianGltf, Option<&TargetAnimation>, Has<PlayOnceAnimation>,)
```

### src/plugins/pedestrians/pedestrian_controller_plugin/arm_ik.rs
```
pub fn apply_arm_ik(state: Res<State<GameControlState>>, controlled: Res<ControlledCharacter>, mouse: Res<ButtonInput<MouseButton>>, mut contexts: EguiContexts, camera: Query<&GlobalTransform, With<MainCamera>>, skeletons: Query<&PedestrianSkeleton>, combat_states: Query<&CombatState>, equipped: Query<&EquippedWeapon>, gun_states: Query<&GunState>, controllers: Query<&GlobalTransform, With<CharacterController>>, driver_meshes: Query<(Entity, &DriverMesh, &GlobalTransform)
```

### src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs
```
pub struct MainCamera
pub struct CameraRig
impl CameraRig
pub fn orbit_camera_input(mouse_buttons: Res<ButtonInput<MouseButton>>, mouse_motion: Res<AccumulatedMouseMotion>, mut rig: ResMut<CameraRig>, capture_state: Res<crate::plugins::states::MouseCaptureState>,)
pub fn follow_camera(time: Res<Time>, mouse: Res<ButtonInput<MouseButton>>, mut contexts: EguiContexts, controlled: Res<ControlledCharacter>, mut rig: ResMut<CameraRig>, controller: Query<&GlobalTransform, With<CharacterController>>, mut camera: Query<&mut Transform, With<MainCamera>>, spatial_query: avian3d::prelude::SpatialQuery,)
```

### src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs
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

### src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs
```
pub struct CarSeatOffset
pub struct EnteringCarTimer
pub struct DriverMesh
pub struct DriverMeshExit
pub struct PendingEnterCar
pub struct EjectedDriver
pub struct WeaponSelection
pub struct SpawnPlayerDriverEvent
pub struct PendingCarDriver
pub enum EjectedStage
impl CarSeatOffset
pub fn handle_freecam_right_click(mouse_button: Res<ButtonInput<MouseButton>>, window_query: Query<&Window>, camera_query: Query<(&Camera, &GlobalTransform)
pub fn spawn_choice_popup_ui(mut commands: Commands, mut contexts: EguiContexts, mut popup: ResMut<SpawnChoicePopup>,)
pub fn detect_car_interaction(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, q_player: Query< (Entity, &GlobalTransform)
pub fn tick_entering_car(mut commands: Commands, time: Res<Time>, mut q_player: Query<( Entity, &mut EnteringCarTimer, &mut Transform, &CharacterScale,)
pub fn tick_ejected_driver_system(mut commands: Commands, time: Res<Time>, mut q_ejected: Query<(Entity, &mut EjectedDriver)
pub fn eject_driver_as_ai(commands: &mut Commands, car_gt: &GlobalTransform, driver_mesh_entity: Entity, driver_faction: Faction, driver_health: Health, scale: f32,)
pub fn drive_driver_mesh_animation(anims: Res<PedestrianAnimations>, mut q_driver: Query<(Entity, &mut DriverMesh, Has<DriverMeshExit>)
pub fn apply_seat_offset(seat: Res<CarSeatOffset>, mut q_driver: Query<&mut Transform, (With<DriverMesh>, Without<DriverMeshExit>)
pub fn car_seat_debug_ui(mut contexts: EguiContexts, mut seat: ResMut<CarSeatOffset>, q_driver: Query<()
pub fn handle_exit_car(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, q_active_car: Query<(Entity, &GlobalTransform)
pub fn tick_driver_mesh_exit(mut commands: Commands, time: Res<Time>, mut q_exit: Query<(Entity, &mut Transform, &mut DriverMeshExit)
pub fn weapon_hud_ui(mut contexts: EguiContexts, controlled: Res<ControlledCharacter>, equipped: Query<(&EquippedWeapon, Option<&GunState>, &Health)
pub fn equip_on_new_character(mut commands: Commands, controlled: Res<ControlledCharacter>, manifest: Res<WeaponManifest>, mut selection: ResMut<WeaponSelection>, mut last: Local<Option<Entity>>, q_equipped: Query<&EquippedWeapon>,)
pub fn weapon_wheel(time: Res<Time>, mut next_switch: Local<f32>, mut commands: Commands, mut wheel: MessageReader<MouseWheel>, mut contexts: EguiContexts, controlled: Res<ControlledCharacter>, manifest: Res<WeaponManifest>, mut selection: ResMut<WeaponSelection>,)
```

### src/plugins/pedestrians/pedestrian_controller_plugin/locomotion_consts.rs
```
pub fn walk_speed_cap(walk_secs: f32) → f32
pub fn sprint_speed_cap(sprint_secs: f32) → f32
pub fn walk_secs_from_speed(speed: f32) → f32
pub fn footstep_playback_speed(speed: f32) → f32
```

### src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs
```
pub struct LocomotionInput
pub struct CharacterController
pub struct PlayerDriven
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
pub fn character_collision_bundle() → impl Bundle
pub fn character_physics_bundle(scale: f32, transform: Transform) → impl Bundle
```

### src/plugins/traffic/debug_ui.rs
```
pub fn traffic_debug_ui(mut contexts: EguiContexts, mut config: ResMut<TrafficConfig>, ui_state: Option<ResMut<UiState>>, q_traffic: Query<Entity, With<TrafficCar>>, q_traffic_peds: Query<Entity, With<TrafficPedestrian>>, graph: Res<TrafficRoadGraph>, q_camera: Query<&GlobalTransform, With<MainCamera>>, mut commands: Commands,)
pub fn draw_traffic_gizmos(mut gizmos: Gizmos, graph: Res<TrafficRoadGraph>, config: Res<TrafficConfig>, q_cars: Query<(&Transform, &TrafficCar)
```

### src/plugins/traffic/despawn.rs
```
pub fn despawn_traffic_cars(time: Res<Time>, config: Res<TrafficConfig>, mut q_cars: Query<(Entity, &Transform, &CarDriveState, &mut TrafficCar)
```

### src/plugins/traffic/pedestrian_traffic.rs
```
pub struct PendingTrafficPeds
pub struct PendingTrafficPedEntry
pub fn traffic_pedestrian_spawner(time: Res<Time>, mut last_spawn: Local<f32>, config: Res<TrafficConfig>, graph: Res<TrafficRoadGraph>, q_camera: Query<(&Camera, &GlobalTransform)
pub fn spawn_traffic_pedestrian_observer(trigger: On<SpawnTrafficPedestrianEvent>, mut commands: Commands, graph: Res<TrafficRoadGraph>, map_tree: Option<Res<MapTree>>, spatial_query: avian3d::prelude::SpatialQuery, mut pending_traffic: ResMut<PendingTrafficPeds>,)
pub fn adopt_traffic_pedestrians(mut commands: Commands, mut pending: ResMut<PendingTrafficPeds>, q_new_ai: Query<(Entity, &Transform)
pub fn drive_traffic_pedestrians(time: Res<Time>, graph: Res<TrafficRoadGraph>, mut q_peds: Query<( Entity, &GlobalTransform, &AiState, &mut TrafficPedestrian, &mut LocomotionInput,)
pub fn despawn_traffic_pedestrians(time: Res<Time>, config: Res<TrafficConfig>, mut q_peds: Query<(Entity, &Transform, &mut TrafficPedestrian)
```

### src/plugins/traffic/spawn.rs
```
pub fn get_ground_y(pos: Vec3, map_tree: Option<&MapTree>, spatial_query: &avian3d::prelude::SpatialQuery,) → f32
pub fn traffic_network_spawner(time: Res<Time>, mut last_spawn: Local<f32>, config: Res<TrafficConfig>, graph: Res<TrafficRoadGraph>, q_camera: Query<(&Camera, &GlobalTransform)
pub fn spawn_traffic_car_observer(trigger: On<SpawnTrafficCarEvent>, mut commands: Commands, graph: Res<TrafficRoadGraph>, asset_server: Res<AssetServer>, map_tree: Option<Res<MapTree>>, spatial_query: avian3d::prelude::SpatialQuery, manifest: Option<Res<PedestrianManifest>>, wheel_assets: Res<WheelAssets>,)
```

### src/plugins/visual_fx/clouds.rs
```
pub struct CloudPlane
pub fn setup_clouds(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut cloud_mats: ResMut<Assets<CloudMaterial>>, settings: Res<VfxSettings>,)
pub fn position_clouds_over_map(map_tree: Option<Res<crate::plugins::map_plugin::MapTree>>, mut q_planes: Query<&mut Transform, With<CloudPlane>>,)
pub fn sync_cloud_uniforms(settings: Res<VfxSettings>, mut cloud_mats: ResMut<Assets<CloudMaterial>>, q_planes: Query<&MeshMaterial3d<CloudMaterial>, With<CloudPlane>>,)
```

### src/plugins/visual_fx/demo.rs
```
pub struct VfxDemoState
pub struct VfxDemoPlugin
pub enum DemoEffect
impl DemoEffect
  pub fn label(self) → &'static str
impl VfxDemoState
impl VfxDemoPlugin
```

### src/plugins/visual_fx/gun_fx.rs
```
pub struct GunFxEvent
pub struct GunFxCounter
pub struct GunSmokeEmitter
pub fn gun_fx_observer(trigger: On<GunFxEvent>, mut commands: Commands, time: Res<Time>, settings: Res<VfxSettings>, meshes: Option<Res<VfxMeshes>>, mut additive_mats: ResMut<Assets<AdditiveFxMaterial>>, mut blend_mats: ResMut<Assets<BlendFxMaterial>>, q_model_state: Query<&WeaponModelState>, mut q_smoke_emitter: Query<&mut GunSmokeEmitter>, counter: Option<ResMut<GunFxCounter>>,)
pub fn tick_gun_smoke_emitters(mut commands: Commands, time: Res<Time>, settings: Res<VfxSettings>, meshes: Option<Res<VfxMeshes>>, mut blend_mats: ResMut<Assets<BlendFxMaterial>>, mut q_emitters: Query<( Entity, &GlobalTransform, Option<&WeaponExtents>, &mut GunSmokeEmitter,)
```

### src/plugins/visual_fx/materials.rs
```
pub struct BillboardParams
pub struct AdditiveFxMaterial
pub struct BlendFxMaterial
pub struct CloudParamsUniform
pub struct CloudMaterial
pub enum FxKind
impl AdditiveFxMaterial
impl BlendFxMaterial
impl CloudMaterial
```

### src/plugins/visual_fx/mod.rs
```
pub struct VisualFXPlugin
impl VisualFXPlugin
```

### src/plugins/visual_fx/settings.rs
```
pub struct VfxSettings
impl VfxSettings
```

### src/plugins/visual_fx/smoke_emitter.rs
```
pub struct SmokeEmitter
pub fn tick_smoke_emitters(mut commands: Commands, time: Res<Time>, settings: Res<VfxSettings>, meshes: Option<Res<VfxMeshes>>, mut blend_mats: ResMut<Assets<BlendFxMaterial>>, mut q_emitters: Query<(Entity, &GlobalTransform, &mut SmokeEmitter)
```

### src/plugins/visual_fx/spawn.rs
```
pub struct VfxLifetime
pub struct VfxDrift
pub struct VfxMeshes
pub fn spawn_additive_billboard_fx(commands: &mut Commands, mats: &mut Assets<AdditiveFxMaterial>, meshes: &VfxMeshes, time: &Time, pos: Vec3, params: BillboardParams,) → Entity
pub fn spawn_blend_billboard_fx(commands: &mut Commands, mats: &mut Assets<BlendFxMaterial>, meshes: &VfxMeshes, time: &Time, pos: Vec3, params: BillboardParams,) → Entity
pub fn despawn_expired_fx(mut commands: Commands, time: Res<Time>, q: Query<(Entity, &VfxLifetime)
pub fn tick_vfx_drift(time: Res<Time>, mut q: Query<(&mut Transform, &VfxDrift)
pub fn setup_vfx_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>)
```

### src/plugins/visual_fx/ui.rs
```
pub fn vfx_controls_window(mut contexts: EguiContexts, mut ui_state: ResMut<UiState>, mut s: ResMut<VfxSettings>,)
```

### src/plugins/weapons/weapon_attach.rs
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
pub fn update_weapon_transforms(grip: Res<WeaponGripOffset>, rig: Res<CameraRig>, controlled: Res<ControlledCharacter>, camera: Query<&GlobalTransform, With<MainCamera>>, spatial: SpatialQuery, parents: Query<&ChildOf>, global_transforms: Query<&GlobalTransform>, combat_states: Query<&CombatState>, mut weapons: Query<(Entity, &mut Transform, &WeaponKind)
```

### src/plugins/weapons/weapon_shooting.rs
```
pub struct GunState
pub struct WeaponCooldown
pub struct FireGunEvent
pub struct ReloadGunEvent
pub struct ShotTracer
pub struct ShotTracers
pub struct BulletSpark
pub struct BulletSparks
pub struct MeleeDebugBox
pub struct MeleeDebugBoxes
pub struct PendingMeleeHit
pub fn tick_weapon_cooldown(time: Res<Time>, mut q: Query<&mut WeaponCooldown>)
pub fn tick_reload(time: Res<Time>, mut q: Query<&mut GunState>)
pub fn draw_melee_debug_boxes(time: Res<Time>, mut gizmos: Gizmos, mut boxes: ResMut<MeleeDebugBoxes>,)
pub fn fire_gun_observer(trigger: On<FireGunEvent>, mut shooters: Query<(&mut GunState, &EquippedWeapon, Option<&WeaponModelState>)
pub fn reload_gun_observer(trigger: On<ReloadGunEvent>, mut shooters: Query<(&mut GunState, &EquippedWeapon, &GlobalTransform)
pub fn draw_shot_tracers(time: Res<Time>, mut gizmos: Gizmos, mut tracers: ResMut<ShotTracers>, settings: Res<crate::plugins::visual_fx::settings::VfxSettings>,)
pub fn draw_bullet_sparks(time: Res<Time>, mut gizmos: Gizmos, mut sparks: ResMut<BulletSparks>, settings: Res<crate::plugins::visual_fx::settings::VfxSettings>,)
pub fn tick_pending_melee_hits(mut commands: Commands, time: Res<Time>, mut query: Query<(Entity, &GlobalTransform, &mut PendingMeleeHit)
```

### src/ui_egui.rs
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

### src/utils/setup_debug_scene.rs
```
pub struct SetupDebugScenePlugin
pub struct DebugSceneGroundComponent
impl SetupDebugScenePlugin
```
