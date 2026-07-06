//! AI combat: directed gun fire, melee hits, punch, damage events, death.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::plugins::audio::audio_fx::{AudioFxEvent, AudioFxEventType};
use crate::plugins::pedestrians::{
    ModelRoot, PedestrianAnimationControlEvent,
    pedestrian_controller_plugin::{CharacterController, DriverMesh},
    skeleton::PedestrianSkeleton,
};
use crate::plugins::weapons::{
    BulletSpark, BulletSparks, EquippedWeapon, GunState, ShotTracers,
    weapon_attach::WeaponModelState,
    weapon_shooting::{ShotTracer, is_person_entity},
};

use super::{
    AiCombatTimers, AiModel, AiPedestrian, AiPerception, AiState,
    faction::Health,
};

// -------------------------------------------------------------------------------------
// Constants
// -------------------------------------------------------------------------------------

const SHOT_INTERVAL: f32 = 0.25;
const AIM_SPREAD_DEG: f32 = 3.0;
const MELEE_RANGE: f32 = 2.0;
const SWORD_DAMAGE: f32 = 35.0;
const SWING_INTERVAL: f32 = 0.8;
const PUNCH_RANGE: f32 = 1.5;
const PUNCH_DAMAGE: f32 = 12.0;
const PUNCH_INTERVAL: f32 = 0.6;
/// How long a shot tracer stays visible.
const TRACER_TTL: f32 = 5.0;
/// Length of the drawn ricochet (reflected bullet path) segment.
const REFLECT_LEN: f32 = 0.5;

// -------------------------------------------------------------------------------------
// Damage event
// -------------------------------------------------------------------------------------

/// Inflict `amount` damage on `target`.
#[derive(Event)]
pub struct DamageEvent {
    pub target: Entity,
    pub amount: f32,
    #[allow(dead_code)]
    pub source: Entity,
}

/// Observer: apply damage and handle death (immediate despawn).
pub fn apply_damage_observer(
    trigger: On<DamageEvent>,
    mut commands: Commands,
    mut healths: Query<(&mut Health, Option<&super::faction::Faction>)>,
) {
    let ev = trigger.event();
    let Ok((mut health, faction)) = healths.get_mut(ev.target) else {
        return;
    };
    health.current -= ev.amount;
    if health.current <= 0.0 {
        let faction_label = faction.map(|f| f.label()).unwrap_or("?");
        info!("[AI {:?}] DIED (faction {})", ev.target, faction_label);
        commands.entity(ev.target).despawn();
    }
}

// -------------------------------------------------------------------------------------
// AI combat system
// -------------------------------------------------------------------------------------

/// Handles attacks for AI peds: directed gun fire, melee swings, and punches.
#[allow(clippy::too_many_arguments)]
pub fn ai_combat(
    mut commands: Commands,
    spatial_query: SpatialQuery,
    mut tracers: ResMut<ShotTracers>,
    mut sparks: ResMut<BulletSparks>,
    mut query: Query<
        (
            Entity,
            &GlobalTransform,
            &AiState,
            &AiPerception,
            &mut AiCombatTimers,
            Option<&EquippedWeapon>,
            Option<&mut GunState>,
            Option<&WeaponModelState>,
            Option<&AiModel>,
        ),
        With<AiPedestrian>,
    >,
    global_transforms: Query<&GlobalTransform>,
    parents: Query<&ChildOf>,
    q_controller: Query<(), With<CharacterController>>,
    q_model: Query<(), With<ModelRoot>>,
    q_skel: Query<(), With<PedestrianSkeleton>>,
    q_driver: Query<(), With<DriverMesh>>,
    healths: Query<&Health>,
) {
    for (
        entity,
        gt,
        state,
        perception,
        mut timers,
        equipped,
        gun_state,
        weapon_model,
        ai_model,
    ) in &mut query
    {
        if *state != AiState::Hunt {
            continue;
        }
        if !perception.visible || perception.target.is_none() {
            continue;
        }
        if timers.attack_cooldown > 0.0 {
            continue;
        }

        // Verify shooter is still alive.
        if let Ok(health) = healths.get(entity) {
            if health.current <= 0.0 {
                continue;
            }
        } else {
            continue;
        }

        let target_entity = perception.target.unwrap();

        // Verify target is still alive and has health > 0.
        if let Ok(target_health) = healths.get(target_entity) {
            if target_health.current <= 0.0 {
                continue;
            }
        } else {
            continue;
        }

        let my_pos = gt.translation();
        let is_gun = equipped.is_some_and(|e| e.0.is_gun());
        let is_melee = equipped.is_some_and(|e| e.0.is_melee());

        if is_gun {
            // --- Directed gun fire ---
            let Some(mut gun) = gun_state else {
                continue;
            };
            if gun.rounds == 0 {
                continue;
            }

            let gun_info = match equipped.map(|e| &e.0) {
                Some(crate::plugins::weapons::WeaponId::Gun(info)) => info,
                _ => continue,
            };

            // Muzzle origin: weapon model position, fallback head.
            let muzzle = weapon_model
                .and_then(|wms| wms.entity)
                .and_then(|e| global_transforms.get(e).ok())
                .map(|gt| gt.translation())
                .unwrap_or_else(|| my_pos + Vec3::Y * 0.85);

            // Direction toward target head with spread jitter.
            let target_head = perception.target_pos;
            let base_dir = (target_head - muzzle).normalize_or_zero();
            let spread_rad = AIM_SPREAD_DEG.to_radians();
            let jitter_yaw = ((_crack_utils::random_u32() % 1000) as f32 / 500.0 - 1.0) * spread_rad;
            let jitter_pitch = ((_crack_utils::random_u32() % 1000) as f32 / 500.0 - 1.0) * spread_rad;
            let dir = Quat::from_euler(bevy::math::EulerRot::YXZ, jitter_yaw, jitter_pitch, 0.0)
                * base_dir;

            gun.rounds -= 1;
            timers.attack_cooldown = SHOT_INTERVAL;

            // Audio
            commands.trigger(AudioFxEvent {
                fx: AudioFxEventType::GunShot {
                    sound_idx: gun.gunshot_sound_idx,
                },
                position: muzzle,
                follow: None,
            });

            // Raycast
            let filter = SpatialQueryFilter::from_excluded_entities([entity]);
            let Ok(ray_dir) = Dir3::new(dir) else {
                continue;
            };

            if let Some(hit) = spatial_query.cast_ray(muzzle, ray_dir, gun_info.range, true, &filter)
            {
                let impact = muzzle + dir * hit.distance;
                let normal: Vec3 = hit.normal;
                let reflect = (dir - 2.0 * dir.dot(normal) * normal).normalize_or_zero();

                tracers.0.push(ShotTracer {
                    from: muzzle,
                    to: impact,
                    reflect_to: Some(impact + reflect * REFLECT_LEN),
                    ttl: TRACER_TTL,
                });

                commands.trigger(AudioFxEvent {
                    fx: AudioFxEventType::BulletImpact,
                    position: impact,
                    follow: None,
                });

                let hit_is_person = is_person_entity(
                    hit.entity, &parents, &q_controller, &q_model, &q_skel, &q_driver,
                );

                // Sparks
                for _ in 0..3 {
                    let offset = Vec3::new(
                        (rand::random::<f32>() * 2.0 - 1.0) * 0.1,
                        (rand::random::<f32>() * 2.0 - 1.0) * 0.1,
                        (rand::random::<f32>() * 2.0 - 1.0) * 0.1,
                    );
                    let jump_dir = Vec3::new(
                        rand::random::<f32>() * 2.0 - 1.0,
                        rand::random::<f32>() * 1.5 + 0.3,
                        rand::random::<f32>() * 2.0 - 1.0,
                    )
                    .normalize_or_zero();
                    let speed = if hit_is_person {
                        rand::random::<f32>() * 1.5 + 0.8
                    } else {
                        rand::random::<f32>() * 4.0 + 3.0
                    };
                    sparks.0.push(BulletSpark {
                        position: impact + offset,
                        velocity: jump_dir * speed,
                        is_person: hit_is_person,
                        lifetime: 1.0,
                    });
                }

                // Apply damage if hit a person with Health.
                if hit_is_person {
                    // Resolve to the controller entity.
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
                        commands.trigger(DamageEvent {
                            target: cur,
                            amount: gun_info.damage,
                            source: entity,
                        });
                    }
                }

                info!("[AI {:?}] SHOOT -> {:?} ({} dmg)", entity, target_entity, gun_info.damage);
            } else {
                // Miss: tracer to max range.
                tracers.0.push(ShotTracer {
                    from: muzzle,
                    to: muzzle + dir * gun_info.range,
                    reflect_to: None,
                    ttl: TRACER_TTL,
                });
            }

            // Shoot animation
            if let Some(ai_model) = ai_model {
                commands.trigger(PedestrianAnimationControlEvent {
                    ped: ai_model.0,
                    animation: "Pistol_Shoot".to_string(),
                    speed: 1.0,
                });
            }
        } else if is_melee {
            // --- Melee (sword) ---
            if perception.target_dist > MELEE_RANGE {
                continue;
            }
            timers.attack_cooldown = SWING_INTERVAL;

            commands.trigger(DamageEvent {
                target: target_entity,
                amount: SWORD_DAMAGE,
                source: entity,
            });

            commands.trigger(AudioFxEvent {
                fx: AudioFxEventType::MeleeWhoosh { volume: 1.0 },
                position: my_pos,
                follow: None,
            });

            if let Some(ai_model) = ai_model {
                commands.trigger(PedestrianAnimationControlEvent {
                    ped: ai_model.0,
                    animation: "Sword_Attack".to_string(),
                    speed: 1.0,
                });
            }

            info!("[AI {:?}] MELEE HIT -> {:?} ({} dmg)", entity, target_entity, SWORD_DAMAGE);
        } else {
            // --- Unarmed (punch) ---
            if perception.target_dist > PUNCH_RANGE {
                continue;
            }
            timers.attack_cooldown = PUNCH_INTERVAL;

            commands.trigger(DamageEvent {
                target: target_entity,
                amount: PUNCH_DAMAGE,
                source: entity,
            });

            let clip = if _crack_utils::random_u32() % 2 == 0 {
                "Punch_Jab"
            } else {
                "Punch_Cross"
            };

            commands.trigger(AudioFxEvent {
                fx: AudioFxEventType::MeleeWhoosh { volume: 0.4 },
                position: my_pos,
                follow: None,
            });

            if let Some(ai_model) = ai_model {
                commands.trigger(PedestrianAnimationControlEvent {
                    ped: ai_model.0,
                    animation: clip.to_string(),
                    speed: 1.0,
                });
            }

            info!("[AI {:?}] PUNCH -> {:?} ({} dmg)", entity, target_entity, PUNCH_DAMAGE);
        }
    }
}
