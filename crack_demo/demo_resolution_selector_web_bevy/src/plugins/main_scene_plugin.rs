use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

pub struct MainScenePlugin;

impl Plugin for MainScenePlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MainScenePlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MainScenePlugin...");
        app.add_systems(
            Startup,
            (setup_camera_and_load, || {
                crate::ui_egui::web_set_loading_status(false, "");
            }),
        );
        info!("done loading: MainScenePlugin");
    }
}

fn setup_camera_and_load(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Keep only default camera spawning
    commands.spawn((
        Transform::from_xyz(0.0, 10.5, -30.0).looking_at(Vec3::ZERO, Vec3::Y),
        Camera {
            clear_color: Color::BLACK.into(),
            ..default()
        },
        Camera3d::default(),
        Tonemapping::None,
    ));

    // Spawn directional light (sun)
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Load and spawn custom assets next to each other
    let base_url = crate::config::DATA_BASE_URL;

    // 1. Kebab Shop (Height = 3.0m, offset up by 1.5m, moved to Z = -20125.0)
    let handle_kebab = asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!("{}/blender_generated/kebab_shop/kebab_shop.glb", base_url)));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_kebab),
        Transform::from_translation(Vec3::new(-1050.0, 3363.0, -20125.0)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));

    // 2. Superbet Shop (Height = 3.0m, offset up by 1.5m, moved to Z = -20135.0)
    let handle_superbet = asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!("{}/blender_generated/superbet_shop/superbet_shop.glb", base_url)));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_superbet),
        Transform::from_translation(Vec3::new(-1050.0, 3363.0, -20135.0)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));

    // 3. Terasa Obor (Height = 3.5m, offset up by 1.75m, moved to Z = -20125.0)
    let handle_obor = asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!("{}/blender_generated/terasa_obor/terasa_obor.glb", base_url)));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_obor),
        Transform::from_translation(Vec3::new(-1070.0, 3363.25, -20125.0)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));

    // 4. Bus 335 (Height = 2.8m, offset up by 1.4m, moved to Z = -20135.0)
    let handle_bus = asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!("{}/blender_generated/bus_335/bus_335.glb", base_url)));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_bus),
        Transform::from_translation(Vec3::new(-1070.0, 3362.9, -20135.0)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));
}
