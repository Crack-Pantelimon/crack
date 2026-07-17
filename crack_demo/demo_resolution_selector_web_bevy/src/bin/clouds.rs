//! Standalone cloud skybox playground.
//!
//! Loads the flat debug scene (which brings in the procedural cloud sky) and
//! opens the cloud controls window, since there is no Debug menu here. See
//! [`demo_resolution_selector_web_bevy::plugins::cloud_sky`] for the details.

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app, ui_egui::UiState, utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("Clouds")
        .add_plugins(bevy_egui::EguiPlugin::default())
        // No menu bar in this harness, so keep the cloud controls always open.
        .insert_resource({
            let mut state = UiState::default();
            state.show_clouds_sky = true;
            state
        })
        .add_plugins(SetupDebugScenePlugin)
        .run();
}
