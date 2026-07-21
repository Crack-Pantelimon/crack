use super::materials::{AdditiveFxMaterial, BillboardParams, BlendFxMaterial, FxKind};
use super::settings::VfxSettings;
use super::spawn::{VfxDrift, VfxMeshes, spawn_additive_billboard_fx, spawn_blend_billboard_fx};
use bevy::prelude::*;

/// car explosion event.
#[derive(Event, Debug, Clone)]
pub struct CarExplosionEvent {
/// position field.
    pub position: Vec3,
}

/// explosion flash.
pub struct ExplosionFlash {
/// position field.
    pub position: Vec3,
/// elapsed field.
    pub elapsed: f32,
/// lifetime field.
    pub lifetime: f32,
}

/// explosion flashes.
#[derive(Resource, Default)]
pub struct ExplosionFlashes {
/// flashes field.
    pub flashes: Vec<ExplosionFlash>,
}

/// draw explosion flashes.
pub fn draw_explosion_flashes(
    mut gizmos: Gizmos,
    time: Res<Time>,
    mut flashes: ResMut<ExplosionFlashes>,
    settings: Res<VfxSettings>,
) {
    let dt = time.delta_secs();
    flashes.flashes.retain_mut(|f| {
        f.elapsed += dt;
        if f.elapsed >= f.lifetime {
            return false;
        }

        if settings.car_explosion_gizmos {
            // Draw spheres matching typical kill, damage, warning radii
            gizmos.sphere(f.position, 2.0, Color::srgb(1.0, 0.1, 0.0));
            gizmos.sphere(f.position, 4.0, Color::srgb(1.0, 0.5, 0.0));
            gizmos.sphere(f.position, 6.0, Color::srgb(1.0, 0.9, 0.1));
        }

        true
    });
}

/// car explosion observer.
pub fn car_explosion_observer(
    trigger: On<CarExplosionEvent>,
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<VfxSettings>,
    meshes: Option<Res<VfxMeshes>>,
    mut additive_mats: ResMut<Assets<AdditiveFxMaterial>>,
    mut blend_mats: ResMut<Assets<BlendFxMaterial>>,
    mut flashes: ResMut<ExplosionFlashes>,
) {
    let event = trigger.event();
    let pos = event.position;
    let now = time.elapsed_secs();

    // 1. Add explosion flash wireframe spheres
    flashes.flashes.push(ExplosionFlash {
        position: pos,
        elapsed: 0.0,
        lifetime: 0.4,
    });

    let Some(meshes) = meshes else {
        return;
    };

    // 2. Fireball shader (additive)
    if settings.car_fireball {
        let params = BillboardParams {
            color: Vec4::new(1.0, 0.6, 0.1, 1.0),
            spawn_time: now,
            lifetime: settings.fireball_lifetime,
            start_radius: 1.0,
            end_radius: settings.fireball_radius,
            seed: rand::random::<f32>(),
            kind: FxKind::Fireball as u32,
            _pad: 0.0,
        };
        spawn_additive_billboard_fx(
            &mut commands,
            &mut additive_mats,
            &meshes,
            &time,
            pos + Vec3::new(0.0, 1.0, 0.0),
            params,
        );
    }

    // 3. Smoke puffs (blend)
    if settings.car_smoke {
        // Spawn 3-4 smoke puffs with slightly different velocities/seeds
        for i in 0..4 {
            let offset = Vec3::new(
                (rand::random::<f32>() - 0.5) * 1.5,
                (rand::random::<f32>() - 0.5) * 0.5,
                (rand::random::<f32>() - 0.5) * 1.5,
            );
            let drift_vel = Vec3::new(
                (rand::random::<f32>() - 0.5) * 0.8,
                rand::random::<f32>() * 1.2 + 0.5, // rising upward
                (rand::random::<f32>() - 0.5) * 0.8,
            );
            let params = BillboardParams {
                color: Vec4::new(0.6, 0.6, 0.6, settings.smoke_opacity),
                spawn_time: now,
                lifetime: settings.smoke_lifetime,
                start_radius: 1.5,
                end_radius: 5.0,
                seed: rand::random::<f32>() + (i as f32 * 0.23),
                kind: FxKind::SmokePuff as u32,
                _pad: 0.0,
            };
            let smoke_entity = spawn_blend_billboard_fx(
                &mut commands,
                &mut blend_mats,
                &meshes,
                &time,
                pos + offset,
                params,
            );
            commands.entity(smoke_entity).insert(VfxDrift {
                velocity: drift_vel,
            });
        }
    }
}
