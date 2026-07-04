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
pub mod ragdoll;
pub mod skeleton;
pub mod spawn_pedestrian;

use bevy::prelude::*;

pub use animation::{AnimationInfo, PedestrianAnimationControlEvent, PedestrianAnimations};
pub use draw_skel_debug::SkeletonDebug;
pub use manifest::{PedestrianManifest, PedestrianUrl};
pub use ragdoll::{RAGDOLL_ANIMATION, RagdollActive, RagdollReady};
pub use spawn_pedestrian::{ModelRoot, SpawnPedestrianEvent};

use animation::{
    play_animations_system, pedestrian_animation_control_observer, setup_animation_players_system,
};
use draw_skel_debug::draw_skeletons_system;
use manifest::{TextAsset, TextAssetLoader, load_pedestrian_manifest_system, start_manifest_load};
use ragdoll::{
    compute_ragdoll_constraints_system, ragdoll_enable_joints_system, ragdoll_toggle_system,
    spawn_ragdoll_bodies_system,
};
use spawn_pedestrian::{PedestrianSpawnCounter, init_pedestrians_system, spawn_pedestrian_observer};

pub struct PedestriansPlugin;

impl Plugin for PedestriansPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<TextAsset>()
            .init_asset_loader::<TextAssetLoader>()
            .init_resource::<PedestrianManifest>()
            .init_resource::<PedestrianAnimations>()
            .init_resource::<PedestrianSpawnCounter>()
            .init_resource::<SkeletonDebug>()
            .add_observer(spawn_pedestrian_observer)
            .add_observer(pedestrian_animation_control_observer)
            .add_systems(Startup, start_manifest_load)
            .add_systems(
                Update,
                (
                    load_pedestrian_manifest_system,
                    init_pedestrians_system,
                    setup_animation_players_system,
                    play_animations_system,
                    draw_skeletons_system,
                    compute_ragdoll_constraints_system,
                    spawn_ragdoll_bodies_system,
                    ragdoll_toggle_system,
                    ragdoll_enable_joints_system,
                ),
            );
    }
}
