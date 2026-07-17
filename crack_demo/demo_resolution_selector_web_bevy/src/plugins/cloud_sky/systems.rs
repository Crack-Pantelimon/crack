use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera;

use super::materials::{
    CloudGroundShadowMaterial, GroundShadowUniform, PrecipOverlayMaterial, SkyDomeMaterial,
    SkyParamsUniform,
};
use super::settings::CloudSkySettings;

/// Marker for the sky dome sphere.
#[derive(Component)]
pub struct CloudSkyDome;
/// Marker for the camera-following rain/snow overlay quad.
#[derive(Component)]
pub struct PrecipOverlayQuad;
/// Marker for the ground cloud-shadow decal.
#[derive(Component)]
pub struct CloudGroundShadowQuad;

/// Builds the shared uniform block from the UI settings.
pub fn make_sky_params(s: &CloudSkySettings) -> SkyParamsUniform {
    let (sun_dir, day_factor) = s.sun_dir_and_day_factor();
    let overcast = s
        .storm_amount
        .max(s.rain_intensity)
        .max(s.snow_intensity)
        .min(1.0);
    let wind = s.wind_vec();
    SkyParamsUniform {
        sun_dir: Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, day_factor),
        amounts: Vec4::new(s.cumulus_amount, s.cirrus_amount, s.storm_amount, overcast),
        detail: Vec4::new(
            s.cumulus_detail,
            s.cirrus_detail,
            s.storm_detail,
            s.cloud_scale,
        ),
        wind: Vec4::new(wind.x, wind.y, s.rain_intensity, s.snow_intensity),
    }
}

fn make_ground_shadow_params(s: &CloudSkySettings) -> GroundShadowUniform {
    let wind = s.wind_vec();
    GroundShadowUniform {
        // uv scale: the 512px tile spans ~333m of ground.
        params: Vec4::new(s.cloud_shadow_intensity, 0.003, 0.0, 0.0),
        wind: Vec4::new(wind.x * 60.0, wind.y * 60.0, 0.0, 0.0),
    }
}

pub fn setup_cloud_sky(
    mut commands: Commands,
    settings: Res<CloudSkySettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut sky_mats: ResMut<Assets<SkyDomeMaterial>>,
    mut precip_mats: ResMut<Assets<PrecipOverlayMaterial>>,
    mut shadow_mats: ResMut<Assets<CloudGroundShadowMaterial>>,
) {
    let params = make_sky_params(&settings);

    // Sky dome: big sphere centered on (and following) the camera, rendered
    // double-sided so we see its inside faces.
    commands.spawn((
        Name::new("CloudSkyDome"),
        CloudSkyDome,
        Mesh3d(meshes.add(Sphere::new(800.0).mesh().uv(64, 32))),
        MeshMaterial3d(sky_mats.add(SkyDomeMaterial { params })),
        Transform::from_xyz(-10.0, 2.0, -15.0),
    ));

    // Precipitation overlay: quad held 2m in front of the camera, no depth
    // testing, so rain/snow draw over the whole view.
    commands.spawn((
        Name::new("PrecipOverlay"),
        PrecipOverlayQuad,
        Mesh3d(meshes.add(Rectangle::new(6.0, 4.0))),
        MeshMaterial3d(precip_mats.add(PrecipOverlayMaterial { params })),
        Transform::from_xyz(-10.0, 2.0, -13.0),
    ));

    // Ground cloud shadows: flat decal over the debug-scene ground.
    let shadow_texture = images.add(generate_cloud_shadow_image());
    commands.spawn((
        Name::new("CloudGroundShadow"),
        CloudGroundShadowQuad,
        Mesh3d(meshes.add(Rectangle::new(1000.0, 1000.0))),
        MeshMaterial3d(shadow_mats.add(CloudGroundShadowMaterial {
            params: make_ground_shadow_params(&settings),
            texture: shadow_texture,
        })),
        Transform::from_xyz(0.0, 0.15, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
}

/// Keep the dome centered on the camera and the overlay pinned in front of it.
pub fn follow_camera(
    camera_q: Query<&GlobalTransform, With<MainCamera>>,
    mut dome_q: Query<&mut Transform, (With<CloudSkyDome>, Without<PrecipOverlayQuad>)>,
    mut overlay_q: Query<&mut Transform, (With<PrecipOverlayQuad>, Without<CloudSkyDome>)>,
) {
    let Ok(cam) = camera_q.single() else {
        return;
    };
    let (_, rot, pos) = cam.to_scale_rotation_translation();
    for mut t in dome_q.iter_mut() {
        t.translation = pos;
    }
    let fwd: Vec3 = cam.forward().into();
    for mut t in overlay_q.iter_mut() {
        t.translation = pos + fwd * 2.0;
        t.rotation = rot;
    }
}

/// Push slider values into the material assets whenever the UI changes them.
pub fn sync_sky_uniforms(
    settings: Res<CloudSkySettings>,
    mut sky_mats: ResMut<Assets<SkyDomeMaterial>>,
    mut precip_mats: ResMut<Assets<PrecipOverlayMaterial>>,
    mut shadow_mats: ResMut<Assets<CloudGroundShadowMaterial>>,
) {
    if !settings.is_changed() {
        return;
    }
    let params = make_sky_params(&settings);
    for (_, mat) in sky_mats.iter_mut() {
        mat.params = params;
    }
    for (_, mat) in precip_mats.iter_mut() {
        mat.params = params;
    }
    let shadow_params = make_ground_shadow_params(&settings);
    for (_, mat) in shadow_mats.iter_mut() {
        mat.params = shadow_params;
    }
}

/// Match the debug scene's sun + ambient light to the time-of-day slider so
/// the world lighting agrees with the sky.
pub fn sync_sun_light(
    settings: Res<CloudSkySettings>,
    mut light_q: Query<(&mut Transform, &mut DirectionalLight)>,
    mut ambient_q: Query<&mut AmbientLight>,
) {
    let (sun_dir, day_factor) = settings.sun_dir_and_day_factor();
    let mut light_dir = sun_dir;
    light_dir.y = light_dir.y.max(0.08);
    for (mut transform, mut light) in light_q.iter_mut() {
        *transform = Transform::from_translation(light_dir.normalize_or_zero() * 100.0)
            .looking_at(Vec3::ZERO, Vec3::Y);
        light.illuminance = 30.0 + 3470.0 * day_factor;
    }
    for mut ambient in ambient_q.iter_mut() {
        ambient.brightness = 150.0 + 850.0 * day_factor;
    }
}

// ---------------------------------------------------------------------------
// CPU cloud-shadow texture generation
// ---------------------------------------------------------------------------

const SHADOW_TEX_SIZE: usize = 512;

fn hash2i(x: i32, y: i32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(0x8da6_b343)
        .wrapping_add((y as u32).wrapping_mul(0xd816_3841))
        .wrapping_add(0xcb1a_b31f);
    h = h.wrapping_mul(0x9e37_79b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Tileable value noise: the lattice is wrapped modulo `period` so the
/// texture repeats seamlessly.
fn vnoise_periodic(x: f32, y: f32, period: i32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let xf = x - xi as f32;
    let yf = y - yi as f32;
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    let w = |i: i32| i.rem_euclid(period);
    let a = hash2i(w(xi), w(yi));
    let b = hash2i(w(xi + 1), w(yi));
    let c = hash2i(w(xi), w(yi + 1));
    let d = hash2i(w(xi + 1), w(yi + 1));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn smoothstep_cpu(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Generates a tileable grayscale FBM image: white where clouds are
/// (the shader turns that into dark shadow alpha).
pub fn generate_cloud_shadow_image() -> Image {
    let mut data = vec![0u8; SHADOW_TEX_SIZE * SHADOW_TEX_SIZE * 4];
    for y in 0..SHADOW_TEX_SIZE {
        for x in 0..SHADOW_TEX_SIZE {
            let u = x as f32 / SHADOW_TEX_SIZE as f32;
            let v = y as f32 / SHADOW_TEX_SIZE as f32;
            // 6 octaves, base period 4 cells across the tile.
            let mut sum = 0.0f32;
            let mut amp = 0.5f32;
            let mut period = 4i32;
            for _ in 0..6 {
                sum += amp
                    * vnoise_periodic(u * period as f32, v * period as f32, period);
                amp *= 0.5;
                period *= 2;
            }
            // sum in ~[0, 0.97]; remap through a coverage curve so shadows
            // form distinct cloud-shaped blobs.
            let n = smoothstep_cpu(0.35, 0.75, sum);
            let g = (n * 255.0) as u8;
            let off = (y * SHADOW_TEX_SIZE + x) * 4;
            data[off] = g;
            data[off + 1] = g;
            data[off + 2] = g;
            data[off + 3] = 255;
        }
    }
    let mut image = Image::new_fill(
        Extent3d {
            width: SHADOW_TEX_SIZE as u32,
            height: SHADOW_TEX_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        ..default()
    });
    image
}
