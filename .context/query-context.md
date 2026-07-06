# SigMap Query Context
Generated: 2026-07-06T13:59:30.933Z

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_manifest.rs
```
pub struct GunInfo
pub struct WeaponManifest
pub struct WeaponManifestBootstrap
pub enum WeaponId
impl WeaponId
pub fn is_unarmed(&self) → bool
pub fn is_gun(&self) → bool
pub fn is_melee(&self) → bool
pub fn path(&self) → Option<&str>
pub fn gun_info(&self) → Option<&GunInfo>
pub fn label(&self) → String
pub fn start_weapon_manifest_load(mut commands: Commands, asset_server: Res<AssetServer>)
pub fn load_weapon_manifest_system(bootstrap: Option<Res<WeaponManifestBootstrap>>, text_assets: Res<Assets<TextAsset>>, mut manifest: ResMut<WeaponManifest>,)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/manifest.rs
```
pub struct PedestrianUrl
pub struct PedestrianManifest
pub struct ManifestBootstrap
pub struct TextAsset
pub struct TextAssetLoader
impl TextAssetLoader
pub fn start_manifest_load(mut commands: Commands, asset_server: Res<AssetServer>)
pub fn load_pedestrian_manifest_system(asset_server: Res<AssetServer>, mut bootstrap: ResMut<ManifestBootstrap>, mut manifest: ResMut<PedestrianManifest>, mut anims: ResMut<PedestrianAnimations>, text_assets: Res<Assets<TextAsset>>, gltf_assets: Res<Assets<bevy::gltf::Gltf>>, clip_assets: Res<Assets<AnimationClip>>, mut graphs: ResMut<Assets<AnimationGraph>>,)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs
```
pub fn print_animation_catalog(anims: Res<PedestrianAnimations>, mut done: Local<bool>)
pub fn drive_character_animation(time: Res<Time>, anims: Res<PedestrianAnimations>, controlled: Res<ControlledCharacter>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut contexts: EguiContexts, mut controllers: Query< ( &LinearVelocity, Has<Grounded>, &MovementModifiers, &CharacterScale, Has<Climbing>, Has<Rolling>, Option<&EquippedWeapon>, Option<&GunState>, &mut AnimState, &mut CombatState, Option<&EnteringCarTimer>,)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs
```
pub struct CarSeatOffset
pub struct EnteringCarTimer
pub struct DriverMesh
pub struct DriverMeshExit
impl CarSeatOffset
pub fn handle_freecam_right_click(mouse_button: Res<ButtonInput<MouseButton>>, window_query: Query<&Window>, camera_query: Query<(&Camera, &GlobalTransform)
pub fn spawn_choice_popup_ui(mut commands: Commands, mut contexts: EguiContexts, mut popup: ResMut<SpawnChoicePopup>,)
pub fn detect_car_interaction(keys: Res<ButtonInput<KeyCode>>, q_player: Query< (Entity, &GlobalTransform)
pub fn tick_entering_car(mut commands: Commands, time: Res<Time>, mut q_player: Query<(Entity, &mut EnteringCarTimer, &mut Transform, &CharacterScale)
pub fn drive_driver_mesh_animation(anims: Res<PedestrianAnimations>, mut q_driver: Query<(Entity, &mut DriverMesh, Has<DriverMeshExit>)
pub fn apply_seat_offset(seat: Res<CarSeatOffset>, mut q_driver: Query<&mut Transform, (With<DriverMesh>, Without<DriverMeshExit>)
pub fn car_seat_debug_ui(mut contexts: EguiContexts, mut seat: ResMut<CarSeatOffset>, q_driver: Query<()
pub fn handle_exit_car(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>, q_active_car: Query<(Entity, &GlobalTransform)
pub fn tick_driver_mesh_exit(mut commands: Commands, time: Res<Time>, mut q_exit: Query<(Entity, &mut Transform, &mut DriverMeshExit)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs
```
pub struct GunState
pub struct FireGunEvent
pub struct ReloadGunEvent
pub struct ShotTracer
pub struct ShotTracers
pub fn fire_gun_observer(trigger: On<FireGunEvent>, mut shooters: Query<(&mut GunState, &EquippedWeapon, Option<&WeaponModelState>)
pub fn reload_gun_observer(trigger: On<ReloadGunEvent>, mut shooters: Query<&mut GunState>,)
pub fn draw_shot_tracers(time: Res<Time>, mut gizmos: Gizmos, mut tracers: ResMut<ShotTracers>)
```
