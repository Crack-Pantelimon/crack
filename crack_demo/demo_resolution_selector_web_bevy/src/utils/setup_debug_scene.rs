//! Implement `SetupDebugScenePlugin` that will spawn some flat ground, a camera and the sun.

use bevy::prelude::*;

use avian3d::prelude::{Collider, CollisionLayers, Restitution, RigidBody};

use crate::{
    plugins::cars_driving::driving_plugin::GamePhysicsLayer,
    plugins::pedestrians::pedestrian_controller_plugin::MainCamera,
    utils::create_texture::create_grayscale_texture,
};

/// This plugin will create simple implementation for ground, scene and camera. There is no camera controller.
/// It also brings in the procedural cloud sky ([`crate::plugins::cloud_sky::CloudSkyPlugin`]).
pub struct SetupDebugScenePlugin;

impl Plugin for SetupDebugScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::plugins::cloud_sky::CloudSkyPlugin)
            .add_systems(PreStartup, setup_scene);
    }
}

/// Component attached to debug ground texture.
#[derive(Component)]
pub struct DebugSceneGroundComponent;

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut gizmo_store: ResMut<GizmoConfigStore>,
) {
    let (config, _) = gizmo_store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;

    let cubes_info = [
        (Vec3::new(250.0, -250.0, 250.0), (50, 70)),
        (Vec3::new(-250.0, -250.0, 250.0), (90, 110)),
        (Vec3::new(250.0, -250.0, -250.0), (130, 150)),
        (Vec3::new(-250.0, -250.0, -250.0), (170, 190)),
    ];

    for (center, (gray1, gray2)) in cubes_info {
        let tile_repeat: f32 = 1.0 + rand::random::<f32>() * 2.0;

        let mut mesh = Mesh::from(Cuboid::from_size(Vec3::new(500.0, 500.0, 500.0)));
        let repeat = 500.0 / tile_repeat;
        if let Some(bevy::render::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
        {
            for uv in uvs.iter_mut() {
                uv[0] *= repeat;
                uv[1] *= repeat;
            }
        }
        let mesh_handle = meshes.add(mesh);

        let texture = create_grayscale_texture(gray1, gray2);
        let texture_handle = images.add(texture);

        let material_handle = materials.add(StandardMaterial {
            base_color_texture: Some(texture_handle),
            perceptual_roughness: 0.9,
            ..default()
        });

        commands.spawn((
            Name("DebugSceneGround".into()),
            DebugSceneGroundComponent,
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            Transform::from_translation(center),
            RigidBody::Static,
            Collider::cuboid(500.0, 500.0, 500.0),
            Restitution::ZERO.with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
            CollisionLayers::new(
                [GamePhysicsLayer::Map],
                [
                    // GamePhysicsLayer::Map,
                    GamePhysicsLayer::Car,
                    GamePhysicsLayer::Wheel,
                ],
            ),
        ));
    }

    commands.spawn((
        Camera3d::default(),
        MainCamera,
        Transform::from_xyz(-10.0, 2.0, -15.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        AmbientLight {
            color: Color::srgb(0.8, 0.85, 1.0),
            brightness: 1000.0,
            ..default()
        },
        Msaa::Off,
        //                 bevy::post_process::motion_blur::MotionBlur {
        //     shutter_angle: 1.0,
        //     samples: 2,
        // },
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 3500.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(200.0, 400.0, 200.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
