use avian3d::prelude::{SpatialQuery, SpatialQueryFilter, Collider};
use bevy::prelude::*;

use super::weapon_attach::{EquippedWeapon, WeaponModel, WeaponModelState};
use super::weapon_manifest::WeaponId;
use crate::plugins::pedestrians::ModelRoot;
use crate::plugins::pedestrians::pedestrian_controller_plugin::{CharacterController, DriverMesh};
use crate::plugins::pedestrians::skeleton::PedestrianSkeleton;

/// How long a shot tracer stays visible.
const TRACER_TTL: f32 = 0.05;
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

pub struct MeleeDebugBox {
    pub position: Vec3,
    pub rotation: Quat,
    pub ttl: f32,
}

#[derive(Resource, Default)]
pub struct MeleeDebugBoxes(pub Vec<MeleeDebugBox>);

pub fn draw_melee_debug_boxes(
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut boxes: ResMut<MeleeDebugBoxes>,
) {
    let dt = time.delta_secs();
    boxes.0.retain_mut(|b| {
        b.ttl -= dt;
        b.ttl > 0.0
    });
    for b in &boxes.0 {
        gizmos.primitive_3d(
            &Cuboid::new(1.0, 1.0, 2.0),
            Isometry3d::new(b.position, b.rotation),
            Color::srgb(1.0, 1.0, 0.0),
        );
    }
}

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
    healths: Query<&crate::plugins::pedestrian_ai::faction::Health>,
    mut car_healths: Query<&mut crate::plugins::cars_driving::driving_plugin::spawn_car::CarHealth>,
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
        fx: crate::plugins::audio::audio_fx::AudioFxEventType::GunShot {
            sound_idx: gun.gunshot_sound_idx,
        },
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

        if is_person {
            let mut cur = hit.entity;
            loop {
                if q_controller.contains(cur) {
                    break;
                }
                match parents.get(cur) {
                    Ok(child_of) => cur = child_of.parent(),
                    Err(_) => break,
                }
            }
            if healths.get(cur).is_ok() {
                commands.trigger(crate::plugins::pedestrian_ai::combat::DamageEvent {
                    target: cur,
                    amount: info.damage,
                    source: shooter,
                });
            }
        } else {
            // Check if bullet hit a car
            let mut target_car = None;
            let mut cur = hit.entity;
            loop {
                if car_healths.get(cur).is_ok() {
                    target_car = Some(cur);
                    break;
                }
                match parents.get(cur) {
                    Ok(child_of) => cur = child_of.parent(),
                    Err(_) => break,
                }
            }
            if let Some(car_ent) = target_car {
                if let Ok(mut car_health) = car_healths.get_mut(car_ent) {
                    car_health.current = (car_health.current - info.damage).max(0.0);
                }
            }
        }
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
    healths: Query<&crate::plugins::pedestrian_ai::faction::Health>,
    mut car_healths: Query<&mut crate::plugins::cars_driving::driving_plugin::spawn_car::CarHealth>,
    mut sparks: ResMut<BulletSparks>,
    q_global_transform: Query<&GlobalTransform>,
    mut debug_boxes: ResMut<MeleeDebugBoxes>,
) {
    let dt = time.delta_secs();
    for (entity, gt, mut pending) in &mut query {
        pending.timer -= dt;
        if pending.timer <= 0.0 {
            let origin = gt.translation() + Vec3::Y * 0.5;
            let forward = Dir3::new(gt.rotation() * Vec3::Z).unwrap_or(Dir3::Z);
            let filter = SpatialQueryFilter::from_excluded_entities([entity]);

            let box_shape = Collider::cuboid(1.0, 1.0, 2.0);
            let box_pos = origin + *forward * 1.0;
            let rotation = gt.rotation();

            // Add the yellow wireframe debug box
            debug_boxes.0.push(MeleeDebugBox {
                position: box_pos,
                rotation,
                ttl: 0.1,
            });

            let intersecting = spatial.shape_intersections(
                &box_shape,
                box_pos,
                rotation,
                &filter,
            );

            let mut hit_roots = std::collections::HashSet::new();

            for hit_entity in intersecting {
                // Resolve the hit entity up to CharacterController or CarHealth
                let mut root_target = hit_entity;
                let mut found_root = false;
                let mut is_car = false;
                loop {
                    if q_controller.contains(root_target) {
                        found_root = true;
                        break;
                    }
                    if car_healths.get(root_target).is_ok() {
                        found_root = true;
                        is_car = true;
                        break;
                    }
                    match parents.get(root_target) {
                        Ok(child_of) => root_target = child_of.parent(),
                        Err(_) => break,
                    }
                }

                if !found_root {
                    continue;
                }

                // Skip if we already hit this root target in this swing
                if !hit_roots.insert(root_target) {
                    continue;
                }

                let hit_pos = q_global_transform.get(hit_entity).map(|g| g.translation()).unwrap_or(box_pos);
                let is_person = is_person_entity(
                    hit_entity,
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
                    let spawn_pos = hit_pos + offset;

                    let rx = rand::random::<f32>() * 2.0 - 1.0;
                    let ry = rand::random::<f32>() * 1.5 + 0.3;
                    let rz = rand::random::<f32>() * 2.0 - 1.0;
                    let jump_dir = Vec3::new(rx, ry, rz).normalize_or_zero();

                    let speed = if is_person {
                        // Red and slower for persons
                        rand::random::<f32>() * 1.5 + 0.8
                    } else {
                        // Metallic clashing sparks
                        rand::random::<f32>() * 4.0 + 3.0
                    };

                    sparks.0.push(BulletSpark {
                        position: spawn_pos,
                        velocity: jump_dir * speed,
                        is_person,
                        lifetime: 1.0,
                    });
                }

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

                    if healths.get(root_target).is_ok() {
                        let amount = if pending.is_melee {
                            crate::plugins::pedestrian_ai::combat::SWORD_DAMAGE
                        } else {
                            crate::plugins::pedestrian_ai::combat::PUNCH_DAMAGE
                        };
                        commands.trigger(crate::plugins::pedestrian_ai::combat::DamageEvent {
                            target: root_target,
                            amount,
                            source: entity,
                        });
                    }
                } else if is_car {
                    let amount = if pending.is_melee {
                        crate::plugins::pedestrian_ai::combat::SWORD_DAMAGE
                    } else {
                        crate::plugins::pedestrian_ai::combat::PUNCH_DAMAGE
                    };
                    if let Ok(mut car_health) = car_healths.get_mut(root_target) {
                        car_health.current = (car_health.current - amount).max(0.0);
                    }

                    if pending.is_melee {
                        commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                            fx: crate::plugins::audio::audio_fx::AudioFxEventType::MeleeClash,
                            position: hit_pos,
                            follow: None,
                        });
                    }
                }
            }
            commands.entity(entity).remove::<PendingMeleeHit>();
        }
    }
}
