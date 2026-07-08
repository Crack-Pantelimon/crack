//! Playable kinematic pedestrian controller.
//!
//! A pill-shaped (capsule) kinematic character controller (ported from the avian3d
//! `kinematic_character_3d` example) moves on WASD, jumps on Space, crouches on `C` and sprints on
//! `Shift`. The visible pedestrian model (spawned by [`crate::plugins::pedestrians`]) is parented
//! *under* the controller as a purely visual child; the only physics body is the capsule. The
//! controller yaws toward its movement direction so the single forward-facing locomotion clips work
//! for movement in any direction.
//!
//! Animations are driven directly on the model's [`AnimationPlayer`] so a locomotion clip and a
//! combat clip can play *at the same time* (LMB jab, RMB-hold aim, LMB+RMB shoot layered on top of
//! walking/crouching/sprinting/jumping).
//!
//! Integration with the main game runs through [`crate::plugins::states::GameControlState`]:
//! right-clicking the map in freecam pops up a "spawn pedestrian / spawn car" choice; choosing the
//! pedestrian enters [`GameControlState::ControllingPedestrian`], and Escape returns to freecam.
//!
//! The plugin does not add `PhysicsPlugins`, `EguiPlugin`, or `PedestriansPlugin` — the host app is
//! expected to provide those (the main game does via its physics/egui/states plugins).

mod animation;
mod camera;
mod controller;
mod interaction_ui;
pub mod locomotion;
mod spawn;

use avian3d::{math::*, prelude::*};
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::plugins::states::GameControlState;

pub use interaction_ui::{
    CarSeatOffset, DriverMesh, EjectedDriver, EjectedStage, eject_driver_as_ai,
    tick_ejected_driver_system,
};
pub use spawn::{ControlledCharacter, SpawnControlledPedestrianEvent};

use animation::{drive_character_animation, print_animation_catalog};
use camera::{follow_camera, orbit_camera_input};
use controller::{character_input, jump_or_climb};
use interaction_ui::{
    WeaponSelection, apply_seat_offset, car_seat_debug_ui, crosshair_ui, detect_car_interaction,
    drive_driver_mesh_animation, equip_on_new_character, handle_exit_car,
    handle_freecam_right_click, spawn_choice_popup_ui, tick_driver_mesh_exit, tick_entering_car,
    weapon_hud_ui, weapon_wheel,
};
use locomotion::CharacterLocomotionPlugin;
pub use spawn::{
    SpawnChoicePopup, escape_to_freecam, player_death_to_freecam,
    spawn_controlled_pedestrian_observer, DeathProp, tick_death_props,
    setup_death_prop_animations,
};

// ---------------------------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------------------------

/// Capsule dimensions (radius + straight cylinder length). Total height = length + 2*radius.
pub const CAPSULE_RADIUS: f32 = 0.35;
pub const CAPSULE_LENGTH: f32 = 1.0;
/// Distance from capsule center to its bottom tip; used to sit the model's feet on the ground.
pub const CAPSULE_HALF_HEIGHT: f32 = CAPSULE_LENGTH / 2.0 + CAPSULE_RADIUS;
/// Full capsule height (tip to tip).
pub const CAPSULE_TOTAL_HEIGHT: f32 = CAPSULE_LENGTH + 2.0 * CAPSULE_RADIUS;

/// Character mesh scale range. Every spawn picks a scale in this range (or clamps the requested one);
/// the mesh is scaled by an intermediate node so animations are unaffected.
pub const SCALE_MIN: f32 = 0.8;
pub const SCALE_MAX: f32 = 1.0;

// Movement. Acceleration is deliberately high so the per-mode speed *caps* are the binding limit.
const MOVE_ACCEL: f32 = 200.0;
const MOVE_DAMPING: f32 = 12.0;
const JUMP_IMPULSE: f32 = 7.5;
const GRAVITY_Y: f32 = -9.81 * 2.0;
/// Per-mode horizontal speed caps.
const CROUCH_SPEED: f32 = 1.8;
const JOG_SPEED: f32 = 4.0;
/// Sprint ramps from `1 * JOG_SPEED` up to `SPRINT_MAX_MULT * JOG_SPEED` while Shift is held.
const SPRINT_MAX_MULT: f32 = 1.5;
const SPRINT_RAMP_TIME: f32 = 2.5;

// Animation selection by current horizontal speed.
const MOVE_ANIM_THRESHOLD: f32 = 0.25;
const WALK_MAX_SPEED: f32 = 2.0;
const JOG_MAX_SPEED: f32 = 4.5;

// Jump animation phase timings (seconds).
const JUMP_START_TIME: f32 = 0.22;
const JUMP_LAND_TIME: f32 = 0.22;

// Climbing. A ledge is climbable if its top is between these fractions of the character's height
// above the feet. Detection uses a few forward/down rays; climbing works even while airborne.
const CLIMB_MIN_FRAC: f32 = 0.3;
const CLIMB_MAX_FRAC: f32 = 1.2;
/// How far in front of the capsule surface to probe for a ledge.
const CLIMB_FORWARD_REACH: f32 = 0.5;
/// Duration of the climb motion (up-then-over).
const CLIMB_DURATION: f32 = 0.6;
/// Speed multiplier for the climb/roll animation clip (the Roll clip is too long at 1x).
const ROLL_ANIM_SPEED_MULT: f32 = 2.0;

// Crouch roll (crouch + Space): a short forward dash with the Roll animation.
const ROLL_SPEED: f32 = 5.0;
const ROLL_DURATION: f32 = 0.7;
/// Sprinting while crouched (crouch + Shift) doubles the crouch speed cap.
const CROUCH_SPRINT_MULT: f32 = 2.0;

/// How fast the controller turns to face its movement direction (higher = snappier).
const TURN_SPEED: f32 = 12.0;
/// Yaw offset applied on top of the movement direction, if the model's forward axis is not +Z.
const MODEL_FORWARD_OFFSET: f32 = 0.0;

// Follow camera. Position trails the character; orientation is manual (left-mouse drag).
const CAM_DISTANCE: f32 = 5.5;
const CAM_LOOK_HEIGHT: f32 = 1.1;
/// Time constant for smoothing the *character-driven* follow position. This attenuates the wild
/// up/down/left/right shake the kinematic controller picks up from the rough map, while leaving
/// user-driven (mouse-drag) camera rotation completely un-attenuated.
const CAM_FOLLOW_SMOOTH_TIME: f32 = 0.15;
/// If the character jumps further than this in one frame (respawn / new spawn), snap instead of
/// smoothing.
const CAM_FOLLOW_SNAP_DIST: f32 = 5.0;
/// Initial (and default) camera pitch — slightly downward.
const CAM_PITCH: f32 = -0.35;
/// Mouse-drag orbit sensitivity (radians per pixel).
const CAM_ORBIT_SENSITIVITY: f32 = 0.006;
/// Pitch clamp limits: -85 degrees min to +85 degrees max.
const CAM_PITCH_MIN: f32 = -85.0 * (std::f32::consts::PI / 180.0);
const CAM_PITCH_MAX: f32 = 85.0 * (std::f32::consts::PI / 180.0);

// ---------------------------------------------------------------------------------------------
// Shared components / resources
// ---------------------------------------------------------------------------------------------

/// Per-entity desired locomotion, written by whoever drives this controller
/// (keyboard for the player, the AI brain for NPCs). Consumed by `movement`.
#[derive(Component, Default)]
pub struct LocomotionInput {
    /// Planar move direction, avian convention: `x -> +x`, `y -> -z`. Zero = no input.
    pub move_dir: Vec2,
    /// Set true for one frame to request a jump; `movement` consumes and clears it.
    pub jump: bool,
}

/// Marker for the kinematic character body. Requires a kinematic rigid body and disables Avian's
/// automatic position integration so move-and-slide drives the transform manually.
#[derive(Component)]
#[require(
    RigidBody::Kinematic,
    CustomPositionIntegration,
    SpeculativeMargin(0.0)
)]
pub struct CharacterController;

/// The random mesh scale chosen for this character (in `[SCALE_MIN, SCALE_MAX]`). Used to speed up
/// locomotion animations for shorter characters and to size climb-height thresholds.
#[derive(Component, Clone, Copy)]
pub struct CharacterScale(pub f32);

/// Held movement modifiers, updated from the keyboard each frame.
#[derive(Component, Default)]
pub struct MovementModifiers {
    pub crouch: bool,
    pub sprint: bool,
    /// Seconds the sprint has been held continuously (drives the sprint speed ramp).
    pub sprint_secs: f32,
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

/// Marker for a character currently standing on ground.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Grounded;

/// An in-progress ledge climb. While present, the normal movement chain is skipped and the
/// controller transform is tweened up-then-over onto the ledge.
#[derive(Component)]
pub struct Climbing {
    pub start: Vec3,
    pub target: Vec3,
    pub elapsed: f32,
    pub duration: f32,
}

/// An in-progress crouch roll (crouch + Space): a short forward dash with the Roll animation.
#[derive(Component)]
pub struct Rolling {
    pub elapsed: f32,
    pub duration: f32,
}

/// Per-frame collisions recorded by move-and-slide, used to push dynamic bodies.
#[derive(Component, Default, Deref)]
pub struct CharacterCollisions(Vec<CharacterCollision>);

pub struct CharacterCollision {
    pub collider: Entity,
    pub point: Vector,
    pub normal: Dir3,
    pub character_velocity: Vector,
}

/// Jump animation phase.
#[derive(Clone, Copy, PartialEq)]
pub enum JumpPhase {
    Grounded,
    Start,
    Loop,
    Land,
}

/// Base locomotion animation state, stored on the controller.
#[derive(Component)]
pub struct AnimState {
    /// The graph node of the base locomotion clip currently playing.
    pub base_node: Option<AnimationNodeIndex>,
    pub phase: JumpPhase,
    pub timer: f32,
    /// True once we have taken over the model's `AnimationPlayer` (cleared its default clip).
    pub took_over: bool,
}

impl Default for AnimState {
    fn default() -> Self {
        Self {
            base_node: None,
            phase: JumpPhase::Grounded,
            timer: 0.0,
            took_over: false,
        }
    }
}

/// Combat overlay animation state, stored on the controller.
#[derive(Component, Default)]
pub struct CombatState {
    /// The graph node of the combat clip currently overlaid, if any.
    pub node: Option<AnimationNodeIndex>,
    /// The kind of overlay currently playing.
    pub kind: CombatKind,
}

#[derive(Default, Clone, Copy, PartialEq)]
pub enum CombatKind {
    #[default]
    None,
    /// A one-shot attack (punch / sword swing / gun shot); reverts when finished.
    OneShot,
    /// Looping aim pose held while RMB is down (guns only).
    Aim,
}

// ---------------------------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------------------------

pub struct PedestrianControllerPlugin;

impl Plugin for PedestrianControllerPlugin {
    fn build(&self, app: &mut App) {
        // Guard-add the shared locomotion plugin (AI plugin may also add it).
        if !app.is_plugin_added::<CharacterLocomotionPlugin>() {
            app.add_plugins(CharacterLocomotionPlugin);
        }

        app.init_resource::<ControlledCharacter>()
            .init_resource::<camera::CameraRig>()
            .init_resource::<SpawnChoicePopup>()
            .init_resource::<CarSeatOffset>()
            .init_resource::<WeaponSelection>()
            .add_observer(spawn_controlled_pedestrian_observer)
            // Runs in every state: log the catalog once, manage death peds, and manage the freecam right-click popup.
            .add_systems(
                Update,
                (
                    print_animation_catalog,
                    equip_on_new_character,
                    tick_death_props,
                    setup_death_prop_animations,
                ),
            )
            .add_systems(
                Update,
                handle_freecam_right_click.run_if(in_state(GameControlState::MapFreecam)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                spawn_choice_popup_ui.run_if(in_state(GameControlState::MapFreecam)),
            )
            // Input before the physics step. `jump_or_climb` decides Space -> jump vs climb.
            .add_systems(
                PreUpdate,
                (character_input, jump_or_climb)
                    .run_if(in_state(GameControlState::ControllingPedestrian)),
            )
            .add_systems(
                Update,
                (
                    tick_ejected_driver_system,
                    orbit_camera_input,
                    follow_camera,
                    drive_character_animation,
                    escape_to_freecam,
                    player_death_to_freecam,
                    detect_car_interaction,
                    tick_entering_car,
                    weapon_wheel,
                )
                    .run_if(in_state(GameControlState::ControllingPedestrian)),
            )
            .add_systems(
                Update,
                handle_exit_car.run_if(in_state(GameControlState::DrivingCar)),
            )
            // Driver-mesh systems run in every state: the mesh exists while DrivingCar,
            // and the exit slide finishes across the state change back to pedestrian.
            .add_systems(
                Update,
                (
                    drive_driver_mesh_animation,
                    apply_seat_offset,
                    tick_driver_mesh_exit,
                ),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    crosshair_ui.run_if(in_state(GameControlState::ControllingPedestrian)),
                    weapon_hud_ui.run_if(in_state(GameControlState::ControllingPedestrian)),
                    car_seat_debug_ui.run_if(in_state(GameControlState::DrivingCar)),
                ),
            );
    }
}

/// Run condition: true when no character is mid-climb (so the movement chain can run).
pub fn no_one_climbing(q: Query<(), With<Climbing>>) -> bool {
    q.is_empty()
}

/// The shared physics/locomotion core for every capsule character (player, AI ped, ejected
/// driver). Callers add their role-specific components on top: the player adds
/// [`AnimState`]/[`CombatState`], the AI adds its `Ai*` components. Keeping this in one place stops
/// the three spawn sites from drifting apart (e.g. wrong `RigidBody`, missing collision layers).
pub fn character_physics_bundle(scale: f32, transform: Transform) -> impl Bundle {
    use crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer;
    (
        // `CharacterController` requires `RigidBody::Kinematic` + custom position integration, so
        // no explicit `RigidBody` is added here (a `Dynamic` body would fight move-and-slide).
        CharacterController,
        CharacterScale(scale),
        CharacterMovementSettings::default(),
        CharacterCollisions::default(),
        MovementModifiers::default(),
        LocomotionInput::default(),
        GroundDetection {
            cast_shape: Some(Collider::capsule(CAPSULE_RADIUS * 0.99, CAPSULE_LENGTH)),
            ..default()
        },
        Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
        CollisionLayers::new(
            GamePhysicsLayer::Car,
            [
                GamePhysicsLayer::Map,
                GamePhysicsLayer::Car,
                GamePhysicsLayer::Wheel,
            ],
        ),
        CollisionEventsEnabled,
        transform,
        Visibility::default(),
    )
}
