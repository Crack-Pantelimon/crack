//! Kinematic character controller systems (ported/adapted from the avian3d
//! `kinematic_character_3d` example).

use avian3d::{math::*, prelude::*};
use bevy::{ecs::query::Has, prelude::*};

use super::*;

/// Reads WASD into a camera-relative move direction and updates modifiers. Space -> jump.
pub fn character_input(
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut modifiers: Query<&mut MovementModifiers>,
    mut movement_writer: MessageWriter<MovementAction>,
) {
    let Ok(cam) = camera.single() else {
        return;
    };

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
        movement_writer.write(MovementAction::Move(Vector2::new(
            world.x as Scalar,
            -world.z as Scalar,
        )));
    }

    if keys.just_pressed(KeyCode::Space) {
        movement_writer.write(MovementAction::Jump);
    }

    for mut m in &mut modifiers {
        m.crouch = keys.pressed(KeyCode::KeyC);
        m.sprint = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    }
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

/// Responds to [`MovementAction`] events and accelerates/jumps character controllers.
pub fn movement(
    time: Res<Time>,
    mut movement_reader: MessageReader<MovementAction>,
    mut controllers: Query<(&CharacterMovementSettings, &mut LinearVelocity, Has<Grounded>)>,
) {
    let delta_secs = time.delta_secs_f64().adjust_precision();

    for event in movement_reader.read() {
        for (movement, mut linear_velocity, is_grounded) in &mut controllers {
            match event {
                MovementAction::Move(direction) => {
                    linear_velocity.x += direction.x * movement.acceleration * delta_secs;
                    linear_velocity.z -= direction.y * movement.acceleration * delta_secs;
                }
                MovementAction::Jump => {
                    if is_grounded {
                        linear_velocity.y = movement.jump_impulse;
                    }
                }
            }
        }
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
    mut query: Query<(&mut MovementModifiers, &mut LinearVelocity)>,
) {
    let dt = time.delta_secs();
    for (mut modifiers, mut velocity) in &mut query {
        // Advance / reset the sprint ramp timer.
        if modifiers.sprint && !modifiers.crouch {
            modifiers.sprint_secs = (modifiers.sprint_secs + dt).min(SPRINT_RAMP_TIME);
        } else {
            modifiers.sprint_secs = 0.0;
        }

        let cap = if modifiers.crouch {
            CROUCH_SPEED
        } else if modifiers.sprint {
            let t = modifiers.sprint_secs / SPRINT_RAMP_TIME;
            JOG_SPEED * (2.0 + (SPRINT_MAX_MULT - 2.0) * t)
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
