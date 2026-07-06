use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use super::weapon_attach::{EquippedWeapon, WeaponModel, WeaponModelState};
use super::weapon_manifest::WeaponId;
use crate::plugins::pedestrians::ModelRoot;
use crate::plugins::pedestrians::pedestrian_controller_plugin::{CharacterController, DriverMesh};
use crate::plugins::pedestrians::skeleton::PedestrianSkeleton;

/// How long a shot tracer stays visible.
const TRACER_TTL: f32 = 5.0;
/// Length of the drawn ricochet (reflected bullet path) segment.
const REFLECT_LEN: f32 = 0.5;

/// Ammo state for a character holding a gun. Inserted on gun equip, removed otherwise.
#[derive(Component)]
pub struct GunState {
    pub rounds: u32,
    pub clip_size: u32,
    pub gunshot_sound_idx: usize,
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

pub struct BulletSpark {
    pub position: Vec3,
    pub velocity: Vec3,
    pub is_person: bool,
    pub lifetime: f32,
}

/// Live bullet impact sparks.
#[derive(Resource, Default)]
pub struct BulletSparks(pub Vec<BulletSpark>);

pub(crate) fn is_person_entity(
    hit_entity: Entity,
    parents: &Query<&ChildOf>,
    q_controller: &Query<(), With<CharacterController>>,
    q_model: &Query<(), With<ModelRoot>>,
    q_skel: &Query<(), With<PedestrianSkeleton>>,
    q_driver: &Query<(), With<DriverMesh>>,
) -> bool {
    let mut cur = hit_entity;
    loop {
        if q_controller.contains(cur)
            || q_model.contains(cur)
            || q_skel.contains(cur)
            || q_driver.contains(cur)
        {
            return true;
        }
        match parents.get(cur) {
            Ok(child_of) => cur = child_of.parent(),
            Err(_) => break,
        }
    }
    false
}

pub fn fire_gun_observer(
    trigger: On<FireGunEvent>,
    mut shooters: Query<(&mut GunState, &EquippedWeapon, Option<&WeaponModelState>)>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    transforms: Query<&GlobalTransform>,
    weapon_models: Query<&GlobalTransform, With<WeaponModel>>,
    spatial: SpatialQuery,
    parents: Query<&ChildOf>,
    q_controller: Query<(), With<CharacterController>>,
    q_model: Query<(), With<ModelRoot>>,
    q_skel: Query<(), With<PedestrianSkeleton>>,
    q_driver: Query<(), With<DriverMesh>>,
    mut tracers: ResMut<ShotTracers>,
    mut sparks: ResMut<BulletSparks>,
    mut commands: Commands,
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

    commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
        fx: crate::plugins::audio::audio_fx::AudioFxEventType::GunShot { sound_idx: gun.gunshot_sound_idx },
        position: muzzle,
        follow: None,
    });

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

        commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
            fx: crate::plugins::audio::audio_fx::AudioFxEventType::BulletImpact,
            position: impact,
            follow: None,
        });

        let is_person = is_person_entity(
            hit.entity,
            &parents,
            &q_controller,
            &q_model,
            &q_skel,
            &q_driver,
        );

        // Spawn 3 sparks jumping at random speeds around contact point +/- 0.1m
        for _ in 0..3 {
            let offset = Vec3::new(
                (rand::random::<f32>() * 2.0 - 1.0) * 0.1,
                (rand::random::<f32>() * 2.0 - 1.0) * 0.1,
                (rand::random::<f32>() * 2.0 - 1.0) * 0.1,
            );
            let spawn_pos = impact + offset;

            let rx = rand::random::<f32>() * 2.0 - 1.0;
            let ry = rand::random::<f32>() * 1.5 + 0.3;
            let rz = rand::random::<f32>() * 2.0 - 1.0;
            let jump_dir = Vec3::new(rx, ry, rz).normalize_or_zero();

            let speed = if is_person {
                // Red and slower for persons
                rand::random::<f32>() * 1.5 + 0.8
            } else {
                // Ground and car sparks
                rand::random::<f32>() * 4.0 + 3.0
            };

            sparks.0.push(BulletSpark {
                position: spawn_pos,
                velocity: jump_dir * speed,
                is_person,
                lifetime: 1.0,
            });
        }

        info!(
            "Gun hit {:?} (is_person={}) at {:.1?} ({} dmg, {} left in clip)",
            hit.entity, is_person, impact, info.damage, gun.rounds
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
    mut shooters: Query<(&mut GunState, &GlobalTransform)>,
    mut commands: Commands,
) {
    if let Ok((mut gun, gt)) = shooters.get_mut(trigger.event().shooter) {
        if gun.rounds < gun.clip_size {
            gun.rounds = gun.clip_size;
            commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                fx: crate::plugins::audio::audio_fx::AudioFxEventType::GunReload,
                position: gt.translation(),
                follow: None,
            });
        }
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

/// Updates position and draws bullet impact sparks (0.04m diameter, air friction, red/slower for persons).
pub fn draw_bullet_sparks(time: Res<Time>, mut gizmos: Gizmos, mut sparks: ResMut<BulletSparks>) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let gravity = Vec3::new(0.0, -9.81, 0.0);
    let air_friction = 3.0;

    sparks.0.retain_mut(|s| {
        s.lifetime -= dt;
        if s.lifetime <= 0.0 {
            return false;
        }

        s.velocity += gravity * dt;
        s.velocity *= (1.0 - air_friction * dt).max(0.0);
        s.position += s.velocity * dt;

        let alpha = (s.lifetime / 1.0).clamp(0.0, 1.0);
        let color = if s.is_person {
            Color::srgba(0.95, 0.15, 0.15, alpha)
        } else {
            Color::srgba(1.0, 0.9, 0.2, alpha)
        };

        // 0.04m diameter => 0.02m radius
        gizmos.sphere(s.position, 0.02, color);

        true
    });
}

#[derive(Component)]
pub struct PendingMeleeHit {
    pub timer: f32,
    pub is_melee: bool,
}

pub fn tick_pending_melee_hits(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &GlobalTransform, &mut PendingMeleeHit)>,
    spatial: SpatialQuery,
    parents: Query<&ChildOf>,
    q_controller: Query<(), With<CharacterController>>,
    q_model: Query<(), With<ModelRoot>>,
    q_skel: Query<(), With<PedestrianSkeleton>>,
    q_driver: Query<(), With<DriverMesh>>,
) {
    let dt = time.delta_secs();
    for (entity, gt, mut pending) in &mut query {
        pending.timer -= dt;
        if pending.timer <= 0.0 {
            let origin = gt.translation() + Vec3::Y * 0.5;
            let forward = gt.forward();
            let filter = SpatialQueryFilter::from_excluded_entities([entity]);
            if let Some(hit) = spatial.cast_ray(origin, forward, 1.5, true, &filter) {
                let hit_pos = origin + *forward * hit.distance;
                let is_person = is_person_entity(
                    hit.entity,
                    &parents,
                    &q_controller,
                    &q_model,
                    &q_skel,
                    &q_driver,
                );
                if is_person {
                    if pending.is_melee {
                        commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                            fx: crate::plugins::audio::audio_fx::AudioFxEventType::MeleeHitMeat,
                            position: hit_pos,
                            follow: None,
                        });
                    } else {
                        commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                            fx: crate::plugins::audio::audio_fx::AudioFxEventType::PunchHit,
                            position: hit_pos,
                            follow: None,
                        });
                    }
                } else if pending.is_melee {
                    commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                        fx: crate::plugins::audio::audio_fx::AudioFxEventType::MeleeClash,
                        position: hit_pos,
                        follow: None,
                    });
                }
            }
            commands.entity(entity).remove::<PendingMeleeHit>();
        }
    }
}
