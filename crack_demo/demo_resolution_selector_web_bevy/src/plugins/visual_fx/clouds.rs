use super::materials::{CloudMaterial, CloudParamsUniform};
use super::settings::VfxSettings;
use bevy::prelude::*;

#[derive(Component)]
pub struct CloudPlane;

pub fn setup_clouds(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cloud_mats: ResMut<Assets<CloudMaterial>>,
    settings: Res<VfxSettings>,
) {
    let quad = Rectangle::new(10000.0, 10000.0);
    let mesh_handle = meshes.add(quad);

    let mat_handle = cloud_mats.add(CloudMaterial {
        params: CloudParamsUniform {
            color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            coverage: settings.cloud_coverage,
            opacity: settings.cloud_opacity,
            wind: Vec2::new(settings.cloud_wind_x, settings.cloud_wind_y),
            scale: settings.cloud_scale,
            _pad1: 0.0,
            _pad2: 0.0,
        },
    });

    // Spawn flat cloud plane high above the world.
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(mat_handle),
        Transform {
            translation: Vec3::new(0.0, 120.0, 0.0),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ..default()
        },
        CloudPlane,
    ));
}

pub fn sync_cloud_uniforms(
    settings: Res<VfxSettings>,
    mut cloud_mats: ResMut<Assets<CloudMaterial>>,
    q_planes: Query<&MeshMaterial3d<CloudMaterial>, With<CloudPlane>>,
) {
    for mat_handle in &q_planes {
        if let Some(mut mat) = cloud_mats.get_mut(mat_handle) {
            mat.params.coverage = settings.cloud_coverage;
            mat.params.opacity = if settings.clouds {
                settings.cloud_opacity
            } else {
                0.0
            };
            mat.params.wind = Vec2::new(settings.cloud_wind_x, settings.cloud_wind_y);
            mat.params.scale = settings.cloud_scale;
        }
    }
}
