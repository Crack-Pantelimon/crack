//! AI movement and steering: translates AiState + perception into LocomotionInput.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::plugins::pedestrians::pedestrian_controller_plugin::{
    LocomotionInput, MovementModifiers,
};
use crate::plugins::weapons::EquippedWeapon;

use super::{AiCombatTimers, AiPedestrian, AiPerception, AiState, AiSteer, debug_ui::AiDebug};

// -------------------------------------------------------------------------------------
// Constants
// -------------------------------------------------------------------------------------

/// Gun standoff band: try to stay between these distances from the target.
const GUN_MIN: f32 = 12.0;
const GUN_MAX: f32 = 30.0;
/// Length of the perpendicular probe rays for flanking.
const FLANK_PROBE: f32 = 4.0;
/// How often to recompute the flank direction.
const FLANK_REPATH: f32 = 1.5;
/// Number of radial directions for cover/flee probing.
const COVER_DIRS: usize = 8;
const FLEE_DIRS: usize = 8;
/// How often to recompute the flee direction.
const FLEE_REPATH: f32 = 0.75;

/// Writes [`LocomotionInput`] and [`MovementModifiers`] for each AI ped based on state.
pub fn ai_movement(
    spatial_query: SpatialQuery,
    ai_debug: Res<AiDebug>,
    time: Res<Time>,
    mut query: Query<
        (
            Entity,
            &mut Transform,
            &AiState,
            &AiPerception,
            &mut AiCombatTimers,
            &mut AiSteer,
            &mut LocomotionInput,
            &mut MovementModifiers,
            &super::faction::Health,
            Option<&EquippedWeapon>,
        ),
        With<AiPedestrian>,
    >,
) {
    for (
        entity,
        mut transform,
        state,
        perception,
        mut timers,
        mut steer,
        mut input,
        mut modifiers,
        health,
        equipped,
    ) in &mut query
    {
        if health.current <= 0.0 {
            input.move_dir = Vec2::ZERO;
            continue;
        }
        let my_pos = transform.translation;
        let is_gun = equipped.is_some_and(|e| e.0.is_gun());
        let record_probes = ai_debug.show_rays;

        if record_probes {
            steer.last_probes.clear();
        }

        modifiers.crouch = false;
        modifiers.sprint = false;

        let dir = match state {
            AiState::Idle => Vec3::ZERO,

            AiState::Hunt => {
                if !perception.visible {
                    Vec3::ZERO
                } else if is_gun {
                    // Gun: keep standoff band.
                    let to_target = (perception.target_pos - my_pos).with_y(0.0);
                    let dist = to_target.length();
                    let to_target_norm = to_target.normalize_or_zero();

                    if dist < GUN_MIN {
                        // Too close — back away.
                        -to_target_norm
                    } else if dist > GUN_MAX {
                        // Too far — approach.
                        to_target_norm
                    } else {
                        // In range — strafe/flank.
                        if timers.repath_timer <= 0.0 {
                            timers.repath_timer = FLANK_REPATH;
                            // Cast two perpendicular probes.
                            let right = Vec3::new(to_target_norm.z, 0.0, -to_target_norm.x);
                            let left = -right;
                            let filter = SpatialQueryFilter::from_excluded_entities([entity]);

                            let right_clear = spatial_query
                                .cast_ray(
                                    my_pos,
                                    Dir3::new(right).unwrap_or(Dir3::X),
                                    FLANK_PROBE,
                                    true,
                                    &filter,
                                )
                                .is_none();
                            let left_clear = spatial_query
                                .cast_ray(
                                    my_pos,
                                    Dir3::new(left).unwrap_or(Dir3::NEG_X),
                                    FLANK_PROBE,
                                    true,
                                    &filter,
                                )
                                .is_none();

                            if record_probes {
                                steer.last_probes.push((
                                    my_pos,
                                    my_pos + right * FLANK_PROBE,
                                    Color::srgb(1.0, 1.0, 0.0),
                                ));
                                steer.last_probes.push((
                                    my_pos,
                                    my_pos + left * FLANK_PROBE,
                                    Color::srgb(1.0, 1.0, 0.0),
                                ));
                            }

                            let chosen = if right_clear && !left_clear {
                                right
                            } else if left_clear && !right_clear {
                                left
                            } else if _crack_utils::random_u32() % 2 == 0 {
                                right
                            } else {
                                left
                            };
                            steer.desired = chosen;
                        }
                        steer.desired
                    }
                } else {
                    let to_target = (perception.target_pos - my_pos).with_y(0.0);
                    let dist = to_target.length();
                    let dir = to_target.normalize_or_zero();

                    if dist <= 1.2 {
                        // Rotate directly towards target
                        if dir != Vec3::ZERO {
                            let target_rot = Quat::from_rotation_y(f32::atan2(dir.x, dir.z));
                            let s = (12.0 * time.delta_secs()).clamp(0.0, 1.0);
                            transform.rotation = transform.rotation.slerp(target_rot, s);
                        }
                        steer.desired = Vec3::ZERO;
                        Vec3::ZERO
                    } else {
                        // Melee/unarmed: sprint straight at target.
                        modifiers.sprint = true;

                        // Check for obstacle at knee height: if blocked, try to jump.
                        let knee = my_pos + Vec3::Y * 0.3;
                        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
                        if let Ok(fwd) = Dir3::new(dir) {
                            if spatial_query
                                .cast_ray(knee, fwd, 1.0, true, &filter)
                                .is_some()
                            {
                                input.jump = true;
                            }
                        }

                        steer.desired = dir;
                        dir
                    }
                }
            }

            AiState::Reposition => {
                // Crouch and find cover.
                modifiers.crouch = true;
                let target_pos = perception.target_pos;
                let away = (my_pos - target_pos).with_y(0.0).normalize_or_zero();

                if timers.repath_timer <= 0.0 {
                    timers.repath_timer = FLANK_REPATH;
                    let filter = SpatialQueryFilter::from_excluded_entities([entity]);

                    let mut best_dir = away;
                    let mut best_score = f32::MIN;

                    for i in 0..COVER_DIRS {
                        let angle = (i as f32 / COVER_DIRS as f32) * std::f32::consts::TAU;
                        let probe_dir = Vec3::new(angle.cos(), 0.0, angle.sin());
                        if let Ok(d) = Dir3::new(probe_dir) {
                            let hit = spatial_query.cast_ray(my_pos, d, 10.0, true, &filter);
                            let dist = hit.map(|h| h.distance).unwrap_or(10.0);
                            // Prefer directions that have geometry (cover) between us and target,
                            // biased toward "away from target".
                            let away_dot = probe_dir.dot(away);
                            let score = if dist < perception.target_dist {
                                // There's geometry closer than the target — potential cover.
                                away_dot * 2.0 + (1.0 / (dist + 0.5))
                            } else {
                                away_dot
                            };
                            if score > best_score {
                                best_score = score;
                                best_dir = probe_dir;
                            }

                            if record_probes {
                                steer.last_probes.push((
                                    my_pos,
                                    my_pos + probe_dir * dist.min(10.0),
                                    Color::srgb(1.0, 1.0, 0.0),
                                ));
                            }
                        }
                    }
                    steer.desired = best_dir;
                }
                steer.desired
            }

            AiState::Flee => {
                modifiers.sprint = true;
                let target_pos = perception.target_pos;
                let away = (my_pos - target_pos).with_y(0.0).normalize_or_zero();

                if timers.repath_timer <= 0.0 {
                    timers.repath_timer = FLEE_REPATH;
                    let filter = SpatialQueryFilter::from_excluded_entities([entity]);

                    let mut best_dir = away;
                    let mut best_dist = 0.0_f32;

                    for i in 0..FLEE_DIRS {
                        let angle = (i as f32 / FLEE_DIRS as f32) * std::f32::consts::TAU;
                        let probe_dir = Vec3::new(angle.cos(), 0.0, angle.sin());
                        // Bias toward "away from target".
                        let bias = probe_dir.dot(away).max(0.0);
                        if bias < 0.2 {
                            continue; // Don't flee toward the enemy.
                        }
                        if let Ok(d) = Dir3::new(probe_dir) {
                            let hit = spatial_query.cast_ray(my_pos, d, 30.0, true, &filter);
                            let dist = hit.map(|h| h.distance).unwrap_or(30.0);
                            let score = dist * bias;

                            if record_probes {
                                let c = if score > best_dist {
                                    Color::srgb(0.0, 1.0, 1.0) // highlight chosen
                                } else {
                                    Color::srgba(0.0, 0.8, 0.8, 0.5)
                                };
                                steer.last_probes.push((
                                    my_pos,
                                    my_pos + probe_dir * dist.min(30.0),
                                    c,
                                ));
                            }

                            if score > best_dist {
                                best_dist = score;
                                best_dir = probe_dir;
                            }
                        }
                    }
                    steer.desired = best_dir;
                }
                steer.desired
            }
        };

        // Convert world planar direction to LocomotionInput convention.
        if dir != Vec3::ZERO {
            let d = dir.normalize_or_zero();
            input.move_dir = Vec2::new(d.x, -d.z);
        } else {
            input.move_dir = Vec2::ZERO;
        }
    }
}
