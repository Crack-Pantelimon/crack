//! Standalone demo harness for the [`PedestrianControllerPlugin`].
//!
//! Sets up a flat debug scene with some physics cubes, then auto-spawns a random controllable
//! pedestrian once the manifest loads. All the controller logic lives in the reusable library
//! module `plugins::pedestrians::pedestrian_controller_plugin` (also used by the main game).
//!
//! Controls: WASD move · Space jump · C crouch · Shift sprint · LMB jab · RMB(hold) aim ·
//! LMB+RMB shoot · Esc back to freecam. In freecam, right-click to open the spawn popup.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        cars_driving::driving_plugin::GamePhysicsLayer,
        pedestrians::{
            pedestrian_controller_plugin::{
                PedestrianControllerPlugin, SpawnControlledPedestrianEvent,
            },
            PedestrianManifest, PedestriansPlugin,
        },
        states::GameControlState,
    },
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("Pedestrian Controller")
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(PhysicsDebugPlugin::default())
        .init_state::<GameControlState>()
        .add_plugins(PedestriansPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(PedestrianControllerPlugin)
        .add_systems(Startup, spawn_physics_cubes)
        .add_systems(Update, demo_auto_spawn)
        .run();
}

/// Auto-spawn a random controllable pedestrian at the origin once the manifest is ready.
fn demo_auto_spawn(
    mut commands: Commands,
    manifest: Res<PedestrianManifest>,
    mut done: Local<bool>,
) {
    if *done || !manifest.loaded {
        return;
    }
    commands.trigger(SpawnControlledPedestrianEvent {
        position: Vec3::ZERO,
        url: None,
    });
    *done = true;
}

/// A few dynamic cubes to walk into and shove around.
fn spawn_physics_cubes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let material = materials.add(Color::srgb_u8(124, 144, 255));

    for i in 0..6 {
        let x = rand::random::<f32>() * 12.0 - 6.0;
        let z = rand::random::<f32>() * 12.0 - 6.0;
        commands.spawn((
            Name::new("PhysicsCube"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, 3.0 + i as f32 * 1.5, z),
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0, 1.0),
            // The debug ground only collides with Car/Wheel layers, so cubes must be on Car.
            CollisionLayers::new(
                GamePhysicsLayer::Car,
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
        ));
    }
}
