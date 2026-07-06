# SigMap Query Context
Generated: 2026-07-06T13:30:55.907Z

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs
```
pub fn print_animation_catalog(anims: Res<PedestrianAnimations>, mut done: Local<bool>)
pub fn drive_character_animation(time: Res<Time>, anims: Res<PedestrianAnimations>, controlled: Res<ControlledCharacter>, mouse: Res<ButtonInput<MouseButton>>, keys: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut contexts: EguiContexts, mut controllers: Query< ( &LinearVelocity, Has<Grounded>, &MovementModifiers, &CharacterScale, Has<Climbing>, Has<Rolling>, Option<&EquippedWeapon>, Option<&GunState>, &mut AnimState, &mut CombatState, Option<&EnteringCarTimer>,)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs
```
pub struct ControlledCharacter
pub struct SpawnChoicePopup
pub struct SpawnControlledPedestrianEvent
pub fn spawn_controlled_pedestrian_observer(trigger: On<SpawnControlledPedestrianEvent>, mut commands: Commands, manifest: Res<PedestrianManifest>, mut controlled: ResMut<ControlledCharacter>, mut next_state: ResMut<NextState<GameControlState>>,)
pub fn adopt_pedestrian(mut commands: Commands, mut controlled: ResMut<ControlledCharacter>, new_peds: Query<Entity, Added<ModelRoot>>,)
pub fn escape_to_freecam(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands, mut controlled: ResMut<ControlledCharacter>, mut next_state: ResMut<NextState<GameControlState>>,)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs
```
pub struct CameraRig
impl CameraRig
pub fn orbit_camera_input(mouse_buttons: Res<ButtonInput<MouseButton>>, mouse_motion: Res<AccumulatedMouseMotion>, mut rig: ResMut<CameraRig>,)
pub fn follow_camera(time: Res<Time>, controlled: Res<ControlledCharacter>, mut rig: ResMut<CameraRig>, controller: Query<&GlobalTransform, With<CharacterController>>, mut camera: Query<&mut Transform, With<Camera3d>>,)
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs
```
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
pub enum MovementAction
pub enum JumpPhase
pub enum CombatKind
impl CharacterMovementSettings
impl GroundDetection
impl AnimState
impl PedestrianControllerPlugin
```

## crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs
```
pub fn character_input(keys: Res<ButtonInput<KeyCode>>, camera: Query<&GlobalTransform, With<Camera3d>>, mut modifiers: Query<&mut MovementModifiers>, mut movement_writer: MessageWriter<MovementAction>,)
pub fn update_grounded(mut commands: Commands, mut query: Query<(Entity, &GroundDetection, &GlobalTransform)
pub fn movement(time: Res<Time>, mut movement_reader: MessageReader<MovementAction>, mut controllers: Query<( &CharacterMovementSettings, &mut LinearVelocity, Has<Grounded>,)
pub fn apply_gravity(time: Res<Time>, mut controllers: Query<(&CharacterMovementSettings, &mut LinearVelocity)
pub fn apply_movement_damping(mut query: Query<(&CharacterMovementSettings, &mut LinearVelocity)
pub fn apply_speed_cap(time: Res<Time>, mut query: Query<(&mut MovementModifiers, &mut LinearVelocity, Has<Rolling>)
pub fn move_and_slide(mut query: Query< ( Entity, Option<&GroundDetection>, Option<&mut CharacterCollisions>, &mut Transform, &mut LinearVelocity, &Collider,)
pub fn apply_forces_to_dynamic_bodies(characters: Query<(&ComputedMass, &CharacterCollisions)
pub fn face_movement(time: Res<Time>, mut query: Query<(&LinearVelocity, &mut Transform)
pub fn respawn_if_fallen(mut query: Query<(&mut Transform, &mut LinearVelocity)
pub fn jump_or_climb(keys: Res<ButtonInput<KeyCode>>, spatial_query: SpatialQuery, mut commands: Commands, mut movement_writer: MessageWriter<MovementAction>, map: Option<Res<MapTree>>, tiles: Query<()
pub fn update_climb(time: Res<Time>, mut commands: Commands, mut query: Query<(Entity, &mut Transform, &mut LinearVelocity, &mut Climbing)
pub fn update_roll(time: Res<Time>, mut commands: Commands, mut query: Query<(Entity, &Transform, &mut LinearVelocity, &mut Rolling)
pub fn detect_fallen_off_map(map: Option<Res<MapTree>>, tiles: Query<()
```
