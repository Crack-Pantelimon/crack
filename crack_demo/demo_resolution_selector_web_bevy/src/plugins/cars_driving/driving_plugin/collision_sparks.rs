use crate::plugins::cars_driving::driving_plugin::spawn_car::Car;
use avian3d::prelude::*;
use bevy::prelude::*;
use std::collections::HashMap;
use tracing::info;

pub const MAX_SPARK_EVENTS_PER_CAR_PER_SEC: usize = 3;
pub const MAX_SPARK_EVENTS_GLOBAL_PER_SEC: usize = 20;

const GRAVITY: f32 = 9.81;

#[derive(Resource, Default)]
pub struct SparkRateLimiter {
    pub per_car: HashMap<Entity, Vec<f32>>,
    pub global: Vec<f32>,
}

#[derive(Component)]
pub struct CollisionMarker {
    pub position: Vec3,
    pub relative_speed: f32,
    pub spawn_time: f32,
    pub lifetime: f32,
}

#[derive(Component)]
pub struct SparkParticle {
    pub velocity: Vec3,
    pub spawn_time: f32,
    pub lifetime: f32,
    pub history: Vec<(Vec3, f32)>,
}

/// Helper function to find if an entity or any of its ancestors has the `Car` component.
fn find_car_entity(
    entity: Entity,
    q_car: &Query<&Car>,
    q_parent: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if q_car.contains(current) {
            return Some(current);
        }
        if let Ok(child_of) = q_parent.get(current) {
            current = child_of.parent();
        } else {
            return None;
        }
    }
}

pub fn handle_car_collisions(
    mut commands: Commands,
    mut collision_events: MessageReader<CollisionStart>,
    mut rate_limiter: ResMut<SparkRateLimiter>,
    spatial_query: SpatialQuery,
    q_car: Query<&Car>,
    q_parent: Query<&ChildOf>,
    q_lin_vel: Query<&LinearVelocity>,
    q_gt: Query<&GlobalTransform>,
    q_name: Query<&Name>,
    time: Res<Time>,
) {
    let current_time = time.elapsed_secs();

    // Clean up timestamps older than 1 second
    rate_limiter.global.retain(|&t| current_time - t <= 1.0);
    for timestamps in rate_limiter.per_car.values_mut() {
        timestamps.retain(|&t| current_time - t <= 1.0);
    }

    for ev in collision_events.read() {
        let car1_opt = find_car_entity(ev.collider1, &q_car, &q_parent)
            .or_else(|| ev.body1.and_then(|b| find_car_entity(b, &q_car, &q_parent)));
        let car2_opt = find_car_entity(ev.collider2, &q_car, &q_parent)
            .or_else(|| ev.body2.and_then(|b| find_car_entity(b, &q_car, &q_parent)));

        if car1_opt.is_none() && car2_opt.is_none() {
            continue;
        }

        let (car_entity, other_entity, other_collider) = if let Some(c1) = car1_opt {
            let other = ev.body2.unwrap_or(ev.collider2);
            (c1, other, ev.collider2)
        } else {
            let other = ev.body1.unwrap_or(ev.collider1);
            (car2_opt.unwrap(), other, ev.collider1)
        };

        let car_gt = q_gt
            .get(car_entity)
            .map(|gt| gt.translation())
            .unwrap_or(Vec3::ZERO);
        let car_vel = q_lin_vel.get(car_entity).map(|v| v.0).unwrap_or(Vec3::ZERO);
        let car_speed = car_vel.length();

        let other_gt = q_gt
            .get(other_collider)
            .or_else(|_| q_gt.get(other_entity))
            .map(|gt| gt.translation())
            .unwrap_or(car_gt);
        let other_vel = q_lin_vel
            .get(other_entity)
            .map(|v| v.0)
            .unwrap_or(Vec3::ZERO);
        let other_name = q_name
            .get(other_entity)
            .or_else(|_| q_name.get(other_collider))
            .map(|n| n.as_str())
            .unwrap_or("Environment/Ground");

        let rel_vel_vec = car_vel - other_vel;
        let rel_speed = rel_vel_vec.length();

        if rel_speed < 0.3 {
            continue;
        }

        // 1. Check Rate Limits (3 per car/sec, 20 global/sec)
        if rate_limiter.global.len() >= MAX_SPARK_EVENTS_GLOBAL_PER_SEC {
            continue;
        }

        let car_events_count = rate_limiter
            .per_car
            .get(&car_entity)
            .map_or(0, |ts| ts.len());
        if car_events_count >= MAX_SPARK_EVENTS_PER_CAR_PER_SEC {
            continue;
        }

        // Record event timestamp for rate limiting
        rate_limiter.global.push(current_time);
        rate_limiter
            .per_car
            .entry(car_entity)
            .or_default()
            .push(current_time);

        // Calculate collision point in global space
        let cast_dir = if rel_speed > 0.1 {
            rel_vel_vec.normalize()
        } else {
            (other_gt - car_gt).normalize_or_zero()
        };

        let mut collision_point = car_gt;

        // Spatial raycast from car position to find exact surface impact location in global coordinates
        let filter = SpatialQueryFilter::default().with_excluded_entities([car_entity]);
        if let Ok(dir) = Dir3::new(cast_dir) {
            if let Some(hit) = spatial_query.cast_ray(car_gt, dir, 10.0, true, &filter) {
                collision_point = car_gt + cast_dir * hit.distance;
            } else {
                collision_point = car_gt.lerp(other_gt, 0.5);
            }
        }

        // Detailed tracing log as requested by user
        info!(
            "💥 CAR COLLISION DETECTED!\n\
             ├─ Car Entity: {:?}\n\
             ├─ Car Global Pos: Vec3({:.2}, {:.2}, {:.2})\n\
             ├─ Car Speed: {:.2} m/s ({:.1} km/h)\n\
             ├─ Collision Point (Global): Vec3({:.2}, {:.2}, {:.2})\n\
             ├─ Relative Speed: {:.2} m/s\n\
             ├─ Other Entity: {:?} ({})\n\
             └─ Other Global Pos: Vec3({:.2}, {:.2}, {:.2})",
            car_entity,
            car_gt.x,
            car_gt.y,
            car_gt.z,
            car_speed,
            car_speed * 3.6,
            collision_point.x,
            collision_point.y,
            collision_point.z,
            rel_speed,
            other_entity,
            other_name,
            other_gt.x,
            other_gt.y,
            other_gt.z,
        );

        // Spawn gizmo collision marker (lives for 5s)
        commands.spawn(CollisionMarker {
            position: collision_point,
            relative_speed: rel_speed,
            spawn_time: current_time,
            lifetime: 5.0,
        });

        // Spawn short-lived spark objects (no collision physics - pure visual particles)
        let spark_count = (rel_speed * 1.5).clamp(8.0, 24.0) as usize;

        for _ in 0..spark_count {
            let rx = rand::random::<f32>() * 2.0 - 1.0;
            let ry = rand::random::<f32>() * 1.2 + 0.3;
            let rz = rand::random::<f32>() * 2.0 - 1.0;
            let rand_dir = Vec3::new(rx, ry, rz).normalize_or_zero();

            let spark_speed = rand::random::<f32>() * 8.0 + 3.0 + rel_speed * 0.4;
            let initial_velocity = rand_dir * spark_speed + car_vel * 0.2;
            let offset = Vec3::new(
                rand::random::<f32>() * 0.2 - 0.1,
                rand::random::<f32>() * 0.2 - 0.1,
                rand::random::<f32>() * 0.2 - 0.1,
            );

            let spawn_pos = collision_point + offset;

            commands.spawn((
                Transform::from_translation(spawn_pos),
                SparkParticle {
                    velocity: initial_velocity,
                    spawn_time: current_time,
                    lifetime: 3.0,
                    history: vec![(spawn_pos, current_time)],
                },
            ));
        }
    }
}

pub fn update_and_draw_collision_effects(
    mut commands: Commands,
    time: Res<Time>,
    mut gizmos: Gizmos,
    q_markers: Query<(Entity, &CollisionMarker)>,
    mut q_sparks: Query<(Entity, &mut Transform, &mut SparkParticle)>,
    ui_state: Option<Res<crate::ui_egui::UiState>>,
) {
    let current_time = time.elapsed_secs();
    let dt = time.delta_secs();
    let draw_spark_origin = ui_state
        .as_ref()
        .map_or(false, |s| s.draw_spark_origin_gizmos);

    // 1. Draw collision markers (point and sphere for 5 seconds)
    for (entity, marker) in q_markers.iter() {
        let age = current_time - marker.spawn_time;
        if age >= marker.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        if draw_spark_origin {
            let alpha = (1.0 - (age / marker.lifetime)).clamp(0.0, 1.0);
            let sphere_radius = 0.35 + (marker.relative_speed * 0.05).min(0.85);

            // Gizmo point (bright inner sphere)
            gizmos.sphere(marker.position, 0.08, Color::srgba(1.0, 1.0, 0.3, alpha));
            // Gizmo sphere around collision location
            gizmos.sphere(
                marker.position,
                sphere_radius,
                Color::srgba(1.0, 0.5, 0.05, alpha * 0.85),
            );
        }
    }

    // 2. Update spark positions (no physics colliders, fall through floor) & draw trails
    for (entity, mut transform, mut spark) in q_sparks.iter_mut() {
        let age = current_time - spark.spawn_time;
        if age >= spark.lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Apply ballistic motion (gravity)
        spark.velocity.y -= GRAVITY * dt;
        transform.translation += spark.velocity * dt;

        let pos = transform.translation;

        // Add current position to trail history if moved
        if let Some(last) = spark.history.last() {
            if last.0.distance(pos) > 0.001 {
                spark.history.push((pos, current_time));
            }
        } else {
            spark.history.push((pos, current_time));
        }

        // Keep trail samples younger than 0.3s for rendering
        spark.history.retain(|(_, t)| current_time - t <= 0.3);

        // Draw spark sphere gizmo (small sphere)
        let spark_alpha = (1.0 - (age / spark.lifetime)).clamp(0.2, 1.0);
        gizmos.sphere(pos, 0.04, Color::srgba(1.0, 0.9, 0.2, spark_alpha));

        // Draw trail segments visible for 0.2s each
        for i in 0..spark.history.len().saturating_sub(1) {
            let (p1, t1) = spark.history[i];
            let (p2, _) = spark.history[i + 1];
            let seg_age = current_time - t1;
            if seg_age <= 0.2 {
                let fade = (1.0 - (seg_age / 0.2)).clamp(0.0, 1.0) * spark_alpha;
                let trail_color = Color::srgba(1.0, 0.65, 0.1, fade);
                gizmos.line(p1, p2, trail_color);
            }
        }
    }
}
