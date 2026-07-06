//! Shared character locomotion physics plugin.
//!
//! Extracted from [`super::PedestrianControllerPlugin`] so both the player controller and the AI
//! can share the same per-entity physics chain without double-registration. Both plugins guard-add
//! this one:
//! ```ignore
//! if !app.is_plugin_added::<CharacterLocomotionPlugin>() {
//!     app.add_plugins(CharacterLocomotionPlugin);
//! }
//! ```

use bevy::prelude::*;

use super::no_one_climbing;
use crate::plugins::pedestrians::pedestrian_controller_plugin::controller::{
    apply_forces_to_dynamic_bodies, apply_gravity, apply_movement_damping, apply_speed_cap,
    face_movement, move_and_slide, movement, respawn_if_fallen, update_climb, update_grounded,
    update_roll, detect_fallen_off_map,
};

/// Registers the **un-gated** per-entity locomotion physics chain for all
/// [`CharacterController`](super::CharacterController) entities (player and AI alike).
pub struct CharacterLocomotionPlugin;

impl Plugin for CharacterLocomotionPlugin {
    fn build(&self, app: &mut App) {
        // Movement in FixedUpdate for frame-rate independence. Skipped while any character
        // is mid-climb (the climb tween owns the transform then).
        app.add_systems(
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
                .run_if(no_one_climbing),
        )
        .add_systems(
            Update,
            (
                face_movement,
                update_climb,
                update_roll,
                respawn_if_fallen,
                detect_fallen_off_map,
            ),
        );
    }
}
