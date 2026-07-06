//! Standalone 3D-audio playground.
//!
//! Loads the flat debug scene, fetches the sound-fx manifest and lets you fire any clip in 3D by
//! clicking the ground. See [`demo_resolution_selector_web_bevy::plugins::audio`] for the UI +
//! playback details.

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{audio::AudioDemoPlugin, physics_plugin::PhysicsPlugin},
    ui_egui::UiState,
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("Audio Demo")
        .add_plugins(bevy_egui::EguiPlugin::default())
        // PhysicsPlugin's `sync_physics_debug_config` reads `UiState`.
        .insert_resource(UiState::with_physics_debug())
        // Needed for `SpatialQuery` raycasting against the debug ground colliders.
        .add_plugins(PhysicsPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(AudioDemoPlugin)
        .run();
}
