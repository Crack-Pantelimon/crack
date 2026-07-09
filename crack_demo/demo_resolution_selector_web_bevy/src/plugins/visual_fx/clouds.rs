use super::materials::{CloudMaterial, CloudParamsUniform};
use super::settings::VfxSettings;
use bevy::prelude::*;

/// Side length of the unit cloud quad spawned at startup (before any map is loaded).
const CLOUD_PLANE_SIZE: f32 = 10000.0;
/// Fixed height of the cloud layer above the map bbox top once the manifest is parsed.
const CLOUD_HEIGHT_ABOVE_MAP: f32 = 150.0;
/// Fallback world-space cloud height used until the map manifest arrives.
const CLOUD_FALLBACK_HEIGHT: f32 = 120.0;
/// The plane is scaled to cover the map extent times this margin, so clouds still reach the
/// horizon when looking outward from the map edge.
const CLOUD_EXTENT_MARGIN: f32 = 2.0;

#[derive(Component)]
pub struct CloudPlane;

pub fn setup_clouds(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cloud_mats: ResMut<Assets<CloudMaterial>>,
    settings: Res<VfxSettings>,
) {
    let quad = Rectangle::new(CLOUD_PLANE_SIZE, CLOUD_PLANE_SIZE);
    let mesh_handle = meshes.add(quad);

    let mat_handle = cloud_mats.add(CloudMaterial {
        params: CloudParamsUniform {
            color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            coverage: settings.cloud_coverage,
            opacity: settings.cloud_opacity,
            wind: Vec2::new(settings.cloud_wind_x, settings.cloud_wind_y),
            scale: settings.cloud_scale,
            debug_solid: if settings.debug_solid { 1.0 } else { 0.0 },
            _pad1: 0.0,
            _pad2: 0.0,
        },
    });

    // Spawn flat cloud plane high above the world. `position_clouds_over_map` re-anchors it over
    // the real map bbox once the manifest is parsed.
    commands.spawn((
        Mesh3d(mesh_handle),
        MeshMaterial3d(mat_handle),
        Transform {
            translation: Vec3::new(0.0, CLOUD_FALLBACK_HEIGHT, 0.0),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ..default()
        },
        CloudPlane,
    ));
}

/// Anchors the cloud plane over the loaded map: centered on the map bbox, a fixed distance above
/// its *top* (the startup position at world y=120 sat below the terrain on maps with real
/// elevations, so the clouds were never visible in the main game), and scaled to cover the map
/// extent with margin.
pub fn position_clouds_over_map(
    map_tree: Option<Res<crate::plugins::map_plugin::MapTree>>,
    mut q_planes: Query<&mut Transform, With<CloudPlane>>,
) {
    let Some(map_tree) = map_tree else {
        return; // demo/sim binaries without the map plugin keep the startup placement
    };
    if !map_tree.is_changed() || !map_tree.parsed {
        return;
    }

    let bbox = &map_tree.bbox;
    let center = (bbox.min + bbox.max) / 2.0;
    let top_y = bbox.min.y.max(bbox.max.y);
    let extent = (bbox.max.x - bbox.min.x)
        .abs()
        .max((bbox.max.z - bbox.min.z).abs());
    let scale = (extent * CLOUD_EXTENT_MARGIN / CLOUD_PLANE_SIZE).max(1.0);

    for mut transform in &mut q_planes {
        transform.translation = Vec3::new(center.x, top_y + CLOUD_HEIGHT_ABOVE_MAP, center.z);
        transform.scale = Vec3::splat(scale);
        info!(
            "Cloud plane anchored over map: y={:.1} (map top {:.1}), scale={:.2}",
            transform.translation.y, top_y, scale
        );
    }
}

pub fn sync_cloud_uniforms(
    settings: Res<VfxSettings>,
    mut cloud_mats: ResMut<Assets<CloudMaterial>>,
    q_planes: Query<&MeshMaterial3d<CloudMaterial>, With<CloudPlane>>,
) {
    if !settings.is_changed() {
        return;
    }
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
            mat.params.debug_solid = if settings.debug_solid { 1.0 } else { 0.0 };
        }
    }
}
