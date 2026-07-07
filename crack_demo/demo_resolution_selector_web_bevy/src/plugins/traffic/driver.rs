use super::road_graph::{RerouteMode, TrafficRoadGraph, pick_continuation, quantize};
use super::{
    LOOKAHEAD_XZ, REVERSE_DURATION_S, STUCK_SPEED_EPS, STUCK_TRIGGER_S, TrafficCar, TrafficConfig,
    TrafficDriveMode, WAYPOINT_REACHED_XZ,
};
use crate::plugins::cars_driving::driving_plugin::{CarDriveState, Drive};
use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

pub fn drive_traffic_cars(
    time: Res<Time>,
    config: Res<TrafficConfig>,
    graph: Res<TrafficRoadGraph>,
    mut q_cars: Query<
        (
            Entity,
            &Transform,
            &LinearVelocity,
            &mut CarDriveState,
            &mut TrafficCar,
        ),
        Without<crate::plugins::cars_driving::driving_plugin::spawn_car::DisabledCar>,
    >,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 || !graph.built {
        return;
    }

    let target_speed_nominal = config.speed_kmh / 3.6;

    for (entity, transform, lin_vel, mut drive_state, mut traffic_car) in q_cars.iter_mut() {
        let car_pos = transform.translation;

        if traffic_car.state.path.is_empty() {
            continue;
        }

        match traffic_car.mode {
            TrafficDriveMode::Normal => {
                // Ensure is_reverse is false in normal mode
                drive_state.is_reverse = false;

                // 1. Advance waypoint index if close in XZ plane
                while traffic_car.state.next_idx < traffic_car.state.path.len() {
                    let target = traffic_car.state.path[traffic_car.state.next_idx];
                    let dist_xz = Vec2::new(car_pos.x - target.x, car_pos.z - target.z).length();
                    if dist_xz < WAYPOINT_REACHED_XZ {
                        traffic_car.state.next_idx += 1;
                    } else {
                        break;
                    }
                }

                // 2. Continuous routing: check if near end of path
                if traffic_car.state.next_idx >= traffic_car.state.path.len() - 1 {
                    let last_node = quantize(*traffic_car.state.path.last().unwrap());
                    let car_fwd = transform.rotation * Vec3::NEG_Z;

                    if let Some((next_seg, next_points)) = pick_continuation(
                        &graph,
                        last_node,
                        traffic_car.state.current_seg,
                        RerouteMode::ClosestAngle(car_fwd),
                    ) {
                        let mut new_path =
                            traffic_car.state.path[traffic_car.state.next_idx..].to_vec();
                        new_path.extend(next_points[1..].iter().cloned());
                        traffic_car.state.path = new_path;
                        traffic_car.state.next_idx = 0;
                        traffic_car.state.current_seg = next_seg;
                    } else {
                        // Dead end fallback: reverse on the current segment
                        let seg = &graph.segments[traffic_car.state.current_seg];
                        let start_quant = quantize(seg.points[0]);
                        let reversed_points: Vec<Vec3> = if start_quant == last_node {
                            seg.points.clone()
                        } else {
                            seg.points.iter().cloned().rev().collect()
                        };

                        let mut new_path =
                            traffic_car.state.path[traffic_car.state.next_idx..].to_vec();
                        new_path.extend(reversed_points[1..].iter().cloned());
                        traffic_car.state.path = new_path;
                        traffic_car.state.next_idx = 0;
                    }
                }

                // 3. Lookahead target
                let mut target_idx = traffic_car.state.next_idx;
                while target_idx < traffic_car.state.path.len() {
                    let target = traffic_car.state.path[target_idx];
                    let dist_xz = Vec2::new(car_pos.x - target.x, car_pos.z - target.z).length();
                    if dist_xz >= LOOKAHEAD_XZ {
                        break;
                    }
                    target_idx += 1;
                }
                let target_idx = target_idx.min(traffic_car.state.path.len() - 1);
                if traffic_car.state.path.is_empty() {
                    continue;
                }
                let target = traffic_car.state.path[target_idx];

                // 4. Steering controller
                let car_fwd = transform.rotation * Vec3::NEG_Z;
                let fwd_xz = Vec2::new(car_fwd.x, car_fwd.z).normalize_or_zero();
                let to_target =
                    Vec2::new(target.x - car_pos.x, target.z - car_pos.z).normalize_or_zero();

                // Perp-dot product for signed angle/steer input
                let cross = fwd_xz.x * to_target.y - fwd_xz.y * to_target.x;
                let steer = (cross * 3.0).clamp(-1.0, 1.0);

                // 5. Throttle / Brake controller
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

                // Trigger input event
                commands.entity(entity).trigger(move |entity| Drive {
                    entity,
                    accelerate,
                    brake,
                    steer,
                });

                // Stuck detection
                if current_speed.abs() < STUCK_SPEED_EPS && accelerate > 0.3 {
                    traffic_car.state.stuck_timer += dt;
                    if traffic_car.state.stuck_timer > STUCK_TRIGGER_S {
                        traffic_car.mode = TrafficDriveMode::Reversing(REVERSE_DURATION_S);
                    }
                } else {
                    traffic_car.state.stuck_timer = 0.0;
                }
            }
            TrafficDriveMode::Reversing(mut remaining) => {
                // Command reversing drive
                drive_state.is_reverse = true;

                commands.entity(entity).trigger(move |entity| Drive {
                    entity,
                    accelerate: 1.0,
                    brake: 0.0,
                    steer: 0.0,
                });

                // Accumulate stuck timer (for hard despawn)
                let car_fwd = transform.rotation * Vec3::NEG_Z;
                let current_speed = lin_vel.0.dot(car_fwd);
                if current_speed.abs() < STUCK_SPEED_EPS {
                    traffic_car.state.stuck_timer += dt;
                } else {
                    traffic_car.state.stuck_timer = 0.0;
                }

                remaining -= dt;
                if remaining <= 0.0 {
                    drive_state.is_reverse = false;

                    // Reroute randomly from current nearest node
                    let seg = &graph.segments[traffic_car.state.current_seg];
                    let dist_a = seg.points[0].distance(car_pos);
                    let dist_b = seg.points.last().unwrap().distance(car_pos);
                    let nearest_node = if dist_a < dist_b {
                        quantize(seg.points[0])
                    } else {
                        quantize(*seg.points.last().unwrap())
                    };

                    if let Some((next_seg, next_points)) = pick_continuation(
                        &graph,
                        nearest_node,
                        traffic_car.state.current_seg,
                        RerouteMode::Random,
                    ) {
                        traffic_car.state.path = next_points;
                        traffic_car.state.next_idx = 1;
                        traffic_car.state.current_seg = next_seg;
                    } else {
                        // Snap to nearest segment overall fallback using build_path_from
                        if let Some((closest_seg_idx, path_points)) =
                            super::common::build_path_from(&graph, car_pos)
                        {
                            traffic_car.state.path = path_points;
                            traffic_car.state.next_idx = 1;
                            traffic_car.state.current_seg = closest_seg_idx;
                        }
                    }

                    traffic_car.mode = TrafficDriveMode::Normal;
                    traffic_car.state.stuck_timer = 0.0;
                } else {
                    traffic_car.mode = TrafficDriveMode::Reversing(remaining);
                }
            }
        }
    }
}
