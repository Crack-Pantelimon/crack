//! Pedestrian Controller — a playable kinematic character driven by the pedestrian models.
//!
//! A pill-shaped (capsule) kinematic character controller (ported from the avian3d
//! `kinematic_character_3d` example) moves on WASD, jumps on Space, crouches on `C` (hold) and
//! sprints on `Shift` (hold). The visible pedestrian model is spawned via [`PedestriansPlugin`] and
//! parented *under* the controller as a purely visual child (its own colliders are disabled — the
//! only physics body is the capsule). The controller yaws toward its movement direction, so the
//! single forward-facing locomotion clips (`Walk_Loop`, `Jog_Fwd_Loop`, `Sprint_Fwd_Loop`,
//! `Crouch_Fwd_Loop`, `Jump_*`) work for movement in any direction.
//!
//! Animation is a plain state machine: it triggers the existing hard-switch
//! [`PedestrianAnimationControlEvent`] whenever the desired clip changes — no weight blending
//! (Bevy's `AnimationPlayer` supports playing several clips at once, which we could use later if a
//! real crossfade is wanted, but a single clip at a time is enough here).
//!
//! An egui window lists every pedestrian in the manifest; clicking `Spawn` despawns the current
//! character and spawns the selected model as the new controlled character.
//!
//! Everything controller-related lives in [`PedestrianAnimationControllerPlugin`].

use avian3d::{math::*, prelude::*};
use bevy::{ecs::query::Has, input::mouse::AccumulatedMouseMotion, prelude::*};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use rand::seq::IndexedRandom;

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        cars_driving::driving_plugin::GamePhysicsLayer,
        pedestrians::{
            ModelRoot, PedestrianAnimationControlEvent, PedestrianAnimations, PedestrianManifest,
            PedestrianUrl, PedestriansPlugin, SpawnPedestrianEvent,
        },
    },
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("Pedestrian Controller")
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        // Draw the capsule controller (and other colliders) as gizmos.
        .add_plugins(PhysicsDebugPlugin::default())
        .add_plugins(PedestriansPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(PedestrianAnimationControllerPlugin)
        .run();
}

// ---------------------------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------------------------

/// Capsule dimensions (radius + straight cylinder length). Total height = length + 2*radius.
const CAPSULE_RADIUS: f32 = 0.35;
const CAPSULE_LENGTH: f32 = 1.0;
/// Distance from capsule center to its bottom tip; used to sit the model's feet on the ground.
const CAPSULE_HALF_HEIGHT: f32 = CAPSULE_LENGTH / 2.0 + CAPSULE_RADIUS;
/// Full capsule height (tip to tip).
const CAPSULE_TOTAL_HEIGHT: f32 = CAPSULE_LENGTH + 2.0 * CAPSULE_RADIUS;
/// If the controller ever falls below the ground plane, it is teleported back up to this height.
const RESPAWN_HEIGHT: f32 = 3.0 + CAPSULE_TOTAL_HEIGHT;

/// Where a freshly-spawned controller appears.
const SPAWN_POS: Vec3 = Vec3::new(0.0, 1.2, 0.0);

// Movement.
const MOVE_ACCEL: f32 = 60.0;
const MOVE_DAMPING: f32 = 12.0;
const JUMP_IMPULSE: f32 = 7.5;
const GRAVITY_Y: f32 = -9.81 * 2.0;
/// Per-mode horizontal speed caps.
const CROUCH_SPEED: f32 = 1.8;
const JOG_SPEED: f32 = 4.0;
const SPRINT_SPEED: f32 = 7.5;

// Animation selection by current horizontal speed.
const MOVE_ANIM_THRESHOLD: f32 = 0.25;
const WALK_MAX_SPEED: f32 = 2.2;
const JOG_MAX_SPEED: f32 = 5.5;

// Jump animation phase timings (seconds).
const JUMP_START_TIME: f32 = 0.22;
const JUMP_LAND_TIME: f32 = 0.22;

/// How fast the controller turns to face its movement direction (higher = snappier).
const TURN_SPEED: f32 = 12.0;
/// Yaw offset applied on top of the movement direction, in case the model's forward axis is not +Z.
/// Flip to `std::f32::consts::PI` if the character faces away from where it walks.
const MODEL_FORWARD_OFFSET: f32 = 0.0;

// Follow camera.
const CAM_DISTANCE: f32 = 5.5;
const CAM_LOOK_HEIGHT: f32 = 1.1;
const CAM_LERP: f32 = 6.0;
/// Mouse-drag orbit sensitivity (radians per pixel).
const CAM_ORBIT_SENSITIVITY: f32 = 0.006;
/// Pitch is clamped so the camera stays above the character and never flips.
const CAM_PITCH_MIN: f32 = -1.4;
const CAM_PITCH_MAX: f32 = -0.05;

// ---------------------------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------------------------

pub struct PedestrianAnimationControllerPlugin;

impl Plugin for PedestrianAnimationControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MovementAction>()
            .init_resource::<ControlledCharacter>()
            .init_resource::<CameraRig>()
            .add_observer(spawn_controlled_observer)
            .add_systems(Startup, spawn_physics_cubes)
            // Collect input, and disable the model's colliders, before the physics update.
            .add_systems(PreUpdate, (character_input, disable_ped_colliders))
            // Movement logic in FixedUpdate for frame-rate independence.
            .add_systems(
                FixedUpdate,
                (
                    update_grounded,
                    apply_gravity,
                    movement,
                    apply_movement_damping,
                    apply_speed_cap,
                    move_and_slide,
                    apply_forces_to_dynamic_bodies,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    print_animation_catalog,
                    auto_spawn_on_manifest,
                    adopt_pedestrian,
                    respawn_if_fallen,
                    face_movement,
                    orbit_camera_input,
                    follow_camera,
                    update_character_animation,
                ),
            )
            .add_systems(EguiPrimaryContextPass, ui_pedestrian_list);
    }
}

// ---------------------------------------------------------------------------------------------
// Kinematic character controller (ported from avian3d kinematic_character_3d example)
// ---------------------------------------------------------------------------------------------

/// A [`Message`] written for a movement input action.
#[derive(Message)]
pub enum MovementAction {
    /// Desired planar move direction, mapped as the example does: `x -> +x`, `y -> -z`.
    Move(Vector2),
    Jump,
}

/// Marker for the kinematic character body. Requires a kinematic rigid body and disables Avian's
/// automatic position integration so move-and-slide can drive the transform manually.
#[derive(Component)]
#[require(RigidBody::Kinematic, CustomPositionIntegration, SpeculativeMargin(0.0))]
pub struct CharacterController;

/// Held movement modifiers, updated from the keyboard each frame.
#[derive(Component, Default)]
pub struct MovementModifiers {
    pub crouch: bool,
    pub sprint: bool,
}

/// Movement settings for a character controller.
#[derive(Component)]
pub struct CharacterMovementSettings {
    pub acceleration: Scalar,
    pub damping: Scalar,
    pub jump_impulse: Scalar,
    pub gravity: Vector,
    pub terminal_velocity: Scalar,
}

impl Default for CharacterMovementSettings {
    fn default() -> Self {
        Self {
            acceleration: MOVE_ACCEL as Scalar,
            damping: MOVE_DAMPING as Scalar,
            jump_impulse: JUMP_IMPULSE as Scalar,
            gravity: Vector::new(0.0, GRAVITY_Y as Scalar, 0.0),
            terminal_velocity: 50.0,
        }
    }
}

/// Ground detection configuration for a character controller.
#[derive(Component)]
pub struct GroundDetection {
    pub max_angle: Scalar,
    pub max_distance: Scalar,
    pub cast_shape: Option<Collider>,
}

impl Default for GroundDetection {
    fn default() -> Self {
        Self {
            max_angle: PI / 6.0,
            max_distance: 0.2,
            cast_shape: None,
        }
    }
}

/// Marker for a character that is currently standing on ground.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Grounded;

/// Per-frame collisions recorded by move-and-slide, used to push dynamic bodies.
#[derive(Component, Default, Deref)]
pub struct CharacterCollisions(Vec<CharacterCollision>);

pub struct CharacterCollision {
    pub collider: Entity,
    pub point: Vector,
    pub normal: Dir3,
    pub character_velocity: Vector,
}

/// Reads WASD into a camera-relative move direction and updates modifiers. Space -> jump.
fn character_input(
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut modifiers: Query<&mut MovementModifiers>,
    mut movement_writer: MessageWriter<MovementAction>,
) {
    let Ok(cam) = camera.single() else {
        return;
    };

    // Camera forward/right flattened onto the ground plane.
    let mut forward = cam.forward().as_vec3();
    forward.y = 0.0;
    let forward = forward.normalize_or_zero();
    let mut right = cam.right().as_vec3();
    right.y = 0.0;
    let right = right.normalize_or_zero();

    let f = keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) as i8
        - keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) as i8;
    let r = keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) as i8
        - keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) as i8;

    let world = forward * f as f32 + right * r as f32;
    let world = world.normalize_or_zero();
    if world != Vec3::ZERO {
        // Map world XZ direction into the example's Move convention (x -> +x, y -> -z).
        movement_writer.write(MovementAction::Move(Vector2::new(
            world.x as Scalar,
            -world.z as Scalar,
        )));
    }

    if keys.just_pressed(KeyCode::Space) {
        movement_writer.write(MovementAction::Jump);
    }

    for mut m in &mut modifiers {
        m.crouch = keys.pressed(KeyCode::KeyC);
        m.sprint = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    }
}

/// Updates the [`Grounded`] status for character controllers.
fn update_grounded(
    mut commands: Commands,
    mut query: Query<(Entity, &GroundDetection, &GlobalTransform)>,
    spatial_query: SpatialQuery,
) {
    for (entity, ground_detection, global_transform) in &mut query {
        let Some(collider) = &ground_detection.cast_shape else {
            continue;
        };

        let translation = global_transform.translation().adjust_precision();
        let rotation = global_transform.rotation().adjust_precision();

        let hit = spatial_query.cast_shape(
            collider,
            translation,
            rotation,
            global_transform.down(),
            &ShapeCastConfig::from_max_distance(ground_detection.max_distance),
            &SpatialQueryFilter::from_excluded_entities([entity]),
        );

        let is_grounded = hit.is_some_and(|hit| {
            let up = global_transform.up().adjust_precision();
            (rotation * hit.normal1).angle_between(up) <= ground_detection.max_angle
        });

        if is_grounded {
            commands.entity(entity).insert(Grounded);
        } else {
            commands.entity(entity).remove::<Grounded>();
        }
    }
}

/// Responds to [`MovementAction`] events and accelerates/jumps character controllers.
fn movement(
    time: Res<Time>,
    mut movement_reader: MessageReader<MovementAction>,
    mut controllers: Query<(&CharacterMovementSettings, &mut LinearVelocity, Has<Grounded>)>,
) {
    let delta_secs = time.delta_secs_f64().adjust_precision();

    for event in movement_reader.read() {
        for (movement, mut linear_velocity, is_grounded) in &mut controllers {
            match event {
                MovementAction::Move(direction) => {
                    linear_velocity.x += direction.x * movement.acceleration * delta_secs;
                    linear_velocity.z -= direction.y * movement.acceleration * delta_secs;
                }
                MovementAction::Jump => {
                    if is_grounded {
                        linear_velocity.y = movement.jump_impulse;
                    }
                }
            }
        }
    }
}

/// Applies custom gravity to character controllers.
fn apply_gravity(
    time: Res<Time>,
    mut controllers: Query<(&CharacterMovementSettings, &mut LinearVelocity)>,
) {
    let delta_secs = time.delta_secs_f64().adjust_precision();

    for (movement, mut linear_velocity) in &mut controllers {
        let gravity_direction = movement.gravity.normalize_or_zero();

        let velocity_along_gravity = linear_velocity.dot(gravity_direction);
        if velocity_along_gravity > movement.terminal_velocity {
            continue;
        }

        let new_velocity = linear_velocity.0 + movement.gravity * delta_secs;
        let new_velocity_along_gravity = new_velocity.dot(gravity_direction);
        if new_velocity_along_gravity < movement.terminal_velocity {
            linear_velocity.0 = new_velocity;
        } else {
            linear_velocity.0 = gravity_direction * movement.terminal_velocity;
        }
    }
}

/// Exponential decay of horizontal velocity (Y left untouched).
fn apply_movement_damping(
    mut query: Query<(&CharacterMovementSettings, &mut LinearVelocity)>,
    time: Res<Time>,
) {
    let delta_secs = time.delta_secs_f64().adjust_precision();

    for (movement, mut linear_velocity) in &mut query {
        linear_velocity.x *= 1.0 / (1.0 + delta_secs * movement.damping);
        linear_velocity.z *= 1.0 / (1.0 + delta_secs * movement.damping);
    }
}

/// Clamps horizontal speed to the current movement-mode cap (crouch / normal / sprint).
fn apply_speed_cap(mut query: Query<(&MovementModifiers, &mut LinearVelocity)>) {
    for (modifiers, mut velocity) in &mut query {
        let cap = if modifiers.crouch {
            CROUCH_SPEED
        } else if modifiers.sprint {
            SPRINT_SPEED
        } else {
            JOG_SPEED
        } as Scalar;

        let horizontal = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
        if horizontal > cap && horizontal > 0.0 {
            let factor = cap / horizontal;
            velocity.x *= factor;
            velocity.z *= factor;
        }
    }
}

/// Performs move-and-slide for character controllers, sliding along contact surfaces.
fn move_and_slide(
    mut query: Query<
        (
            Entity,
            Option<&GroundDetection>,
            Option<&mut CharacterCollisions>,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
        ),
        With<CharacterController>,
    >,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    for (entity, ground_detection, mut collisions, mut transform, mut lin_vel, collider) in
        &mut query
    {
        let mut hit_ground_or_ceiling = false;

        if let Some(collisions) = &mut collisions {
            collisions.0.clear();
        }

        let up = transform.up().adjust_precision();

        let MoveAndSlideOutput {
            position: new_position,
            projected_velocity,
        } = move_and_slide.move_and_slide(
            collider,
            transform.translation.adjust_precision(),
            transform.rotation.adjust_precision(),
            lin_vel.0,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::from_excluded_entities([entity]),
            |hit| {
                let Some(ground_detection) = ground_detection else {
                    return MoveAndSlideHitResponse::Accept;
                };

                let angle = up.angle_between(hit.normal.adjust_precision());
                let is_ground = angle <= ground_detection.max_angle;
                let is_ceiling = is_ground && up.dot(hit.normal.adjust_precision()) < 0.0;

                let [_horizontal_component, vertical_component] =
                    split_into_components(lin_vel.0, up);

                let horizontal_velocity_decomposition =
                    decompose_hit_velocity(_horizontal_component, *hit.normal, up);
                let decomposition = decompose_hit_velocity(*hit.velocity, *hit.normal, up);

                let slipping_intent =
                    up.dot(horizontal_velocity_decomposition.vertical_tangent) < -0.001;
                let slipping = up.dot(decomposition.vertical_tangent) < -0.001;
                let climbing_intent = up.dot(vertical_component) > 0.0;
                let climbing = up.dot(decomposition.vertical_tangent) > 0.0;

                let projected_velocity = if !is_ground && climbing && !climbing_intent {
                    decomposition.horizontal_tangent + decomposition.normal_part
                } else if is_ground && slipping && !slipping_intent {
                    decomposition.horizontal_tangent + decomposition.normal_part
                } else {
                    decomposition.horizontal_tangent
                        + decomposition.vertical_tangent
                        + decomposition.normal_part
                };

                *hit.velocity = projected_velocity;

                if is_ground || is_ceiling {
                    hit_ground_or_ceiling = true;
                }

                if let Some(collisions) = &mut collisions {
                    collisions.0.push(CharacterCollision {
                        collider: hit.entity,
                        point: hit.point,
                        normal: *hit.normal,
                        character_velocity: *hit.velocity,
                    });
                }

                MoveAndSlideHitResponse::Accept
            },
        );

        transform.translation = new_position.f32();

        if hit_ground_or_ceiling {
            let up = up.adjust_precision();
            let velocity_along_up = lin_vel.dot(up);
            let new_velocity_along_up = projected_velocity.dot(up);
            lin_vel.0 += (new_velocity_along_up - velocity_along_up) * up;
        }
    }
}

struct VelocityDecomposition {
    normal_part: Vector,
    horizontal_tangent: Vector,
    vertical_tangent: Vector,
}

fn decompose_hit_velocity(velocity: Vector, normal: Dir, up: Vector) -> VelocityDecomposition {
    let normal = normal.adjust_precision();
    let normal_part = normal * normal.dot(velocity);
    let tangent_part = velocity - normal_part;

    let horizontal_tangent_dir = normal.cross(up).normalize_or_zero();
    let horizontal_tangent = tangent_part.dot(horizontal_tangent_dir) * horizontal_tangent_dir;
    let vertical_tangent = tangent_part - horizontal_tangent;

    VelocityDecomposition {
        normal_part,
        horizontal_tangent,
        vertical_tangent,
    }
}

fn split_into_components(v: Vector, up: Vector) -> [Vector; 2] {
    let vertical_component = up * v.dot(up);
    let horizontal_component = v - vertical_component;
    [horizontal_component, vertical_component]
}

/// Applies impulses to dynamic rigid bodies the character pushed into.
fn apply_forces_to_dynamic_bodies(
    characters: Query<(&ComputedMass, &CharacterCollisions)>,
    colliders: Query<&ColliderOf>,
    mut rigid_bodies: Query<(&RigidBody, Forces)>,
) {
    for (mass, collisions) in &characters {
        let mass = mass.value();
        for collision in &collisions.0 {
            let Ok(collider_of) = colliders.get(collision.collider) else {
                continue;
            };
            let Ok((rigid_body, mut forces)) = rigid_bodies.get_mut(collider_of.body) else {
                continue;
            };
            if !rigid_body.is_dynamic() {
                continue;
            }

            let touch_dir = -collision.normal.adjust_precision();
            let relative_velocity = collision.character_velocity - forces.linear_velocity();
            let touch_velocity = touch_dir.dot(relative_velocity) * touch_dir;
            let impulse = touch_velocity * mass;

            forces.apply_linear_impulse_at_point(impulse, collision.point);
        }
    }
}

/// Rotates the controller (and therefore its model child) to face its horizontal velocity.
fn face_movement(
    time: Res<Time>,
    mut query: Query<(&LinearVelocity, &mut Transform), With<CharacterController>>,
) {
    for (velocity, mut transform) in &mut query {
        let vx = velocity.x as f32;
        let vz = velocity.z as f32;
        if Vec2::new(vx, vz).length() < 0.3 {
            continue;
        }
        let target =
            Quat::from_rotation_y(f32::atan2(vx, vz) + MODEL_FORWARD_OFFSET);
        let s = (TURN_SPEED * time.delta_secs()).clamp(0.0, 1.0);
        transform.rotation = transform.rotation.slerp(target, s);
    }
}

/// Safety net: if the controller ever ends up below the ground plane (y < 0), teleport it back up.
fn respawn_if_fallen(
    mut query: Query<(&mut Transform, &mut LinearVelocity), With<CharacterController>>,
) {
    for (mut transform, mut velocity) in &mut query {
        if transform.translation.y < 0.0 {
            transform.translation.y = RESPAWN_HEIGHT;
            velocity.0 = Vector::ZERO;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Third-person follow camera (drives the SetupDebugScenePlugin camera)
// ---------------------------------------------------------------------------------------------

/// Orbit angles for the follow camera, driven by left-mouse drag.
#[derive(Resource)]
struct CameraRig {
    yaw: f32,
    pitch: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: -0.35,
        }
    }
}

/// Left-mouse drag rotates the follow camera around the character.
fn orbit_camera_input(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut contexts: EguiContexts,
    mut rig: ResMut<CameraRig>,
) {
    if !mouse_buttons.pressed(MouseButton::Left) {
        return;
    }
    // Ignore drags that start/occur over the egui panel.
    if let Ok(ctx) = contexts.ctx_mut() {
        if ctx.is_pointer_over_egui() || ctx.wants_pointer_input() {
            return;
        }
    }
    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }
    rig.yaw -= delta.x * CAM_ORBIT_SENSITIVITY;
    rig.pitch = (rig.pitch - delta.y * CAM_ORBIT_SENSITIVITY).clamp(CAM_PITCH_MIN, CAM_PITCH_MAX);
}

fn follow_camera(
    time: Res<Time>,
    controlled: Res<ControlledCharacter>,
    rig: Res<CameraRig>,
    controller: Query<&GlobalTransform, With<CharacterController>>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    let Some(controller_ent) = controlled.controller else {
        return;
    };
    let Ok(controller_gt) = controller.get(controller_ent) else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };

    let look = controller_gt.translation() + Vec3::Y * CAM_LOOK_HEIGHT;
    // Orbit offset: yaw around Y, pitch tilts up/down; camera sits CAM_DISTANCE behind (+Z).
    let offset = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0) * Vec3::new(0.0, 0.0, CAM_DISTANCE);
    let desired = look + offset;

    let t = (CAM_LERP * time.delta_secs()).clamp(0.0, 1.0);
    cam.translation = cam.translation.lerp(desired, t);
    cam.look_at(look, Vec3::Y);
}

// ---------------------------------------------------------------------------------------------
// Spawn / adopt / despawn the controlled pedestrian
// ---------------------------------------------------------------------------------------------

/// Tracks the currently controlled character and its (child) pedestrian model.
#[derive(Resource, Default)]
struct ControlledCharacter {
    controller: Option<Entity>,
    ped: Option<Entity>,
    /// True after spawning a controller while we wait for the pedestrian model to appear.
    awaiting: bool,
}

/// Request to (re)spawn the controlled character with a given pedestrian model.
#[derive(Event)]
struct SpawnControlledEvent {
    url: PedestrianUrl,
}

fn spawn_controlled_observer(
    trigger: On<SpawnControlledEvent>,
    mut commands: Commands,
    mut controlled: ResMut<ControlledCharacter>,
) {
    // Despawn the previous character (its model child goes with it).
    if let Some(old) = controlled.controller.take() {
        commands.entity(old).despawn();
    }
    controlled.ped = None;

    let controller = commands
        .spawn((
            Name::new("CharacterController"),
            CharacterController,
            CharacterMovementSettings::default(),
            CharacterCollisions::default(),
            MovementModifiers::default(),
            AnimState::default(),
            GroundDetection {
                cast_shape: Some(Collider::capsule(CAPSULE_RADIUS * 0.99, CAPSULE_LENGTH)),
                ..default()
            },
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
            // Same layer convention as the cars/cubes so the solver resolves interactions with the
            // Car/Wheel-filtered ground and the dynamic cubes.
            CollisionLayers::new(
                GamePhysicsLayer::Car,
                [
                    GamePhysicsLayer::Map,
                    GamePhysicsLayer::Car,
                    GamePhysicsLayer::Wheel,
                ],
            ),
            Transform::from_translation(SPAWN_POS),
            Visibility::default(),
        ))
        .id();

    controlled.controller = Some(controller);
    controlled.awaiting = true;

    commands.trigger(SpawnPedestrianEvent {
        url: trigger.event().url.clone(),
        position: SPAWN_POS,
    });
}

/// Once the manifest is loaded, spawn a random pedestrian as the controlled character (runs once).
fn auto_spawn_on_manifest(
    mut commands: Commands,
    manifest: Res<PedestrianManifest>,
    mut done: Local<bool>,
) {
    if *done || !manifest.loaded {
        return;
    }
    if let Some(url) = manifest.urls.choose(&mut rand::rng()) {
        commands.trigger(SpawnControlledEvent { url: url.clone() });
    }
    *done = true;
}

/// Adopts a freshly spawned pedestrian model as the child of the pending controller.
fn adopt_pedestrian(
    mut commands: Commands,
    mut controlled: ResMut<ControlledCharacter>,
    new_peds: Query<Entity, Added<ModelRoot>>,
) {
    if !controlled.awaiting {
        return;
    }
    let Some(controller) = controlled.controller else {
        return;
    };
    for ped in new_peds.iter() {
        commands.entity(ped).insert((
            ChildOf(controller),
            // Local offset so the model's feet meet the bottom of the capsule.
            Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0),
        ));
        controlled.ped = Some(ped);
        controlled.awaiting = false;
        break;
    }
}

/// Disables all colliders under the controlled pedestrian — the model is visual-only; the capsule
/// is the sole physics body. `init_pedestrians_system` adds trimesh colliders a few frames after
/// load, so this runs continuously and disables them as they appear.
fn disable_ped_colliders(
    mut commands: Commands,
    controlled: Res<ControlledCharacter>,
    children_query: Query<&Children>,
    needs_disable: Query<(), (With<Collider>, Without<ColliderDisabled>)>,
) {
    let Some(ped) = controlled.ped else {
        return;
    };
    let mut stack = vec![ped];
    while let Some(entity) = stack.pop() {
        if needs_disable.get(entity).is_ok() {
            commands.entity(entity).insert(ColliderDisabled);
        }
        if let Ok(children) = children_query.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Animation state machine (single clip at a time, via the hard-switch control event)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum JumpPhase {
    Grounded,
    Start,
    Loop,
    Land,
}

/// Per-controller animation state.
#[derive(Component)]
struct AnimState {
    current: Option<String>,
    phase: JumpPhase,
    timer: f32,
}

impl Default for AnimState {
    fn default() -> Self {
        Self {
            current: None,
            phase: JumpPhase::Grounded,
            timer: 0.0,
        }
    }
}

/// Logs the animation catalog once it is ready, so the exact clip names are visible.
fn print_animation_catalog(anims: Res<PedestrianAnimations>, mut done: Local<bool>) {
    if *done || !anims.ready {
        return;
    }
    info!("=== Pedestrian animation catalog ({}) ===", anims.catalog.len());
    for (name, info) in &anims.catalog {
        info!("  {:<24} duration={:.2}s frames={}", name, info.duration, info.frames);
    }
    *done = true;
}

/// Returns the first available clip name from `candidates`, falling back to the default clip.
fn resolve_anim(anims: &PedestrianAnimations, candidates: &[&str]) -> Option<String> {
    for c in candidates {
        if anims.catalog.contains_key(*c) {
            return Some((*c).to_string());
        }
    }
    anims.default_animation()
}

/// Picks the desired animation from controller state and triggers a switch when it changes.
fn update_character_animation(
    time: Res<Time>,
    anims: Res<PedestrianAnimations>,
    controlled: Res<ControlledCharacter>,
    mut commands: Commands,
    mut query: Query<
        (&LinearVelocity, Has<Grounded>, &MovementModifiers, &mut AnimState),
        With<CharacterController>,
    >,
) {
    if !anims.ready {
        return;
    }
    let Some(ped) = controlled.ped else {
        return;
    };
    let Some(controller) = controlled.controller else {
        return;
    };
    let Ok((velocity, grounded, modifiers, mut state)) = query.get_mut(controller) else {
        return;
    };

    let dt = time.delta_secs();
    if state.timer > 0.0 {
        state.timer -= dt;
    }

    // Advance the jump phase state machine.
    let just_airborne = !grounded && matches!(state.phase, JumpPhase::Grounded | JumpPhase::Land);
    let just_landed = grounded && matches!(state.phase, JumpPhase::Start | JumpPhase::Loop);
    if just_airborne {
        state.phase = JumpPhase::Start;
        state.timer = JUMP_START_TIME;
    } else if just_landed {
        state.phase = JumpPhase::Land;
        state.timer = JUMP_LAND_TIME;
    } else {
        match state.phase {
            JumpPhase::Start if state.timer <= 0.0 => state.phase = JumpPhase::Loop,
            JumpPhase::Land if state.timer <= 0.0 => state.phase = JumpPhase::Grounded,
            _ => {}
        }
    }

    let horizontal_speed = Vec2::new(velocity.x as f32, velocity.z as f32).length();
    let moving = horizontal_speed > MOVE_ANIM_THRESHOLD;

    // Choose the clip candidates for the current state.
    let candidates: &[&str] = match state.phase {
        JumpPhase::Start => &["Jump_Start"],
        JumpPhase::Loop => &["Jump_Loop"],
        JumpPhase::Land => &["Jump_Land"],
        JumpPhase::Grounded => {
            if modifiers.crouch {
                if moving {
                    &["Crouch_Fwd_Loop"]
                } else {
                    &["Crouch_Idle_Loop", "Idle_Loop"]
                }
            } else if moving {
                if horizontal_speed < WALK_MAX_SPEED {
                    &["Walk_Loop"]
                } else if horizontal_speed < JOG_MAX_SPEED {
                    &["Jog_Fwd_Loop"]
                } else {
                    &["Sprint_Loop", "Sprint_Fwd_Loop"]
                }
            } else {
                &["Idle_Loop", "A_TPose"]
            }
        }
    };

    let Some(desired) = resolve_anim(&anims, candidates) else {
        return;
    };

    if state.current.as_deref() != Some(desired.as_str()) {
        state.current = Some(desired.clone());
        commands.trigger(PedestrianAnimationControlEvent {
            ped,
            animation: desired,
            speed: 1.0,
        });
    }
}

// ---------------------------------------------------------------------------------------------
// egui pedestrian list
// ---------------------------------------------------------------------------------------------

fn ui_pedestrian_list(
    mut commands: Commands,
    mut contexts: EguiContexts,
    manifest: Res<PedestrianManifest>,
    controlled: Res<ControlledCharacter>,
    model_roots: Query<&ModelRoot>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let current_name = controlled
        .ped
        .and_then(|ped| model_roots.get(ped).ok())
        .map(|root| root.name.clone());

    egui::Window::new("Pedestrian Controller")
        .default_pos(egui::pos2(12.0, 50.0))
        .default_size(egui::vec2(280.0, 380.0))
        .show(ctx, |ui| {
            ui.label("Controls:");
            ui.label("WASD move · Space jump · C crouch · Shift sprint");
            ui.separator();

            match &current_name {
                Some(name) => ui.label(format!("Controlling: {name}")),
                None => ui.label("Controlling: (spawning…)"),
            };

            ui.separator();
            if !manifest.loaded {
                ui.label("Loading manifest…");
                return;
            }
            ui.label(format!("Pedestrians ({}):", manifest.urls.len()));

            egui::ScrollArea::vertical().show(ui, |ui| {
                for url in &manifest.urls {
                    let name = url
                        .0
                        .split('/')
                        .next_back()
                        .unwrap_or(&url.0)
                        .replace(".glb", "");
                    ui.horizontal(|ui| {
                        if ui.button("Spawn").clicked() {
                            commands.trigger(SpawnControlledEvent { url: url.clone() });
                        }
                        ui.label(name);
                    });
                }
            });
        });
}

// ---------------------------------------------------------------------------------------------
// Random physics cubes
// ---------------------------------------------------------------------------------------------

fn spawn_physics_cubes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let material = materials.add(Color::srgb_u8(124, 144, 255));

    for i in 0..6 {
        let x = rand::random::<f32>() * 12.0 - 6.0;
        let z = rand::random::<f32>() * 12.0 - 6.0;
        commands.spawn((
            Name::new("PhysicsCube"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, 3.0 + i as f32 * 1.5, z),
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0, 1.0),
            // The debug ground only collides with Car/Wheel layers, so cubes must be on Car to rest
            // on it (and to collide with each other / the character capsule).
            CollisionLayers::new(
                GamePhysicsLayer::Car,
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
        ));
    }
}
