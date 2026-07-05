//! Hitscan gun shooting: ammo tracking, ray-cast shots into cars/pedestrians/map, and tracer
//! gizmos (shot line + muzzle/impact points + a short ricochet path) that live for a few seconds.
//!
//! The shot ray is fired from the camera through the screen-center crosshair, so what is under the
//! crosshair is what gets hit. The tracer is drawn from the weapon muzzle to the impact point.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use super::weapon_attach::{EquippedWeapon, WeaponModel, WeaponModelState};
use super::weapon_manifest::WeaponId;

/// How long a shot tracer stays visible.
const TRACER_TTL: f32 = 5.0;
/// Length of the drawn ricochet (reflected bullet path) segment.
const REFLECT_LEN: f32 = 0.5;

/// Ammo state for a character holding a gun. Inserted on gun equip, removed otherwise.
#[derive(Component)]
pub struct GunState {
    pub rounds: u32,
    pub clip_size: u32,
}

/// Fire the shooter's gun once (ammo permitting).
#[derive(Event)]
pub struct FireGunEvent {
    pub shooter: Entity,
}

/// Refill the shooter's clip.
#[derive(Event)]
pub struct ReloadGunEvent {
    pub shooter: Entity,
}

pub struct ShotTracer {
    pub from: Vec3,
    pub to: Vec3,
    /// End point of the short ricochet segment, when the shot hit something.
    pub reflect_to: Option<Vec3>,
    pub ttl: f32,
}

/// Live shot tracers, drawn as gizmos each frame until their TTL runs out.
#[derive(Resource, Default)]
pub struct ShotTracers(pub Vec<ShotTracer>);

pub fn fire_gun_observer(
    trigger: On<FireGunEvent>,
    mut shooters: Query<(&mut GunState, &EquippedWeapon, Option<&WeaponModelState>)>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    transforms: Query<&GlobalTransform>,
    weapon_models: Query<&GlobalTransform, With<WeaponModel>>,
    spatial: SpatialQuery,
    mut tracers: ResMut<ShotTracers>,
) {
    let shooter = trigger.event().shooter;
    let Ok((mut gun, equipped, model_state)) = shooters.get_mut(shooter) else {
        return;
    };
    let WeaponId::Gun(info) = &equipped.0 else {
        return;
    };
    if gun.rounds == 0 {
        return;
    }
    gun.rounds -= 1;

    let Some(cam) = camera.iter().next() else {
        return;
    };
    // The shot goes from the camera through the screen-center crosshair.
    let origin = cam.translation();
    let dir = cam.forward();

    // Tracer starts at the gun muzzle (weapon model position), falling back to chest height.
    let muzzle = model_state
        .and_then(|s| s.entity)
        .and_then(|e| weapon_models.get(e).ok())
        .map(|gt| gt.translation())
        .or_else(|| {
            transforms
                .get(shooter)
                .ok()
                .map(|gt| gt.translation() + Vec3::Y * 0.4)
        })
        .unwrap_or(origin);

    let filter = SpatialQueryFilter::from_excluded_entities([shooter]);
    if let Some(hit) = spatial.cast_ray(origin, dir, info.range, true, &filter) {
        let impact = origin + *dir * hit.distance;
        let normal: Vec3 = hit.normal;
        let reflect = (*dir - 2.0 * dir.dot(normal) * normal).normalize_or_zero();
        tracers.0.push(ShotTracer {
            from: muzzle,
            to: impact,
            reflect_to: Some(impact + reflect * REFLECT_LEN),
            ttl: TRACER_TTL,
        });
        info!(
            "Gun hit {:?} at {:.1?} ({} dmg, {} left in clip)",
            hit.entity, impact, info.damage, gun.rounds
        );
    } else {
        // Missed everything: tracer flies out to max range.
        tracers.0.push(ShotTracer {
            from: muzzle,
            to: origin + *dir * info.range,
            reflect_to: None,
            ttl: TRACER_TTL,
        });
    }
}

pub fn reload_gun_observer(
    trigger: On<ReloadGunEvent>,
    mut shooters: Query<&mut GunState>,
) {
    if let Ok(mut gun) = shooters.get_mut(trigger.event().shooter) {
        gun.rounds = gun.clip_size;
    }
}

/// Draws the live tracers and expires them after [`TRACER_TTL`].
pub fn draw_shot_tracers(time: Res<Time>, mut gizmos: Gizmos, mut tracers: ResMut<ShotTracers>) {
    let dt = time.delta_secs();
    tracers.0.retain_mut(|t| {
        t.ttl -= dt;
        t.ttl > 0.0
    });
    for t in &tracers.0 {
        // Bullet track.
        gizmos.line(t.from, t.to, Color::srgb(1.0, 0.9, 0.3));
        // Shooting point and impact point as small circles.
        gizmos.sphere(t.from, 0.03, Color::WHITE);
        gizmos.sphere(t.to, 0.05, Color::srgb(1.0, 0.3, 0.2));
        // Short ricochet path.
        if let Some(reflect_to) = t.reflect_to {
            gizmos.line(t.to, reflect_to, Color::srgb(1.0, 0.5, 0.1));
        }
    }
}
