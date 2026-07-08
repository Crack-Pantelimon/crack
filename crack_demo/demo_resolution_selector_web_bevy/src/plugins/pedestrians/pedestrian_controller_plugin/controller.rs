//! Kinematic character controller systems (ported/adapted from the avian3d
//! `kinematic_character_3d` example).

use avian3d::{math::*, prelude::*};
use bevy::{ecs::query::Has, prelude::*};

use super::*;
use crate::plugins::audio::audio_fx::{AudioFxEvent, AudioFxEventType};
use crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer;
use crate::plugins::map_plugin::{MapTree, TreeMapTile};

/// Reads WASD into a camera-relative move direction and updates modifiers. Space -> jump.
pub fn character_input(
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    controlled: Res<
        crate::plugins::pedestrians::pedestrian_controller_plugin::spawn::ControlledCharacter,
    >,
    mut query: Query<(&mut LocomotionInput, &mut MovementModifiers), With<CharacterController>>,
    q_ejected: Query<&super::EjectedDriver>,
    mut contexts: bevy_egui::EguiContexts,
) {
    let egui_wants_keyboard = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_keyboard_input()
    } else {
        false
    };

    let Some(controller_entity) = controlled.controller else {
        return;
    };
    let Ok((mut input, mut modifiers)) = query.get_mut(controller_entity) else {
        return;
    };

    if egui_wants_keyboard {
        input.move_dir = Vec2::ZERO;
        input.jump = false;
        modifiers.sprint = false;
        modifiers.crouch = false;
        return;
    }

    let Ok(cam) = camera.single() else {
        return;
    };

    if q_ejected.contains(controller_entity) {
        // Freeze all input while the ejected driver plays its on-ground -> stand-up sequence.
        input.move_dir = Vec2::ZERO;
        input.jump = false;
        modifiers.sprint = false;
        modifiers.crouch = false;
        return;
    }

    // Camera forward/right flattened onto the ground plane.
    let mut forward = cam.forward().as_vec3();
    forward.y = 0.0;
    let forward = forward.normalize_or_zero();
    let mut right = cam.right().as_vec3();
    right.y = 0.0;
    let right = right.normalize_or_zero();

    let f = keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) as i8
        - keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) as i8;
    let r = keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) as i8
        - keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) as i8;

    let world = (forward * f as f32 + right * r as f32).normalize_or_zero();
    if world != Vec3::ZERO {
        input.move_dir = Vec2::new(world.x, -world.z);
    } else {
        input.move_dir = Vec2::ZERO;
    }

    // Space is handled by `jump_or_climb` (it decides between jumping and climbing a ledge).

    modifiers.crouch = keys.pressed(KeyCode::KeyC);
    modifiers.sprint = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
}

/// Updates the [`Grounded`] status for character controllers.
pub fn update_grounded(
    mut commands: Commands,
    mut query: Query<(Entity, &GroundDetection, &GlobalTransform)>,
    spatial_query: SpatialQuery,
) {
    for (entity, ground_detection, global_transform) in &mut query {
        let Some(collider) = &ground_detection.cast_shape else {
            continue;
        };

        let translation = global_transform.translation().adjust_precision();
        let rotation = global_transform.rotation().adjust_precision();

        let hit = spatial_query.cast_shape(
            collider,
            translation,
            rotation,
            global_transform.down(),
            &ShapeCastConfig::from_max_distance(ground_detection.max_distance),
            &SpatialQueryFilter::from_excluded_entities([entity]),
        );

        let is_grounded = hit.is_some_and(|hit| {
            let up = global_transform.up().adjust_precision();
            (rotation * hit.normal1).angle_between(up) <= ground_detection.max_angle
        });

        if is_grounded {
            commands.entity(entity).insert(Grounded);
        } else {
            commands.entity(entity).remove::<Grounded>();
        }
    }
}

/// Responds to per-entity [`LocomotionInput`] and accelerates/jumps character controllers.
pub fn movement(
    time: Res<Time>,
    mut controllers: Query<(
        &mut LocomotionInput,
        &CharacterMovementSettings,
        &mut LinearVelocity,
        Has<Grounded>,
    )>,
) {
    let delta_secs = time.delta_secs_f64().adjust_precision();

    for (mut input, movement, mut linear_velocity, is_grounded) in &mut controllers {
        let direction = input.move_dir;
        if direction != Vec2::ZERO {
            linear_velocity.x += direction.x as Scalar * movement.acceleration * delta_secs;
            linear_velocity.z -= direction.y as Scalar * movement.acceleration * delta_secs;
        }

        if input.jump && is_grounded {
            linear_velocity.y = movement.jump_impulse;
        }

        // Consume the per-frame inputs.
        input.move_dir = Vec2::ZERO;
        input.jump = false;
    }
}

/// Applies custom gravity to character controllers.
pub fn apply_gravity(
    time: Res<Time>,
    mut controllers: Query<(&CharacterMovementSettings, &mut LinearVelocity)>,
) {
    let delta_secs = time.delta_secs_f64().adjust_precision();

    for (movement, mut linear_velocity) in &mut controllers {
        let gravity_direction = movement.gravity.normalize_or_zero();

        let velocity_along_gravity = linear_velocity.dot(gravity_direction);
        if velocity_along_gravity > movement.terminal_velocity {
            continue;
        }

        let new_velocity = linear_velocity.0 + movement.gravity * delta_secs;
        let new_velocity_along_gravity = new_velocity.dot(gravity_direction);
        if new_velocity_along_gravity < movement.terminal_velocity {
            linear_velocity.0 = new_velocity;
        } else {
            linear_velocity.0 = gravity_direction * movement.terminal_velocity;
        }
    }
}

/// Exponential decay of horizontal velocity (Y left untouched).
pub fn apply_movement_damping(
    mut query: Query<(&CharacterMovementSettings, &mut LinearVelocity)>,
    time: Res<Time>,
) {
    let delta_secs = time.delta_secs_f64().adjust_precision();

    for (movement, mut linear_velocity) in &mut query {
        linear_velocity.x *= 1.0 / (1.0 + delta_secs * movement.damping);
        linear_velocity.z *= 1.0 / (1.0 + delta_secs * movement.damping);
    }
}

/// Clamps horizontal speed to the current movement-mode cap. Sprint starts at 2x jog speed and
/// ramps toward `SPRINT_MAX_MULT` x jog speed while Shift is held.
pub fn apply_speed_cap(
    time: Res<Time>,
    mut query: Query<(&mut MovementModifiers, &mut LinearVelocity, Has<Rolling>)>,
) {
    let dt = time.delta_secs();
    for (mut modifiers, mut velocity, rolling) in &mut query {
        // A crouch roll drives its own speed; don't clamp it down to the crouch cap.
        if rolling {
            continue;
        }
        // Advance / reset the sprint ramp timer.
        if modifiers.sprint && !modifiers.crouch {
            modifiers.sprint_secs = (modifiers.sprint_secs + dt).min(SPRINT_RAMP_TIME);
        } else {
            modifiers.sprint_secs = 0.0;
        }

        let cap = if modifiers.crouch {
            // Crouch-sprint: Shift while crouched doubles the crouch speed.
            if modifiers.sprint {
                CROUCH_SPEED * CROUCH_SPRINT_MULT
            } else {
                CROUCH_SPEED
            }
        } else if modifiers.sprint {
            let t = modifiers.sprint_secs / SPRINT_RAMP_TIME;
            JOG_SPEED * (1.0 + (SPRINT_MAX_MULT - 1.0) * t)
        } else {
            JOG_SPEED
        } as Scalar;

        let horizontal = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
        if horizontal > cap && horizontal > 0.0 {
            let factor = cap / horizontal;
            velocity.x *= factor;
            velocity.z *= factor;
        }
    }
}

/// Performs move-and-slide for character controllers, sliding along contact surfaces.
pub fn move_and_slide(
    mut query: Query<
        (
            Entity,
            Option<&GroundDetection>,
            Option<&mut CharacterCollisions>,
            &mut Transform,
            &mut LinearVelocity,
            &Collider,
        ),
        With<CharacterController>,
    >,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    for (entity, ground_detection, mut collisions, mut transform, mut lin_vel, collider) in
        &mut query
    {
        let mut hit_ground_or_ceiling = false;

        if let Some(collisions) = &mut collisions {
            collisions.0.clear();
        }

        let up = transform.up().adjust_precision();

        let MoveAndSlideOutput {
            position: new_position,
            projected_velocity,
        } = move_and_slide.move_and_slide(
            collider,
            transform.translation.adjust_precision(),
            transform.rotation.adjust_precision(),
            lin_vel.0,
            time.delta(),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::from_excluded_entities([entity]),
            |hit| {
                let Some(ground_detection) = ground_detection else {
                    return MoveAndSlideHitResponse::Accept;
                };

                let angle = up.angle_between(hit.normal.adjust_precision());
                let is_ground = angle <= ground_detection.max_angle;
                let is_ceiling = is_ground && up.dot(hit.normal.adjust_precision()) < 0.0;

                let [horizontal_component, vertical_component] =
                    split_into_components(lin_vel.0, up);

                let horizontal_velocity_decomposition =
                    decompose_hit_velocity(horizontal_component, *hit.normal, up);
                let decomposition = decompose_hit_velocity(*hit.velocity, *hit.normal, up);

                let slipping_intent =
                    up.dot(horizontal_velocity_decomposition.vertical_tangent) < -0.001;
                let slipping = up.dot(decomposition.vertical_tangent) < -0.001;
                let climbing_intent = up.dot(vertical_component) > 0.0;
                let climbing = up.dot(decomposition.vertical_tangent) > 0.0;

                let projected_velocity = if !is_ground && climbing && !climbing_intent {
                    decomposition.horizontal_tangent + decomposition.normal_part
                } else if is_ground && slipping && !slipping_intent {
                    decomposition.horizontal_tangent + decomposition.normal_part
                } else {
                    decomposition.horizontal_tangent
                        + decomposition.vertical_tangent
                        + decomposition.normal_part
                };

                *hit.velocity = projected_velocity;

                if is_ground || is_ceiling {
                    hit_ground_or_ceiling = true;
                }

                if let Some(collisions) = &mut collisions {
                    collisions.0.push(CharacterCollision {
                        collider: hit.entity,
                        point: hit.point,
                        normal: *hit.normal,
                        character_velocity: *hit.velocity,
                    });
                }

                MoveAndSlideHitResponse::Accept
            },
        );

        transform.translation = new_position.f32();

        if hit_ground_or_ceiling {
            let up = up.adjust_precision();
            let velocity_along_up = lin_vel.dot(up);
            let new_velocity_along_up = projected_velocity.dot(up);
            lin_vel.0 += (new_velocity_along_up - velocity_along_up) * up;
        }
    }
}

struct VelocityDecomposition {
    normal_part: Vector,
    horizontal_tangent: Vector,
    vertical_tangent: Vector,
}

fn decompose_hit_velocity(velocity: Vector, normal: Dir, up: Vector) -> VelocityDecomposition {
    let normal = normal.adjust_precision();
    let normal_part = normal * normal.dot(velocity);
    let tangent_part = velocity - normal_part;

    let horizontal_tangent_dir = normal.cross(up).normalize_or_zero();
    let horizontal_tangent = tangent_part.dot(horizontal_tangent_dir) * horizontal_tangent_dir;
    let vertical_tangent = tangent_part - horizontal_tangent;

    VelocityDecomposition {
        normal_part,
        horizontal_tangent,
        vertical_tangent,
    }
}

fn split_into_components(v: Vector, up: Vector) -> [Vector; 2] {
    let vertical_component = up * v.dot(up);
    let horizontal_component = v - vertical_component;
    [horizontal_component, vertical_component]
}

/// Applies impulses to dynamic rigid bodies the character pushed into.
pub fn apply_forces_to_dynamic_bodies(
    characters: Query<(&ComputedMass, &CharacterCollisions)>,
    colliders: Query<&ColliderOf>,
    mut rigid_bodies: Query<(&RigidBody, Forces)>,
) {
    for (mass, collisions) in &characters {
        let mass = mass.value();
        for collision in &collisions.0 {
            let Ok(collider_of) = colliders.get(collision.collider) else {
                continue;
            };
            let Ok((rigid_body, mut forces)) = rigid_bodies.get_mut(collider_of.body) else {
                continue;
            };
            if !rigid_body.is_dynamic() {
                continue;
            }

            let touch_dir = -collision.normal.adjust_precision();
            let relative_velocity = collision.character_velocity - forces.linear_velocity();
            let touch_velocity = touch_dir.dot(relative_velocity) * touch_dir;
            let impulse = touch_velocity * mass;

            forces.apply_linear_impulse_at_point(impulse, collision.point);
        }
    }
}

/// Rotates the controller (and therefore its model child) to face its horizontal velocity.
pub fn face_movement(
    time: Res<Time>,
    mut query: Query<(&LinearVelocity, &mut Transform), With<CharacterController>>,
) {
    for (velocity, mut transform) in &mut query {
        let vx = velocity.x as f32;
        let vz = velocity.z as f32;
        if Vec2::new(vx, vz).length() < 0.3 {
            continue;
        }
        let target = Quat::from_rotation_y(f32::atan2(vx, vz) + MODEL_FORWARD_OFFSET);
        let s = (TURN_SPEED * time.delta_secs()).clamp(0.0, 1.0);
        transform.rotation = transform.rotation.slerp(target, s);
    }
}

/// Safety net: if the controller ends up below the ground plane (y < 0), teleport it back up.
pub fn respawn_if_fallen(
    mut query: Query<(&mut Transform, &mut LinearVelocity), With<CharacterController>>,
) {
    for (mut transform, mut velocity) in &mut query {
        if transform.translation.y < 0.0 {
            transform.translation.y = 3.0 + CAPSULE_TOTAL_HEIGHT;
            velocity.0 = Vector::ZERO;
        }
    }
}

/// On Space, decide between jumping, climbing a ledge in front, or (while crouched) a forward
/// roll. Climbing is allowed even while airborne/falling; jumping only takes effect when grounded
/// (handled in `movement`).
pub fn jump_or_climb(
    keys: Res<ButtonInput<KeyCode>>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
    controlled: Res<
        crate::plugins::pedestrians::pedestrian_controller_plugin::spawn::ControlledCharacter,
    >,
    map: Option<Res<MapTree>>,
    tiles: Query<(), With<TreeMapTile>>,
    mut query: Query<
        (
            Entity,
            &GlobalTransform,
            &CharacterScale,
            &MovementModifiers,
            &mut LocomotionInput,
            Has<Grounded>,
        ),
        (
            With<CharacterController>,
            Without<Climbing>,
            Without<Rolling>,
        ),
    >,
    mut contexts: bevy_egui::EguiContexts,
) {
    let egui_wants_keyboard = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_keyboard_input()
    } else {
        false
    };
    if egui_wants_keyboard {
        return;
    }

    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    let Some(controller_entity) = controlled.controller else {
        return;
    };
    // Extra off-map safety checks only make sense in the main game (map loaded + tiles active).
    let map_active = map.map(|m| m.parsed).unwrap_or(false) && !tiles.is_empty();

    let Ok((entity, gt, scale, modifiers, mut input, grounded)) = query.get_mut(controller_entity)
    else {
        return;
    };

    // Crouch + Space = forward roll.
    if modifiers.crouch {
        if grounded {
            commands.entity(entity).insert(Rolling {
                elapsed: 0.0,
                duration: ROLL_DURATION,
            });
        }
        return;
    }
    if let Some((start, target)) = detect_climb(gt, scale.0, &spatial_query, entity, map_active) {
        commands.entity(entity).insert(Climbing {
            start,
            target,
            elapsed: 0.0,
            duration: CLIMB_DURATION,
        });
        commands.trigger(AudioFxEvent {
            fx: AudioFxEventType::Climb,
            position: gt.translation(),
            follow: None,
        });
    } else {
        input.jump = true;
    }
}

/// Casts a few rays to detect a climbable ledge directly in front of the character. Returns the
/// climb start and target positions (capsule center) if one is found.
pub(crate) fn detect_climb(
    gt: &GlobalTransform,
    scale: f32,
    spatial_query: &SpatialQuery,
    self_entity: Entity,
    map_active: bool,
) -> Option<(Vec3, Vec3)> {
    let pos = gt.translation();
    let mut forward = gt.rotation() * Vec3::Z;
    forward.y = 0.0;
    let forward = forward.normalize_or_zero();
    let forward_dir = Dir3::new(forward).ok()?;

    let feet_y = pos.y - CAPSULE_HALF_HEIGHT;
    let height = CAPSULE_TOTAL_HEIGHT * scale;
    let min_ledge = CLIMB_MIN_FRAC * height;
    let max_ledge = CLIMB_MAX_FRAC * height;

    let filter = SpatialQueryFilter::from_excluded_entities([self_entity]);

    // 1. There must be a wall directly in front, around lower-body height.
    let wall_origin = Vec3::new(pos.x, feet_y + 0.2 * height, pos.z);
    let reach = CAPSULE_RADIUS + CLIMB_FORWARD_REACH;
    spatial_query.cast_ray(wall_origin, forward_dir, reach, true, &filter)?;

    // 2. Probe downward just past the wall to find the ledge top height.
    let probe = pos + forward * (CAPSULE_RADIUS + 0.35);
    let probe_origin = Vec3::new(probe.x, feet_y + max_ledge + 0.5, probe.z);
    let down_dist = max_ledge + 0.6;
    let top_hit = spatial_query.cast_ray(probe_origin, Dir3::NEG_Y, down_dist, true, &filter)?;
    let top_y = probe_origin.y - top_hit.distance;
    let ledge_height = top_y - feet_y;
    if ledge_height < min_ledge || ledge_height > max_ledge {
        return None;
    }

    // 3. Require head clearance above the ledge so the character can actually stand there.
    let stand = pos + forward * (CAPSULE_RADIUS + 0.5);
    let clearance_origin = Vec3::new(stand.x, top_y + 0.1, stand.z);
    if spatial_query
        .cast_ray(clearance_origin, Dir3::Y, height * 0.8, true, &filter)
        .is_some()
    {
        return None;
    }

    // 4. Off-map guard (main game only): sample the climb-over path with a few extra short rays
    // and verify there is actually ground to land on. If the far side of the wall is a void (map
    // edge), abort the climb instead of vaulting into hell.
    if map_active {
        for t in [0.0_f32, 0.35, 0.7] {
            let sample = stand + forward * t;
            let sample_origin = Vec3::new(sample.x, top_y + 0.4, sample.z);
            if spatial_query
                .cast_ray(sample_origin, Dir3::NEG_Y, 3.0, true, &filter)
                .is_none()
            {
                return None;
            }
        }
    }

    let target = Vec3::new(stand.x, top_y + CAPSULE_HALF_HEIGHT, stand.z);
    Some((pos, target))
}

/// Tweens a climbing character up-then-over onto the ledge, then removes [`Climbing`].
pub fn update_climb(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Transform, &mut LinearVelocity, &mut Climbing)>,
) {
    for (entity, mut transform, mut velocity, mut climb) in &mut query {
        velocity.0 = Vector::ZERO;
        climb.elapsed += time.delta_secs();
        let f = (climb.elapsed / climb.duration).clamp(0.0, 1.0);

        // Up-then-over path so the character does not clip through the wall.
        let up = Vec3::new(climb.start.x, climb.target.y, climb.start.z);
        transform.translation = if f < 0.5 {
            let s = smoothstep(f * 2.0);
            climb.start.lerp(up, s)
        } else {
            let s = smoothstep((f - 0.5) * 2.0);
            up.lerp(climb.target, s)
        };

        if f >= 1.0 {
            commands.entity(entity).remove::<Climbing>();
        }
    }
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Drives an in-progress crouch roll: pushes the character forward along its facing at
/// [`ROLL_SPEED`] until the roll duration elapses.
pub fn update_roll(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &Transform, &mut LinearVelocity, &mut Rolling)>,
) {
    for (entity, transform, mut velocity, mut roll) in &mut query {
        roll.elapsed += time.delta_secs();

        let mut forward = transform.rotation * Vec3::Z;
        forward.y = 0.0;
        let forward = forward.normalize_or_zero();
        velocity.x = (forward.x * ROLL_SPEED) as Scalar;
        velocity.z = (forward.z * ROLL_SPEED) as Scalar;

        if roll.elapsed >= roll.duration {
            commands.entity(entity).remove::<Rolling>();
        }
    }
}

/// Off-map safety net for the main game: if the character somehow ends up *below* the map surface
/// (e.g. clipped through a wall while double-jumping), pop it back on top.
///
/// The ground height at the character's XZ is found by shooting a physics ray upward from just
/// below the map's minimum height, colliding with map-layer (ground) geometry only. Only runs when
/// the main world is loaded and map tiles are active.
pub fn detect_fallen_off_map(
    map: Option<Res<MapTree>>,
    tiles: Query<(), With<TreeMapTile>>,
    spatial_query: SpatialQuery,
    mut query: Query<(Entity, &mut Transform, &mut LinearVelocity), With<CharacterController>>,
) {
    let Some(map) = map else {
        return;
    };
    if !map.parsed || tiles.is_empty() {
        return;
    }

    let min_y = map.bbox.min.y - 1.0;
    let max_y = map.bbox.max.y + 1.0;
    let ray_len = max_y - min_y;
    if ray_len <= 0.0 {
        return;
    }

    for (entity, mut transform, mut velocity) in &mut query {
        let origin = Vec3::new(transform.translation.x, min_y, transform.translation.z);
        // Ground (map tiles) only — cars/props must not count as "the map surface".
        let mut filter = SpatialQueryFilter::from_mask(GamePhysicsLayer::Map);
        filter.excluded_entities.insert(entity);

        if let Some(hit) = spatial_query.cast_ray(origin, Dir3::Y, ray_len, true, &filter) {
            let ground_y = min_y + hit.distance;
            if transform.translation.y < ground_y {
                transform.translation.y = ground_y + CAPSULE_TOTAL_HEIGHT + 0.5;
                velocity.0 = Vector::ZERO;
                info!("Character fell below the map; teleported back above ground.");
            }
        }
    }
}
