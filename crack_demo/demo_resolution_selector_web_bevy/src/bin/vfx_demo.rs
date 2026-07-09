//! Standalone VFX playground.
//!
//! Loads the flat debug scene, and lets you spawn any visual effect at the clicked 3D point.
//! See [`demo_resolution_selector_web_bevy::plugins::visual_fx`] for the details.

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        physics_plugin::PhysicsPlugin,
        visual_fx::{VisualFXPlugin, demo::VfxDemoPlugin},
    },
    ui_egui::UiState,
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("VFX Demo")
        .add_plugins(bevy_egui::EguiPlugin::default())
        // PhysicsPlugin's sync_physics_debug_config reads UiState.
        // We set show_vfx_shaders to true by default in our demo harness so controls are open.
        .insert_resource({
            let mut state = UiState::with_physics_debug();
            state.show_vfx_shaders = true;
            state
        })
        // Needed for SpatialQuery raycasting against the debug ground colliders.
        .add_plugins(PhysicsPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(VisualFXPlugin)
        .add_plugins(VfxDemoPlugin)
        .run();
}
