use bevy::prelude::*;
use avian3d::prelude::LinearVelocity;
use crate::plugins::cars_driving::driving_plugin::{CarDriveState, Drive};
use super::{TrafficConfig, TrafficCar};

pub fn drive_traffic_cars(
    time: Res<Time>,
    config: Res<TrafficConfig>,
    mut q_cars: Query<(Entity, &Transform, &LinearVelocity, &CarDriveState, &mut TrafficCar)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let target_speed_nominal = config.speed_kmh / 3.6;

    for (entity, transform, lin_vel, _drive_state, mut traffic_car) in q_cars.iter_mut() {
        let car_pos = transform.translation;

        // 1. Advance waypoint index if close in XZ plane
        while traffic_car.next_idx < traffic_car.path.len() {
            let target = traffic_car.path[traffic_car.next_idx];
            let dist_xz = Vec2::new(car_pos.x - target.x, car_pos.z - target.z).length();
            if dist_xz < 4.0 {
                traffic_car.next_idx += 1;
            } else {
                break;
            }
        }

        // 2. Lookahead target
        let mut target_idx = traffic_car.next_idx;
        while target_idx < traffic_car.path.len() {
            let target = traffic_car.path[target_idx];
            let dist_xz = Vec2::new(car_pos.x - target.x, car_pos.z - target.z).length();
            if dist_xz >= 8.0 {
                break;
            }
            target_idx += 1;
        }
        let target_idx = target_idx.min(traffic_car.path.len() - 1);
        if traffic_car.path.is_empty() {
            continue;
        }
        let target = traffic_car.path[target_idx];

        // 3. Steering controller
        let car_fwd = transform.rotation * Vec3::NEG_Z;
        let fwd_xz = Vec2::new(car_fwd.x, car_fwd.z).normalize_or_zero();
        let to_target = Vec2::new(target.x - car_pos.x, target.z - car_pos.z).normalize_or_zero();

        // Perp-dot product for signed angle/steer input
        let cross = fwd_xz.x * to_target.y - fwd_xz.y * to_target.x;
        let steer = (cross * 3.0).clamp(-1.0, 1.0);

        // 4. Throttle / Brake controller
        let dot = fwd_xz.dot(to_target);
        // Slow down near sharp turns
        let target_speed = if dot < 0.707 {
            target_speed_nominal * 0.4
        } else {
            target_speed_nominal
        };

        let current_speed = lin_vel.0.dot(car_fwd);
        let mut accelerate = 0.0;
        let mut brake = 0.0;

        if current_speed < target_speed {
            accelerate = ((target_speed - current_speed) * 0.5).clamp(0.0, 1.0);
        } else if current_speed > target_speed + 2.0 {
            brake = ((current_speed - target_speed) * 0.5).clamp(0.0, 1.0);
        }

        // Trigger input event via a closure as required by EntityCommands::trigger
        commands.entity(entity).trigger(move |entity| Drive {
            entity,
            accelerate,
            brake,
            steer,
        });

        // 5. Stuck detection
        if current_speed.abs() < 0.5 && accelerate > 0.3 {
            traffic_car.stuck_timer += dt;
        } else {
            traffic_car.stuck_timer = 0.0;
        }
    }
}
