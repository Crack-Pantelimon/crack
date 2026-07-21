//! Fane demo binary: minimal scene for the Fane web entry point.

use bevy::prelude::*;
use demo_resolution_selector_web_bevy::basic_app::make_basic_app;
use demo_resolution_selector_web_bevy::utils::setup_debug_scene::SetupDebugScenePlugin;

fn main() {
    make_basic_app("Fane")
        .add_systems(Startup, scene.spawn())
        .add_plugins(SetupDebugScenePlugin)
        .run();
}

/// set up a simple 3D scene
fn scene() -> impl SceneList {
    bsn_list! [

        (
            #Cube
            Mesh3d(asset_value(Cuboid::new(1.0, 1.0, 1.0)))
            MeshMaterial3d::<StandardMaterial>(asset_value(Color::srgb_u8(124, 144, 255)))
            Transform::from_xyz(0.0, 0.5, 0.0)
        ),
    ]
}
