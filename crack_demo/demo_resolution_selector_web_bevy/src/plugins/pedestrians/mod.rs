//! Pedestrian engine plugin.
//!
//! Loads a manifest of rigged pedestrian GLB models, spawns them on request, classifies their
//! skeletons, and drives their animations via events. Reusable by any consumer (viewer binary or
//! the main simulation).
//!
//! # API
//! - On startup the plugin loads the manifest into [`PedestrianManifest`] (`urls`, `loaded`) and
//!   builds the animation catalog in [`PedestrianAnimations`].
//! - Trigger [`SpawnPedestrianEvent`] `{ url, position }` to spawn a pedestrian.
//! - Trigger [`PedestrianAnimationControlEvent`] `{ ped, animation, speed }` to drive one.
//! - Toggle [`SkeletonDebug`] `show` to draw the colored skeleton gizmos.

pub mod animation;
pub mod draw_skel_debug;
pub mod manifest;
pub mod pedestrian_controller_plugin;
pub mod skeleton;
pub mod spawn_pedestrian;

use bevy::prelude::*;

pub use animation::{
    ActiveOneShot, AnimationInfo, ManualAnimation, PedestrianAnimationControlEvent,
    PedestrianAnimations, TargetAnimation, PlayOnceAnimation,
};
pub use draw_skel_debug::SkeletonDebug;
pub use manifest::{PedestrianManifest, PedestrianUrl};
pub use spawn_pedestrian::{ModelRoot, SpawnPedestrianEvent};

use animation::{
    pedestrian_animation_control_observer, play_animations_system, setup_animation_players_system,
};
use draw_skel_debug::draw_skeletons_system;
use manifest::{
    load_pedestrian_manifest_system, poll_pedestrian_first_glb_task, poll_pedestrian_manifest_task,
    spawn_pedestrian_manifest_task, start_manifest_load,
};
use spawn_pedestrian::{
    PedestrianSpawnCounter, init_pedestrians_system, link_pedestrian_model,
    poll_pedestrian_glb_fetches, spawn_pedestrian_observer,
};

pub struct PedestriansPlugin;

impl Plugin for PedestriansPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PedestrianManifest>()
            .init_resource::<PedestrianAnimations>()
            .init_resource::<PedestrianSpawnCounter>()
            .init_resource::<SkeletonDebug>()
            .add_observer(spawn_pedestrian_observer)
            .add_observer(pedestrian_animation_control_observer)
            .add_systems(Startup, start_manifest_load)
            .add_systems(
                Update,
                (
                    spawn_pedestrian_manifest_task,
                    poll_pedestrian_manifest_task,
                    poll_pedestrian_first_glb_task,
                    load_pedestrian_manifest_system,
                    poll_pedestrian_glb_fetches,
                    init_pedestrians_system,
                    link_pedestrian_model,
                    setup_animation_players_system,
                    play_animations_system,
                    draw_skeletons_system,
                ),
            );
    }
}
