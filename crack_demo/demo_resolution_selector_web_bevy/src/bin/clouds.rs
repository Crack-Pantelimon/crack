//! Standalone cloud skybox playground.
//!
//! Loads the flat debug scene and renders a fully procedural sky: blue-sky
//! gradient with sun and day/night cycle, three cloud layer types
//! (cumulus / cirrus / storm), rain & snow overlay, and scrolling cloud
//! shadows on the ground. See
//! [`demo_resolution_selector_web_bevy::plugins::cloud_sky`] for the details.

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::cloud_sky::CloudSkyPlugin,
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("Clouds")
        .add_plugins(bevy_egui::EguiPlugin::default())
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(CloudSkyPlugin)
        .run();
}
