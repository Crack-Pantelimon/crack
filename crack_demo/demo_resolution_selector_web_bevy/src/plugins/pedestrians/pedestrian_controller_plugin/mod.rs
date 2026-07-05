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
mod spawn;

use avian3d::{math::*, prelude::*};
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::plugins::states::GameControlState;

pub use spawn::SpawnControlledPedestrianEvent;

use animation::{drive_character_animation, print_animation_catalog};
use camera::{follow_camera, orbit_camera_input};
use controller::{
    apply_forces_to_dynamic_bodies, apply_gravity, apply_movement_damping, apply_speed_cap,
    character_input, face_movement, move_and_slide, movement, respawn_if_fallen, update_grounded,
};
use interaction_ui::{handle_freecam_right_click, spawn_choice_popup_ui};
use spawn::{
    adopt_pedestrian, escape_to_freecam, spawn_controlled_pedestrian_observer, ControlledCharacter,
    SpawnChoicePopup,
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

// Movement. Acceleration is deliberately high so the per-mode speed *caps* are the binding limit.
const MOVE_ACCEL: f32 = 200.0;
const MOVE_DAMPING: f32 = 12.0;
const JUMP_IMPULSE: f32 = 7.5;
const GRAVITY_Y: f32 = -9.81 * 2.0;
/// Per-mode horizontal speed caps.
const CROUCH_SPEED: f32 = 1.8;
const JOG_SPEED: f32 = 4.0;
/// Sprint ramps from `2 * JOG_SPEED` up to `SPRINT_MAX_MULT * JOG_SPEED` while Shift is held.
const SPRINT_MAX_MULT: f32 = 3.0;
const SPRINT_RAMP_TIME: f32 = 2.5;

// Animation selection by current horizontal speed.
const MOVE_ANIM_THRESHOLD: f32 = 0.25;
const WALK_MAX_SPEED: f32 = 2.2;
const JOG_MAX_SPEED: f32 = 6.0;

// Jump animation phase timings (seconds).
const JUMP_START_TIME: f32 = 0.22;
const JUMP_LAND_TIME: f32 = 0.22;

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
/// Pitch is clamped so the camera stays above the character and never flips.
const CAM_PITCH_MIN: f32 = -1.4;
const CAM_PITCH_MAX: f32 = -0.05;

// ---------------------------------------------------------------------------------------------
// Shared components / resources / messages
// ---------------------------------------------------------------------------------------------

/// A [`Message`] written for a movement input action.
#[derive(Message)]
pub enum MovementAction {
    /// Desired planar move direction, mapped as the avian example does: `x -> +x`, `y -> -z`.
    Move(Vector2),
    Jump,
}

/// Marker for the kinematic character body. Requires a kinematic rigid body and disables Avian's
/// automatic position integration so move-and-slide drives the transform manually.
#[derive(Component)]
#[require(RigidBody::Kinematic, CustomPositionIntegration, SpeculativeMargin(0.0))]
pub struct CharacterController;

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
    /// One-shot punch; reverts to `None` when finished.
    Jab,
    /// Looping aim pose held while RMB is down.
    Aim,
    /// One-shot shot; reverts to `Aim` (if RMB still held) when finished.
    Shoot,
}

// ---------------------------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------------------------

pub struct PedestrianControllerPlugin;

impl Plugin for PedestrianControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MovementAction>()
            .init_resource::<ControlledCharacter>()
            .init_resource::<camera::CameraRig>()
            .init_resource::<SpawnChoicePopup>()
            .add_observer(spawn_controlled_pedestrian_observer)
            // Runs in every state: log the catalog once, and manage the freecam right-click popup.
            .add_systems(Update, print_animation_catalog)
            .add_systems(
                Update,
                handle_freecam_right_click.run_if(in_state(GameControlState::MapFreecam)),
            )
            .add_systems(
                EguiPrimaryContextPass,
                spawn_choice_popup_ui.run_if(in_state(GameControlState::MapFreecam)),
            )
            // Input before the physics step.
            .add_systems(
                PreUpdate,
                character_input.run_if(in_state(GameControlState::ControllingPedestrian)),
            )
            // Movement in FixedUpdate for frame-rate independence.
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
                    .chain()
                    .run_if(in_state(GameControlState::ControllingPedestrian)),
            )
            .add_systems(
                Update,
                (
                    adopt_pedestrian,
                    respawn_if_fallen,
                    face_movement,
                    orbit_camera_input,
                    follow_camera,
                    drive_character_animation,
                    escape_to_freecam,
                )
                    .run_if(in_state(GameControlState::ControllingPedestrian)),
            );
    }
}
