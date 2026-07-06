pub mod camera_follow;
pub mod keybinds_control;
pub mod rk4_prediction;
pub mod spawn_car;
pub mod speedometer_ui;

pub use rk4_prediction::{
    update_speculative_contacts_system, CarSpeculativeContactData, SpeculativeStepData,
};

use crate::plugins::cars_driving::driving_plugin::{
    camera_follow::camera_follows_car, spawn_car::Car,
};
use avian3d::prelude::{
    AngularInertia, AngularVelocity, CenterOfMass, ComputeMassProperties3d, LinearVelocity, Mass,
    MassPropertiesExt, PhysicsLayer, SpatialQuery, SpatialQueryFilter,
};
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use {keybinds_control::keybinds_control_car, speedometer_ui::speedometer_ui};

// Hard caps for the grounded hover controller. These are what make violent launches
// impossible by construction: no matter how bad a frame of ray data is, the car's
// vertical velocity can only change by MAX_VERTICAL_ACCEL * dt.
const MAX_LIFT_SPEED: f32 = 8.0; // m/s of ride-height correction
const MAX_VERTICAL_ACCEL: f32 = 50.0; // ~5G vertical acceleration budget
const MAX_TILT_RATE: f32 = 2.0; // rad/s of pitch/roll correction
/// Time constant for smoothing the per-wheel ground heights the controller chases.
/// Bumpy terrain must read as gentle undulation, not per-frame steps.
const WHEEL_HEIGHT_SMOOTH: f32 = 0.15;
/// Ride-height errors smaller than this are ignored (bump rejection).
const HEIGHT_DEADBAND: f32 = 0.03;
/// Tilt errors smaller than this (radians) are ignored (bump rejection).
const TILT_DEADBAND: f32 = 0.03;
/// Input averaging window (also used for expiring stale inputs on uncontrolled cars).
const INPUT_WINDOW: f32 = 0.060;

pub struct DrivingPlugin<S: States> {
    pub state: S,
}

impl<S: States> Plugin for DrivingPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, configure_gizmo_depth);
        // World-wide car physics & visuals run in all states so cars stay grounded and gizmos show
        app.add_systems(
            Update,
            (
                update_wheel_contact_normals,
                update_speculative_contacts_system,
                apply_car_steering_and_drive,
                draw_car_gizmos,
                cap_car_velocities,
                update_vehicle_physics_from_tuning,
                spawn_car::init_cars_system,
            )
                .chain(),
        );
        // Cosmetic wheels are placed late in the frame: after Avian's FixedPostUpdate
        // writeback has produced the car's final Transform, but before PostUpdate transform
        // propagation, so the wheels' child meshes render in the correct place this frame.
        app.add_systems(
            PostUpdate,
            (init_cosmetic_wheels_system, update_cosmetic_wheels)
                .chain()
                .before(bevy::transform::TransformSystems::Propagate),
        );
        // Player control & camera & UI systems only run when driving a car
        app.add_systems(
            Update,
            (
                camera_follows_car,
                keybinds_control_car,
            )
                .chain()
                .run_if(in_state(self.state.clone())),
        );
        app.add_systems(
            EguiPrimaryContextPass,
            (speedometer_ui,).run_if(in_state(self.state.clone())),
        );
    }
}

pub fn configure_gizmo_depth(mut gizmo_store: ResMut<GizmoConfigStore>) {
    let (config, _) = gizmo_store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;
}

#[derive(PhysicsLayer, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamePhysicsLayer {
    #[default]
    Map,
    Car,
    Wheel,
}

#[derive(Clone, Debug)]
pub struct WheelContactData {
    pub ray_distances: [f32; 9],
    // Low-passed ray distances (~0.06s) that the hover controller reads; raw
    // distances stay available for gizmos so seam artifacts remain visible.
    pub smoothed_distances: [f32; 9],
    pub hit_points: [Vec3; 9],
    pub ray_origins: [Vec3; 9],
    pub contact_normal: Vec3,
    pub hits_count: u8,
}

impl Default for WheelContactData {
    fn default() -> Self {
        Self {
            ray_distances: [f32::MAX; 9],
            smoothed_distances: [f32::MAX; 9],
            hit_points: [Vec3::ZERO; 9],
            ray_origins: [Vec3::ZERO; 9],
            contact_normal: Vec3::Y,
            hits_count: 0,
        }
    }
}

#[derive(Component, Clone, Debug, Default)]
pub struct CarWheelsContactData {
    // 0: FL, 1: FR, 2: RL, 3: RR
    pub wheels: [WheelContactData; 4],
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Drive {
    pub entity: Entity,
    pub accelerate: f32, // 0.0 ..= 1.0
    pub brake: f32,      // 0.0 ..= 1.0
    pub steer: f32,      // -1.0 ..= 1.0
}

#[derive(Resource, Default)]
pub struct SimState {
    pub time_elapsed: f32,
    pub spawned: bool,
    pub is_sim: bool,
}

#[derive(Component, Clone)]
pub struct CarDriveState {
    pub history: Vec<(f32, Drive)>,
    pub current_steer_integrated: f32,
    pub avg_accelerate: f32,
    pub avg_brake: f32,
    pub avg_steer: f32,
    pub is_reverse: bool,

    // Spawn position for reset functionality
    pub spawn_position: Option<Vec3>,

    // Ride height model: rays start just above the car's bottom bed and go down
    // max_ray_length; the car hovers at rest_length_pct% of that length.
    pub max_ray_length: f32,
    pub rest_length_pct: f32,
    /// Derived each frame: max_ray_length * rest_length_pct.
    pub suspension_rest: f32,
    /// Derived each frame: rays farther than this count as "no traction".
    pub traction_loss_threshold: f32,
    /// Mean engaged ray distance across grounded wheels (for UI / wheel placement).
    pub avg_suspension_height: f32,
    /// Low-passed per-wheel ground heights the hover controller chases (NaN = unset).
    pub smoothed_wheel_height: [f32; 4],

    // Hover controller response times & grip.
    pub height_response: f32,
    pub tilt_response: f32,
    pub grip: f32,

    pub ray_grid_width_frac: f32,
    pub ray_grid_length_frac: f32,
    pub ray_start_y_offset: f32,

    pub car_mass: f32,

    pub car_half_width: f32,
    pub car_half_length: f32,
    pub car_half_height: f32,

    /// Nominal wheel radius (engine RPM calc + cosmetic wheel fallback until measured).
    pub wheel_radius: f32,

    pub car_max_speed: f32,

    // Hand-simulated engine and gearbox parameters
    pub horsepower: f32,
    pub current_gear: usize,
    pub engine_rpm: f32,
}

impl Default for CarDriveState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            current_steer_integrated: 0.0,
            avg_accelerate: 0.0,
            avg_brake: 0.0,
            avg_steer: 0.0,
            is_reverse: false,
            spawn_position: None,

            max_ray_length: 1.35,
            rest_length_pct: 65.0,
            suspension_rest: 0.69,
            traction_loss_threshold: 1.15,
            avg_suspension_height: 0.0,
            smoothed_wheel_height: [f32::NAN; 4],

            height_response: 0.12,
            tilt_response: 0.18,
            grip: 4.5,

            ray_grid_width_frac: 0.8,
            ray_grid_length_frac: 0.75,
            ray_start_y_offset: 0.0, // Set dynamically after load

            car_mass: 1200.0,

            car_half_width: 0.9,
            car_half_length: 1.52,
            car_half_height: 0.5,

            wheel_radius: 0.55,

            car_max_speed: 140.0,

            horsepower: 80.0,
            current_gear: 1,
            engine_rpm: 800.0,
        }
    }
}

pub fn cap_car_velocities(
    mut q_car: Query<(&mut LinearVelocity, &mut AngularVelocity, &CarDriveState), With<Car>>,
) {
    for (mut lin_vel, mut ang_vel, drive_state) in q_car.iter_mut() {
        // Max speed: dynamic based on slider in km/h -> m/s
        let max_speed = drive_state.car_max_speed / 3.6;
        let speed = lin_vel.0.length();
        if speed > max_speed {
            lin_vel.0 = lin_vel.0.normalize_or_zero() * max_speed;
        }

        // Max rotational speed: 720 deg/s = 12.566 rad/s
        let max_ang_speed = 720.0f32.to_radians();
        let ang_speed = ang_vel.0.length();
        if ang_speed > max_ang_speed {
            ang_vel.0 = ang_vel.0.normalize_or_zero() * max_ang_speed;
        }
    }
}

pub fn car_drive_observer(
    trigger: On<Drive>,
    mut query: Query<&mut CarDriveState>,
    time: Res<Time>,
) {
    let car_entity = trigger.event_target();
    let drive_input = trigger.event().clone();

    let Ok(mut drive_state) = query.get_mut(car_entity) else {
        return;
    };

    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }

    let current_time = time.elapsed_secs();

    // 1. Accumulate drive inputs and average over the input window
    drive_state.history.push((current_time, drive_input));
    let threshold = current_time - INPUT_WINDOW;
    drive_state.history.retain(|(t, _)| *t >= threshold);

    let mut sum_accel = 0.0;
    let mut sum_brake = 0.0;
    let mut sum_steer = 0.0;
    for (_, d) in &drive_state.history {
        sum_accel += d.accelerate;
        sum_brake += d.brake;
        sum_steer += d.steer;
    }
    let count = drive_state.history.len() as f32;
    if count > 0.0 {
        drive_state.avg_accelerate = sum_accel / count;
        drive_state.avg_brake = sum_brake / count;
        drive_state.avg_steer = sum_steer / count;
    } else {
        drive_state.avg_accelerate = 0.0;
        drive_state.avg_brake = 0.0;
        drive_state.avg_steer = 0.0;
    }

    // 2. Integrate and shrink steering
    let steer_rate = 4.0;
    let shrink_rate = 5.0;
    let target_steer = drive_state.avg_steer;
    if target_steer.abs() > 0.01 {
        drive_state.current_steer_integrated += target_steer * steer_rate * dt;
    } else {
        let shrink = shrink_rate * dt;
        if drive_state.current_steer_integrated > 0.0 {
            drive_state.current_steer_integrated =
                (drive_state.current_steer_integrated - shrink).max(0.0);
        } else if drive_state.current_steer_integrated < 0.0 {
            drive_state.current_steer_integrated =
                (drive_state.current_steer_integrated + shrink).min(0.0);
        }
    }
    drive_state.current_steer_integrated = drive_state.current_steer_integrated.clamp(-1.0, 1.0);
}

pub fn update_vehicle_physics_from_tuning(
    q_car: Query<(Entity, &CarDriveState), Changed<CarDriveState>>,
    mut q_body: Query<(&mut Mass, &mut AngularInertia, &mut CenterOfMass), With<Car>>,
) {
    for (car_entity, drive_state) in q_car.iter() {
        if let Ok((mut body_mass, mut body_inertia, mut body_center)) = q_body.get_mut(car_entity) {
            let volume = (drive_state.car_half_width * 2.0)
                * (drive_state.car_half_height * 2.0)
                * (drive_state.car_half_length * 2.0);
            let shape = Cuboid::new(
                drive_state.car_half_width * 2.0,
                drive_state.car_half_height * 2.0,
                drive_state.car_half_length * 2.0,
            );

            let mprops = shape
                .mass_properties(drive_state.car_mass / volume)
                .to_bundle();
            *body_mass = mprops.mass;
            *body_inertia = mprops.angular_inertia;
            *body_center = mprops.center_of_mass;
        }
    }
}

/// Arcade "hover controller" car physics.
///
/// The car is a dynamic rigid body, but while grounded (>= 2 virtual wheels with valid ray
/// contact) its *velocities* are steered directly instead of applying spring forces:
/// - vertical velocity is nudged toward closing the ride-height error, with a hard
///   acceleration cap so bad ray data physically cannot launch the car;
/// - pitch/roll angular velocity is replaced by a clamped correction toward the ground
///   plane fitted from the four wheel heights (mesh normals are never read — the map
///   tiles have vertical skirts that make them useless); yaw stays player-controlled;
/// - planar velocity gets engine/brake/drag accelerations and lateral-slip decay (grip).
///
/// With < 2 wheels grounded the body is left entirely to Avian (ballistic flight).
pub fn apply_car_steering_and_drive(
    mut q_car: Query<
        (
            &Transform,
            &mut CarDriveState,
            &CarWheelsContactData,
            Option<&CarSpeculativeContactData>,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<Car>,
    >,
    time: Res<Time>,
    sim_state: Option<Res<SimState>>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }
    let is_sim = sim_state.map(|s| s.is_sim).unwrap_or(false);

    for (car_transform, mut drive_state, contact_data, speculative_data, mut lin_vel, mut ang_vel) in
        q_car.iter_mut()
    {
        drive_state.suspension_rest =
            drive_state.max_ray_length * (drive_state.rest_length_pct / 100.0);
        drive_state.traction_loss_threshold = drive_state.max_ray_length;
        let rest = drive_state.suspension_rest.max(0.01);

        // Expire stale inputs: `car_drive_observer` only runs when Drive events arrive,
        // so an uncontrolled car would otherwise keep its last throttle/steer averages
        // forever and drive away on its own. (Skipped in sim mode, where the sim binary
        // writes the averages directly.)
        if !is_sim {
            let input_threshold = time.elapsed_secs() - INPUT_WINDOW;
            drive_state.history.retain(|(t, _)| *t >= input_threshold);
            if drive_state.history.is_empty() {
                drive_state.avg_accelerate = 0.0;
                drive_state.avg_brake = 0.0;
                drive_state.avg_steer = 0.0;
                let shrink = 5.0 * dt;
                if drive_state.current_steer_integrated > 0.0 {
                    drive_state.current_steer_integrated =
                        (drive_state.current_steer_integrated - shrink).max(0.0);
                } else if drive_state.current_steer_integrated < 0.0 {
                    drive_state.current_steer_integrated =
                        (drive_state.current_steer_integrated + shrink).min(0.0);
                }
            }
        }

        // --- Per-wheel ground heights from the smoothed, median-filtered rays ---
        let mut wheel_height = [rest; 4];
        let mut grounded_wheels = 0;
        for (i, wheel) in contact_data.wheels.iter().enumerate() {
            let mut sum = 0.0f32;
            let mut n = 0u32;
            for &d in &wheel.smoothed_distances {
                if d != f32::MAX && d <= drive_state.traction_loss_threshold {
                    sum += d;
                    n += 1;
                }
            }
            if n > 0 {
                wheel_height[i] = sum / n as f32;
                grounded_wheels += 1;
            }
        }

        // Temporally smooth the per-wheel heights the controller chases. Raw heights on
        // bumpy terrain made the tilt controller roll-oscillate left/right, and the grip
        // term then bled the oscillating "lateral" velocity off as a massive slowdown.
        let height_alpha = (dt / WHEEL_HEIGHT_SMOOTH).min(1.0);
        for i in 0..4 {
            let prev = drive_state.smoothed_wheel_height[i];
            drive_state.smoothed_wheel_height[i] = if prev.is_nan() {
                wheel_height[i]
            } else {
                prev + (wheel_height[i] - prev) * height_alpha
            };
        }
        let wheel_height = drive_state.smoothed_wheel_height;

        let car_forward = car_transform.rotation * Vec3::NEG_Z;
        let forward_speed = lin_vel.0.dot(car_forward);

        // --- Engine & Gearbox Simulation (RPM/gear UI + drive force magnitude) ---
        let gear_ratios = [3.5f32, 2.1f32, 1.4f32, 1.0f32, 0.8f32];
        let final_drive = 3.7f32;
        let gear_ratio = if drive_state.is_reverse {
            3.4f32
        } else {
            gear_ratios[(drive_state.current_gear - 1).min(4)]
        };

        let physical_rpm = (forward_speed.abs() * 60.0f32 * final_drive * gear_ratio)
            / (2.0f32 * std::f32::consts::PI * drive_state.wheel_radius);

        let mut target_rpm = physical_rpm;
        if drive_state.avg_accelerate > 0.05f32 {
            let throttle_rpm = 800.0f32 + drive_state.avg_accelerate * 2200.0f32;
            target_rpm = target_rpm.max(throttle_rpm);
            drive_state.engine_rpm = drive_state.engine_rpm.lerp(target_rpm, 10.0 * dt);
        } else {
            // Coasting: engine RPM decays towards physical_rpm or idle (800.0)
            let decay_target = physical_rpm.max(800.0f32);
            if drive_state.engine_rpm > decay_target {
                drive_state.engine_rpm = (drive_state.engine_rpm - 2500.0 * dt).max(decay_target);
            } else {
                drive_state.engine_rpm = drive_state.engine_rpm.lerp(decay_target, 10.0 * dt);
            }
        }
        drive_state.engine_rpm = drive_state.engine_rpm.min(6500.0);

        // Automatic gear shifting
        if !drive_state.is_reverse {
            if drive_state.engine_rpm > 5500.0f32 && drive_state.current_gear < 5 {
                drive_state.current_gear += 1;
            } else if drive_state.engine_rpm < 1800.0f32 && drive_state.current_gear > 1 {
                drive_state.current_gear -= 1;
            }
        } else {
            drive_state.current_gear = 1;
        }

        // If we don't have traction on at least 2 wheels, apply no driving-related
        // velocity control at all: the car coasts/flies under plain physics rules.
        if grounded_wheels < 2 {
            drive_state.avg_suspension_height = 0.0;
            continue;
        }

        let mut height_sum = 0.0f32;
        for &h in &wheel_height {
            height_sum += h;
        }
        // Ungrounded wheels default to `rest`, which is a fine neutral contribution.
        let avg_height = height_sum / 4.0f32;
        drive_state.avg_suspension_height = avg_height;

        // --- Vertical: ride height control (rate-limited, launch-proof) ---
        // Anticipatory slope adjustment from speculative future rays:
        let speed_kmh = lin_vel.0.length() * 3.6;
        let speed_weight = ((speed_kmh - 40.0) / 40.0).clamp(0.0, 1.0);
        let mut anticipatory_height_bias = 0.0f32;

        if speed_weight > 0.0 {
            if let Some(spec) = speculative_data {
                let mut sum_future_rel_y = 0.0f32;
                let mut valid_steps = 0f32;
                for step in &spec.steps {
                    if step.has_ground_contact {
                        let rel_y = step.predicted_ground_pos.y - car_transform.translation.y;
                        sum_future_rel_y += rel_y;
                        valid_steps += 1.0;
                    }
                }
                if valid_steps > 0.0 {
                    let avg_future_rel_y = sum_future_rel_y / valid_steps;
                    anticipatory_height_bias = (avg_future_rel_y * 0.35 * speed_weight).clamp(-0.5, 0.5);
                }
            }
        }

        // Small errors are ignored entirely so terrain ripple doesn't pump the car.
        // "Too low" reacts twice as fast as "too high": clipping a bump peak with the
        // chassis collider is far worse than briefly floating a little high.
        let height_error = rest - avg_height + anticipatory_height_bias; // positive => car too low => go up
        let effective_error =
            (height_error.abs() - HEIGHT_DEADBAND).max(0.0) * height_error.signum();
        let response = if height_error > 0.0 {
            drive_state.height_response.max(0.02) * 0.5
        } else {
            drive_state.height_response.max(0.02)
        };
        let target_vy = (effective_error / response).clamp(-MAX_LIFT_SPEED, MAX_LIFT_SPEED);
        let max_dv = MAX_VERTICAL_ACCEL * dt;
        lin_vel.0.y += (target_vy - lin_vel.0.y).clamp(-max_dv, max_dv);

        // --- Attitude: tilt toward the wheel-height ground plane, yaw from steering ---
        let track = (drive_state.car_half_width * 2.0).max(0.5);
        let wheelbase = (drive_state.car_half_length * 2.0).max(0.5);
        let h_left = (wheel_height[0] + wheel_height[2]) * 0.5;
        let h_right = (wheel_height[1] + wheel_height[3]) * 0.5;
        let h_front = (wheel_height[0] + wheel_height[1]) * 0.5;
        let h_rear = (wheel_height[2] + wheel_height[3]) * 0.5;
        let ground_normal_local = Vec3::new(
            (h_right - h_left) / track,
            1.0,
            (h_rear - h_front) / wheelbase,
        )
        .normalize_or_zero();
        let target_up = (car_transform.rotation * ground_normal_local).normalize_or_zero();
        let up = car_transform.rotation * Vec3::Y;

        let mut tilt_correction = Vec3::ZERO;
        let tilt_axis = up.cross(target_up);
        if tilt_axis.length_squared() > 1e-8 {
            let tilt_angle = up.angle_between(target_up);
            // Deadband: small terrain ripple must not rock the chassis.
            let effective_angle = (tilt_angle - TILT_DEADBAND).max(0.0);
            tilt_correction = (tilt_axis.normalize()
                * (effective_angle / drive_state.tilt_response.max(0.02)))
            .clamp_length_max(MAX_TILT_RATE);
        }

        // Kinematic (bicycle-model) steering: yaw rate follows the steering angle scaled
        // by speed, so there is no yaw at standstill and reverse flips it naturally.
        let max_steer = 1.2f32 / (1.0f32 + 0.3f32 * forward_speed.abs());
        let steer_angle = drive_state.current_steer_integrated * max_steer;
        let target_yaw_rate = -steer_angle * forward_speed / wheelbase;
        let current_yaw_rate = ang_vel.0.dot(up);
        let new_yaw_rate =
            current_yaw_rate + (target_yaw_rate - current_yaw_rate) * (dt / 0.1).min(1.0);

        // Yaw stays player-controlled; everything else becomes the tilt correction, so
        // collision-induced spins are damped out while grounded (arcade self-righting).
        ang_vel.0 = up * new_yaw_rate + tilt_correction;

        // --- Planar velocity: drive, brake, drag, grip ---
        let mut v_xz = Vec3::new(lin_vel.0.x, 0.0, lin_vel.0.z);
        let fwd_xz = Vec3::new(car_forward.x, 0.0, car_forward.z).normalize_or_zero();

        // Drive: engine power -> acceleration along the car's planar forward.
        if drive_state.avg_accelerate > 0.0f32 {
            let power_watts = drive_state.horsepower * 745.7f32;
            let speed_for_power = forward_speed.abs().max(2.0f32);
            let max_speed_mps = if drive_state.is_reverse {
                40.0f32 / 3.6f32
            } else {
                drive_state.car_max_speed / 3.6f32
            };
            let speed_ratio = forward_speed.abs() / max_speed_mps;
            let force_scale = (1.0f32 - speed_ratio).max(0.0f32);
            let mut drive_force_mag =
                (power_watts / speed_for_power) * drive_state.avg_accelerate * force_scale;
            // Engine force limit: 1G of engine traction limit
            drive_force_mag = drive_force_mag.min(drive_state.car_mass * 9.81f32);

            let dir = if drive_state.is_reverse { -fwd_xz } else { fwd_xz };
            v_xz += dir * (drive_force_mag / drive_state.car_mass) * dt;
        }

        // Brake: up to ~1.2G of deceleration along the current motion.
        if drive_state.avg_brake > 0.0f32 {
            let decel = drive_state.avg_brake * 11.8f32 * dt;
            v_xz = v_xz.normalize_or_zero() * (v_xz.length() - decel).max(0.0);
        }

        // Aerodynamic drag + rolling resistance.
        let speed_xz = v_xz.length();
        if speed_xz > 0.1f32 {
            let drag_decel = (0.46f32 * speed_xz * speed_xz / drive_state.car_mass) + 0.59f32;
            v_xz = v_xz.normalize_or_zero() * (speed_xz - drag_decel * dt).max(0.0);
        }

        // Grip: decay the lateral (sideways) velocity component so the car travels
        // where it points. Lower grip = more drift.
        let v_forward = fwd_xz * v_xz.dot(fwd_xz);
        let v_lateral = (v_xz - v_forward) * (1.0 - (drive_state.grip * dt).min(1.0));
        let v_xz = v_forward + v_lateral;

        lin_vel.0.x = v_xz.x;
        lin_vel.0.z = v_xz.z;
    }
}

#[derive(Component)]
pub struct CosmeticWheel {
    pub wheel_idx: usize,
    pub parent_car: Entity,
    pub accumulated_rotation: f32,
    /// World-space wheel radius measured from the loaded GLB (None until loaded).
    pub measured_radius: Option<f32>,
}

/// Measures each cosmetic wheel's real rendered radius from its loaded GLB meshes.
/// The wheel model's cylinder axis is +/-Z in Bevy, so the radius lives in the XY plane.
pub fn init_cosmetic_wheels_system(
    mut q_wheels: Query<(Entity, &Transform, &mut CosmeticWheel)>,
    children_query: Query<&Children>,
    mesh_query: Query<&Mesh3d>,
    global_transform_query: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    for (root_entity, root_transform, mut wheel) in q_wheels.iter_mut() {
        if wheel.measured_radius.is_some() {
            continue;
        }
        let Ok(children) = children_query.get(root_entity) else {
            continue;
        };

        let mut mesh_entities = Vec::new();
        let mut queue: Vec<Entity> = children.to_vec();
        while let Some(ent) = queue.pop() {
            if let Ok(m) = mesh_query.get(ent) {
                mesh_entities.push((ent, m.0.clone()));
            }
            if let Ok(kids) = children_query.get(ent) {
                queue.extend(kids.iter());
            }
        }
        if mesh_entities.is_empty()
            || mesh_entities.iter().any(|(_, h)| meshes.get(h).is_none())
        {
            continue;
        }

        let Ok(root_gt) = global_transform_query.get(root_entity) else {
            continue;
        };
        let root_inv = root_gt.to_matrix().inverse();

        let mut max_radius = 0.0f32;
        for (ent, handle) in &mesh_entities {
            let Ok(mesh_gt) = global_transform_query.get(*ent) else {
                continue;
            };
            let Some(mesh) = meshes.get(handle) else {
                continue;
            };
            if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                for pos in positions {
                    let world_pos = mesh_gt.transform_point(Vec3::from(*pos));
                    let local = root_inv.transform_point3(world_pos);
                    max_radius = max_radius.max(Vec2::new(local.x, local.y).length());
                }
            }
        }

        if max_radius > 0.0 {
            wheel.measured_radius = Some(max_radius * root_transform.scale.x);
        }
    }
}

pub fn update_cosmetic_wheels(
    mut commands: Commands,
    mut q_wheels: Query<(Entity, &mut Transform, &mut CosmeticWheel)>,
    q_cars: Query<
        (&Transform, &CarDriveState, &CarWheelsContactData, &LinearVelocity),
        (With<Car>, Without<CosmeticWheel>),
    >,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (wheel_entity, mut wheel_transform, mut cosmetic_wheel) in q_wheels.iter_mut() {
        let Ok((car_transform, drive_state, contact_data, lin_vel)) =
            q_cars.get(cosmetic_wheel.parent_car)
        else {
            // Parent car is gone; the wheel has no reason to exist.
            if let Ok(mut e) = commands.get_entity(wheel_entity) {
                e.despawn();
            }
            continue;
        };

        let wheel_data = &contact_data.wheels[cosmetic_wheel.wheel_idx];

        let is_front = cosmetic_wheel.wheel_idx == 0 || cosmetic_wheel.wheel_idx == 1;
        let is_right = cosmetic_wheel.wheel_idx == 1 || cosmetic_wheel.wheel_idx == 3;

        let radius = cosmetic_wheel
            .measured_radius
            .unwrap_or(drive_state.wheel_radius * 0.6);

        let x_offset = if is_right {
            drive_state.car_half_width + 0.1
        } else {
            -drive_state.car_half_width - 0.1
        };
        let z_offset = if is_front {
            -drive_state.car_half_length
        } else {
            drive_state.car_half_length
        };

        let mut engaged_dist = 0.0;
        let mut engaged_count = 0;
        for &dist in &wheel_data.smoothed_distances {
            if dist != f32::MAX && dist <= drive_state.traction_loss_threshold {
                engaged_dist += dist;
                engaged_count += 1;
            }
        }

        // Grounded: hub sits at ground contact + radius. Airborne: hang slightly
        // below rest instead of stretching down the whole ray length.
        let dist = if engaged_count > 0 {
            engaged_dist / engaged_count as f32
        } else {
            (drive_state.suspension_rest + 0.1).min(drive_state.max_ray_length)
        };
        let y_offset = drive_state.ray_start_y_offset - dist + radius;

        let local_pos = Vec3::new(x_offset, y_offset, z_offset);
        wheel_transform.translation = car_transform.transform_point(local_pos);

        let car_forward = car_transform.rotation * Vec3::NEG_Z;
        let forward_speed = lin_vel.0.dot(car_forward);
        // Rolling forward (car moving -Z) means the wheel top moves -Z: a negative
        // rotation about the car-local X axle.
        cosmetic_wheel.accumulated_rotation -= (forward_speed / radius.max(0.05)) * dt;

        let steer_angle = if is_front {
            let max_steer = 1.2f32 / (1.0f32 + 0.3f32 * forward_speed.abs());
            drive_state.current_steer_integrated * max_steer
        } else {
            0.0
        };

        let base_y_rot = if is_right {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };

        // Order matters: the base yaw orients the GLB's cylinder axis (+/-Z) onto the
        // car-local X axle FIRST; the spin then rolls about that axle; steering yaws on
        // top. Composing the spin before the base yaw is what made wheels tumble around
        // the car's forward axis.
        wheel_transform.rotation = car_transform.rotation
            * Quat::from_rotation_y(-steer_angle)
            * Quat::from_rotation_x(cosmetic_wheel.accumulated_rotation)
            * Quat::from_rotation_y(base_y_rot);
    }
}

pub fn update_wheel_contact_normals(
    spatial_query: SpatialQuery,
    mut q_cars: Query<
        (
            Entity,
            &Transform,
            &CarDriveState,
            &mut CarWheelsContactData,
        ),
        With<Car>,
    >,
    time: Res<Time>,
) {
    for (car_entity, car_transform, drive_state, mut contact_data) in q_cars.iter_mut() {
        for wheel_idx in 0..4 {
            let (x_min, x_max, z_min, z_max) = match wheel_idx {
                0 => (
                    -drive_state.car_half_width,
                    0.0f32,
                    -drive_state.car_half_length,
                    0.0f32,
                ), // FL
                1 => (
                    0.0f32,
                    drive_state.car_half_width,
                    -drive_state.car_half_length,
                    0.0f32,
                ), // FR
                2 => (
                    -drive_state.car_half_width,
                    0.0f32,
                    0.0f32,
                    drive_state.car_half_length,
                ), // RL
                _ => (
                    0.0f32,
                    drive_state.car_half_width,
                    0.0f32,
                    drive_state.car_half_length,
                ), // RR
            };

            let mut local_corners = [Vec3::ZERO; 9];
            let grid_size = 3;
            for x in 0..grid_size {
                for z in 0..grid_size {
                    let frac_x = x as f32 / (grid_size - 1) as f32;
                    let frac_z = z as f32 / (grid_size - 1) as f32;
                    let local_x = x_min + frac_x * (x_max - x_min) * drive_state.ray_grid_width_frac;
                    let local_z = z_min + frac_z * (z_max - z_min) * drive_state.ray_grid_length_frac;
                    let local_y = drive_state.ray_start_y_offset;
                    local_corners[x * grid_size + z] = Vec3::new(local_x, local_y, local_z);
                }
            }

            // Transform corners to world space
            let mut world_origins = [Vec3::ZERO; 9];
            for i in 0..9 {
                world_origins[i] = car_transform.transform_point(local_corners[i]);
                contact_data.wheels[wheel_idx].ray_origins[i] = world_origins[i];
            }

            let ray_dir_vec = car_transform.rotation * Vec3::NEG_Y;
            let ray_dir = Dir3::new(ray_dir_vec).unwrap_or(Dir3::NEG_Y);
            let max_dist = drive_state.max_ray_length;
            let solid = true;
            let filter = SpatialQueryFilter::from_mask([GamePhysicsLayer::Map])
                .with_excluded_entities([car_entity]);

            let mut distances = [f32::MAX; 9];
            let mut hit_points = [Vec3::ZERO; 9];

            for i in 0..9 {
                if let Some(hit) =
                    spatial_query.cast_ray(world_origins[i], ray_dir, max_dist, solid, &filter)
                {
                    distances[i] = hit.distance;
                    hit_points[i] = world_origins[i] + *ray_dir * hit.distance;
                }
            }

            // Median-anchored validity window
            let mut sorted = [0.0f32; 9];
            let mut hit_total = 0usize;
            for &d in &distances {
                if d != f32::MAX {
                    sorted[hit_total] = d;
                    hit_total += 1;
                }
            }
            let mut hits_count = 0;
            if hit_total > 0 {
                sorted[..hit_total].sort_unstable_by(|a, b| a.total_cmp(b));
                let median = sorted[hit_total / 2];
                for i in 0..9 {
                    if distances[i] != f32::MAX && (distances[i] - median).abs() <= 0.25 {
                        hits_count += 1;
                    } else {
                        distances[i] = f32::MAX;
                        hit_points[i] = Vec3::ZERO;
                    }
                }
            }

            let w_contact = &mut contact_data.wheels[wheel_idx];

            let alpha = (time.delta_secs() / 0.06f32).min(1.0f32);
            for i in 0..9 {
                let raw = distances[i];
                let prev = w_contact.smoothed_distances[i];
                w_contact.smoothed_distances[i] = if raw == f32::MAX {
                    f32::MAX
                } else if prev == f32::MAX {
                    raw
                } else {
                    prev + (raw - prev) * alpha
                };
            }

            w_contact.ray_distances = distances;
            w_contact.hit_points = hit_points;
            w_contact.hits_count = hits_count;

            w_contact.contact_normal = Vec3::Y;
        }
    }
}

pub fn draw_car_gizmos(
    mut gizmos: Gizmos,
    q_car: Query<
        (
            &Transform,
            &CarDriveState,
            &CarWheelsContactData,
            Option<&CarSpeculativeContactData>,
        ),
        With<Car>,
    >,
) {
    for (car_transform, drive_state, contact_data, speculative_data) in q_car.iter() {
        // Draw orange lines for 9 rays for each of the 4 virtual wheels
        let ray_color = Color::srgb(1.0, 0.5, 0.0);
        let star_color = Color::srgb(0.0, 0.0, 1.0);
        let sphere_color = Color::srgb(0.0, 0.0, 1.0);
        let local_down = car_transform.rotation * Vec3::NEG_Y;
        let max_len = drive_state.max_ray_length;

        for wheel_idx in 0..4 {
            let wheel_contact = &contact_data.wheels[wheel_idx];

            for i in 0..9 {
                let start = wheel_contact.ray_origins[i];
                let end = if wheel_contact.ray_distances[i] > max_len {
                    start + local_down * max_len
                } else {
                    wheel_contact.hit_points[i]
                };
                gizmos.line(start, end, ray_color);

                // If hit ground and engaged, draw blue star and sphere
                if wheel_contact.ray_distances[i] <= max_len {
                    let hit = wheel_contact.hit_points[i];
                    // Draw 3 perpendicular lines (star) of total span 0.3 (so each arm is 0.15)
                    gizmos.line(hit - Vec3::X * 0.15, hit + Vec3::X * 0.15, star_color);
                    gizmos.line(hit - Vec3::Y * 0.15, hit + Vec3::Y * 0.15, star_color);
                    gizmos.line(hit - Vec3::Z * 0.15, hit + Vec3::Z * 0.15, star_color);

                    // Draw small sphere
                    let sphere = Sphere::new(0.05);
                    gizmos.primitive_3d(&sphere, Isometry3d::from_translation(hit), sphere_color);
                }
            }

            // Draw plane defining the plane segment (green for contact, red for no contact)
            let mut centroid = Vec3::ZERO;
            let mut engaged_count = 0;
            for i in 0..9 {
                if wheel_contact.ray_distances[i] <= max_len {
                    centroid += wheel_contact.hit_points[i];
                    engaged_count += 1;
                }
            }

            let has_contact = engaged_count > 0;
            let plane_center = if has_contact {
                centroid / engaged_count as f32
            } else {
                wheel_contact.ray_origins.iter().sum::<Vec3>() / 9.0f32 + local_down * max_len
            };

            let box_color = if has_contact {
                Color::srgb(0.0, 1.0, 0.0) // Green contact marker
            } else {
                Color::srgb(1.0, 0.0, 0.0) // Red no-contact marker
            };

            // Rotated to the contact normal
            let plane_rotation = Quat::from_rotation_arc(Vec3::Y, wheel_contact.contact_normal);

            // Draw a nice plane segment using flat cuboid
            let cuboid = Cuboid::new(0.5, 0.01, 0.5);
            gizmos.primitive_3d(
                &cuboid,
                Isometry3d::new(plane_center, plane_rotation),
                box_color,
            );
        }

        // Speculative Rays & RK4 Trajectory Gizmos (attached to car front ground point)
        if let Some(spec) = speculative_data {
            let trajectory_yellow = Color::srgb(1.0, 1.0, 0.0);
            let speculative_blue = Color::srgb(0.4, 0.7, 1.0);
            let star_blue = Color::srgb(0.2, 0.8, 1.0);

            // Front ground projection point of the car
            let car_fwd = car_transform.rotation * Vec3::NEG_Z;
            let car_front = car_transform.translation + car_fwd * drive_state.car_half_length;
            let current_front_ground = car_front + local_down * drive_state.suspension_rest;

            let mut path_points = vec![current_front_ground];
            for step in &spec.steps {
                path_points.push(step.predicted_ground_pos);
            }

            // Draw yellow gizmo line connecting front ground point to predicted future ground positions
            for window in path_points.windows(2) {
                gizmos.line(window[0], window[1], trajectory_yellow);
            }

            // Draw light blue speculative rays for left, center, right at each of the 8 future steps
            let spec_max_len = drive_state.max_ray_length * 1.5;
            for step in &spec.steps {
                let spec_down = step.predicted_rotation * Vec3::NEG_Y;

                let draw_ray = |gizmos: &mut Gizmos, orig: Vec3, hit: Option<Vec3>| {
                    let end = hit.unwrap_or(orig + spec_down * spec_max_len);
                    gizmos.line(orig, end, speculative_blue);
                    if let Some(h) = hit {
                        gizmos.line(h - Vec3::X * 0.1, h + Vec3::X * 0.1, star_blue);
                        gizmos.line(h - Vec3::Y * 0.1, h + Vec3::Y * 0.1, star_blue);
                        gizmos.line(h - Vec3::Z * 0.1, h + Vec3::Z * 0.1, star_blue);
                        let sphere = Sphere::new(0.04);
                        gizmos.primitive_3d(&sphere, Isometry3d::from_translation(h), star_blue);
                    }
                };

                draw_ray(&mut gizmos, step.left_ray_origin, step.left_hit_point);
                draw_ray(&mut gizmos, step.right_ray_origin, step.right_hit_point);
                draw_ray(&mut gizmos, step.center_ray_origin, step.center_hit_point);
            }
        }
    }
}
