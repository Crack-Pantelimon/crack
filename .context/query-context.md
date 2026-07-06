# SigMap Query Context
Generated: 2026-07-06T11:11:00.003Z

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs
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
pub fn apply_car_steering_and_drive(mut q_car: Query< ( &Transform, &mut CarDriveState, &CarWheelsContactData, &mut LinearVelocity, &mut AngularVelocity,)
pub fn init_cosmetic_wheels_system(mut q_wheels: Query<(Entity, &Transform, &mut CosmeticWheel)
pub fn update_cosmetic_wheels(mut commands: Commands, mut q_wheels: Query<(Entity, &mut Transform, &mut CosmeticWheel)
pub fn update_wheel_contact_normals(spatial_query: SpatialQuery, mut q_cars: Query<(Entity, &Transform, &CarDriveState, &mut CarWheelsContactData)
pub fn draw_car_gizmos(mut gizmos: Gizmos, q_car: Query<(&Transform, &CarDriveState, &CarWheelsContactData)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs
```
pub struct SpawnCarRequestEvent
pub struct Car
pub struct NeedCarBoundsCompute
pub struct ActivePlayerVehicle
pub fn spawn_car_request_event_observer(spawn_car_event: On<SpawnCarRequestEvent>, mut commands: Commands, current_state: Res<State<GameControlState>>, mut next_state: ResMut<NextState<GameControlState>>, spatial_query: avian3d::prelude::SpatialQuery, asset_server: Res<AssetServer>, q_active_cars: Query<Entity, With<ActivePlayerVehicle>>,)
pub fn init_cars_system(mut commands: Commands, query: Query<(Entity, &NeedCarBoundsCompute, &Children)
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

## crack_demo/demo_resolution_selector_web_bevy/src/bin/car_sim.rs
```
impl SimLogTimer
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
