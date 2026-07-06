use avian3d::prelude::{AngularVelocity, LinearVelocity, SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use crate::plugins::cars_driving::driving_plugin::{
    CarDriveState, GamePhysicsLayer, spawn_car::Car,
};

pub const NUM_PREDICTION_STEPS: usize = 8;
pub const STEP_DELTA_TIME: f32 = 0.030; // 30ms per step -> 240ms horizon

#[derive(Clone, Debug)]
pub struct SpeculativeStepData {
    pub predicted_position: Vec3,
    pub predicted_ground_pos: Vec3,
    pub predicted_rotation: Quat,
    pub left_hit_point: Option<Vec3>,
    pub right_hit_point: Option<Vec3>,
    pub center_hit_point: Option<Vec3>,
    pub left_ray_origin: Vec3,
    pub right_ray_origin: Vec3,
    pub center_ray_origin: Vec3,
    pub has_ground_contact: bool,
}

impl Default for SpeculativeStepData {
    fn default() -> Self {
        Self {
            predicted_position: Vec3::ZERO,
            predicted_ground_pos: Vec3::ZERO,
            predicted_rotation: Quat::IDENTITY,
            left_hit_point: None,
            right_hit_point: None,
            center_hit_point: None,
            left_ray_origin: Vec3::ZERO,
            right_ray_origin: Vec3::ZERO,
            center_ray_origin: Vec3::ZERO,
            has_ground_contact: false,
        }
    }
}

#[derive(Component, Clone, Debug, Default)]
pub struct CarSpeculativeContactData {
    pub steps: Vec<SpeculativeStepData>,
}

pub fn simulate_rk4_future_steps(
    p0: Vec3,
    v0: Vec3,
    q0: Quat,
    ang_vel0: Vec3,
    drive_state: &CarDriveState,
) -> Vec<(Vec3, Vec3, Quat)> {
    let mut steps = Vec::with_capacity(NUM_PREDICTION_STEPS);
    let mut p = p0;
    let mut v = v0;
    let mut q = q0;
    let w = ang_vel0;
    let dt = STEP_DELTA_TIME;

    let mass = drive_state.car_mass.max(1.0);
    let power_watts = drive_state.horsepower * 745.7;
    let max_speed_mps = drive_state.car_max_speed / 3.6;

    for _ in 0..NUM_PREDICTION_STEPS {
        let get_accel = |_eval_p: Vec3, eval_v: Vec3, eval_q: Quat| -> Vec3 {
            let fwd_dir = eval_q * Vec3::NEG_Z;
            let fwd_xz = Vec3::new(fwd_dir.x, 0.0, fwd_dir.z).normalize_or_zero();
            let forward_speed = eval_v.dot(fwd_xz);

            let mut accel = Vec3::ZERO;
            if drive_state.avg_accelerate > 0.0 {
                let speed_for_power = forward_speed.abs().max(2.0);
                let speed_ratio = (forward_speed.abs() / max_speed_mps).min(1.0);
                let force_scale = (1.0 - speed_ratio).max(0.0);
                let mut drive_force_mag =
                    (power_watts / speed_for_power) * drive_state.avg_accelerate * force_scale;
                drive_force_mag = drive_force_mag.min(mass * 9.81);
                let dir = if drive_state.is_reverse {
                    -fwd_xz
                } else {
                    fwd_xz
                };
                accel += dir * (drive_force_mag / mass);
            }

            if drive_state.avg_brake > 0.0 {
                let decel = drive_state.avg_brake * 11.8;
                accel -= eval_v.normalize_or_zero() * decel;
            }

            let speed_xz = Vec3::new(eval_v.x, 0.0, eval_v.z).length();
            if speed_xz > 0.1 {
                let drag_decel = (0.46 * speed_xz * speed_xz / mass) + 0.59;
                accel -= eval_v.normalize_or_zero() * drag_decel;
            }

            accel
        };

        // 4th-Order Runge-Kutta Integration for (position, velocity)
        let k1_v = get_accel(p, v, q);
        let k1_p = v;

        let k2_v = get_accel(p + k1_p * (0.5 * dt), v + k1_v * (0.5 * dt), q);
        let k2_p = v + k1_v * (0.5 * dt);

        let k3_v = get_accel(p + k2_p * (0.5 * dt), v + k2_v * (0.5 * dt), q);
        let k3_p = v + k2_v * (0.5 * dt);

        let k4_v = get_accel(p + k3_p * dt, v + k3_v * dt, q);
        let k4_p = v + k3_v * dt;

        p += (k1_p + k2_p * 2.0 + k3_p * 2.0 + k4_p) * (dt / 6.0);
        v += (k1_v + k2_v * 2.0 + k3_v * 2.0 + k4_v) * (dt / 6.0);

        let rot_axis = w * dt;
        if rot_axis.length_squared() > 1e-8 {
            q = (Quat::from_scaled_axis(rot_axis) * q).normalize();
        }

        steps.push((p, v, q));
    }

    steps
}

pub fn update_speculative_contacts_system(
    spatial_query: SpatialQuery,
    mut q_cars: Query<
        (
            Entity,
            &Transform,
            &LinearVelocity,
            &AngularVelocity,
            &CarDriveState,
            Option<&mut CarSpeculativeContactData>,
        ),
        With<Car>,
    >,
    mut commands: Commands,
) {
    for (car_entity, car_transform, lin_vel, ang_vel, drive_state, speculative_opt) in
        q_cars.iter_mut()
    {
        let p0 = car_transform.translation;
        let v0 = lin_vel.0;
        let q0 = car_transform.rotation;
        let w0 = ang_vel.0;

        let future_states = simulate_rk4_future_steps(p0, v0, q0, w0, drive_state);

        let filter = SpatialQueryFilter::from_mask([GamePhysicsLayer::Map])
            .with_excluded_entities([car_entity]);
        let max_spec_dist = drive_state.max_ray_length * 1.5;
        let solid = true;

        let mut spec_steps = Vec::with_capacity(NUM_PREDICTION_STEPS);

        for (pred_p, _pred_v, pred_q) in future_states {
            // Front-attached speculative ray origins (at predicted car front)
            let fwd_offset = pred_q * Vec3::new(0.0, 0.0, -drive_state.car_half_length);
            let front_center = pred_p + fwd_offset;

            let left_orig = front_center
                + pred_q
                    * Vec3::new(
                        -drive_state.car_half_width,
                        drive_state.ray_start_y_offset,
                        0.0,
                    );
            let right_orig = front_center
                + pred_q
                    * Vec3::new(
                        drive_state.car_half_width,
                        drive_state.ray_start_y_offset,
                        0.0,
                    );
            let center_orig =
                front_center + pred_q * Vec3::new(0.0, drive_state.ray_start_y_offset, 0.0);

            let ray_dir_vec = pred_q * Vec3::NEG_Y;
            let ray_dir = Dir3::new(ray_dir_vec).unwrap_or(Dir3::NEG_Y);

            let left_hit = spatial_query
                .cast_ray(left_orig, ray_dir, max_spec_dist, solid, &filter)
                .map(|h| left_orig + *ray_dir * h.distance);
            let right_hit = spatial_query
                .cast_ray(right_orig, ray_dir, max_spec_dist, solid, &filter)
                .map(|h| right_orig + *ray_dir * h.distance);
            let center_hit = spatial_query
                .cast_ray(center_orig, ray_dir, max_spec_dist, solid, &filter)
                .map(|h| center_orig + *ray_dir * h.distance);

            let has_contact = left_hit.is_some() || right_hit.is_some() || center_hit.is_some();

            let ground_pos = if let Some(ch) = center_hit {
                ch
            } else if let (Some(lh), Some(rh)) = (left_hit, right_hit) {
                (lh + rh) * 0.5
            } else if let Some(lh) = left_hit {
                lh
            } else if let Some(rh) = right_hit {
                rh
            } else {
                front_center + ray_dir_vec * drive_state.suspension_rest
            };

            spec_steps.push(SpeculativeStepData {
                predicted_position: pred_p,
                predicted_ground_pos: ground_pos,
                predicted_rotation: pred_q,
                left_hit_point: left_hit,
                right_hit_point: right_hit,
                center_hit_point: center_hit,
                left_ray_origin: left_orig,
                right_ray_origin: right_orig,
                center_ray_origin: center_orig,
                has_ground_contact: has_contact,
            });
        }

        if let Some(mut spec_data) = speculative_opt {
            spec_data.steps = spec_steps;
        } else {
            commands
                .entity(car_entity)
                .insert(CarSpeculativeContactData { steps: spec_steps });
        }
    }
}
