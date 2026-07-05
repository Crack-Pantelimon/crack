pub mod camera_follow;
pub mod keybinds_control;
pub mod spawn_car;
pub mod speedometer_ui;

use crate::plugins::cars_driving::driving_plugin::{
    camera_follow::camera_follows_car, spawn_car::Car,
};
use avian3d::prelude::{
    AngularInertia, AngularVelocity, CenterOfMass, ComputeMassProperties3d, Forces, LinearVelocity,
    Mass, MassPropertiesExt, PhysicsLayer, ReadRigidBodyForces, SpatialQuery, SpatialQueryFilter,
    WriteRigidBodyForces,
};
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use {keybinds_control::keybinds_control_car, speedometer_ui::speedometer_ui};

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
                apply_car_steering_and_drive,
                draw_car_gizmos,
                cap_car_velocities,
                update_vehicle_physics_from_tuning,
                spawn_car::init_cars_system,
                update_cosmetic_wheels,
            )
                .chain(),
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
    pub hit_points: [Vec3; 9],
    pub ray_origins: [Vec3; 9],
    pub contact_normal: Vec3,
    pub hits_count: u8,
}

impl Default for WheelContactData {
    fn default() -> Self {
        Self {
            ray_distances: [f32::MAX; 9],
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

    // Sliders
    pub suspension_min: f32,
    pub suspension_max: f32,
    pub suspension_rest: f32,
    pub suspension_stiffness: f32,
    pub suspension_damping: f32,
    pub extra_spring_length: f32,
    pub avg_suspension_height: f32,

    pub max_ray_length: f32,
    pub rest_length_pct: f32,
    pub ray_grid_width_frac: f32,
    pub ray_grid_length_frac: f32,
    pub ray_start_y_offset: f32,
    pub traction_loss_threshold: f32,

    pub car_mass: f32,

    pub car_half_width: f32,
    pub car_half_length: f32,
    pub car_half_height: f32,

    pub wheel_radius: f32,
    pub wheel_width: f32,
    pub wheel_y_offset: f32,

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

            suspension_min: 0.1,
            suspension_max: 1.0,
            suspension_rest: 0.3,
            suspension_stiffness: 8.0,
            suspension_damping: 0.8,
            extra_spring_length: 0.2,
            avg_suspension_height: 0.0,

            max_ray_length: 1.15,
            rest_length_pct: 50.0,
            ray_grid_width_frac: 0.8,
            ray_grid_length_frac: 0.75,
            ray_start_y_offset: 0.0, // Set dynamically after load
            traction_loss_threshold: 1.15,

            car_mass: 1200.0,

            car_half_width: 0.9,
            car_half_length: 1.52,
            car_half_height: 0.5,

            wheel_radius: 0.45,
            wheel_width: 0.35,
            wheel_y_offset: 0.25,

            car_max_speed: 140.0,

            horsepower: 150.0,
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

    // 1. Accumulate drive inputs and average over 0.060s
    drive_state.history.push((current_time, drive_input));
    let threshold = current_time - 0.060;
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

pub fn apply_car_steering_and_drive(
    mut q_car: Query<
        (
            &mut Transform,
            &CenterOfMass,
            &mut CarDriveState,
            &mut CarWheelsContactData,
            Forces,
        ),
        With<Car>,
    >,
    time: Res<Time>,
) {
    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }

    for (mut car_transform, body_center, mut drive_state, contact_data, mut car_forces) in
        q_car.iter_mut()
    {
        drive_state.suspension_rest = drive_state.max_ray_length * (drive_state.rest_length_pct / 100.0);
        drive_state.suspension_min = drive_state.suspension_rest * 0.3;
        drive_state.traction_loss_threshold = drive_state.max_ray_length;

    let speed = car_forces.linear_velocity().length();
    let max_steer = 1.2f32 / (1.0f32 + 0.3f32 * speed);
    let steer_angle = drive_state.current_steer_integrated * max_steer;

    let car_forward = car_transform.rotation * Vec3::NEG_Z;
    let car_right = car_transform.rotation * Vec3::X;
    let steer_dir_world =
        car_transform.rotation * Vec3::new(steer_angle.sin(), 0.0, -steer_angle.cos());

    let mut total_linear_force = Vec3::ZERO;
    let mut total_torque = Vec3::ZERO;

    let com_world = car_transform.transform_point(body_center.0);

    // 1. Engine & Gearbox Simulation
    let gear_ratios = [3.5f32, 2.1f32, 1.4f32, 1.0f32, 0.8f32];
    let final_drive = 3.7f32;
    let forward_speed = car_forces.linear_velocity().dot(car_forward);

    // Calculate physical RPM matching the speed
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

    // 2. Drive Force Calculation
    let power_watts = drive_state.horsepower * 745.7f32;
    let speed_for_power = forward_speed.abs().max(2.0f32);
    let mut drive_force_mag = 0.0f32;

    let mut grounded_wheels = 0;
    for wheel in &contact_data.wheels {
        let engaged_rays = wheel
            .ray_distances
            .iter()
            .filter(|&&d| d != f32::MAX && d <= drive_state.traction_loss_threshold)
            .count();
        if engaged_rays > 0 {
            grounded_wheels += 1;
        }
    }

    if drive_state.avg_accelerate > 0.0f32 && grounded_wheels >= 2 {
        let max_speed_mps = if drive_state.is_reverse {
            40.0f32 / 3.6f32
        } else {
            drive_state.car_max_speed / 3.6f32
        };
        let speed_ratio = forward_speed.abs() / max_speed_mps;
        let force_scale = (1.0f32 - speed_ratio).max(0.0f32);
        drive_force_mag =
            (power_watts / speed_for_power) * drive_state.avg_accelerate * force_scale;

        // Engine force limit: 1G of engine traction limit
        let max_engine_force = drive_state.car_mass * 9.81f32 * 1.0f32;
        drive_force_mag = drive_force_mag.min(max_engine_force);
    }

    // Rolling resistance + Aerodynamic Drag (applies always)
    if forward_speed.abs() > 0.1f32 {
        let direction = forward_speed.signum();
        let aero_drag = 0.46f32 * forward_speed * forward_speed;
        let rolling_res = drive_state.car_mass * 9.81f32 * 0.06f32;
        let total_drag = (aero_drag + rolling_res).min(drive_state.car_mass * 9.81f32 * 0.5f32);
        total_linear_force += -car_forward * direction * total_drag;
    }

    // 3. Virtual Wheels Force Accumulation
    let wheel_offsets = [
        // FL
        (
            Vec3::new(
                -drive_state.car_half_width,
                -drive_state.car_half_height,
                -drive_state.car_half_length,
            ),
            true,
            true,
        ),
        // FR
        (
            Vec3::new(
                drive_state.car_half_width + 0.1,
                -drive_state.car_half_height,
                -drive_state.car_half_length,
            ),
            true,
            false,
        ),
        // RL
        (
            Vec3::new(
                -drive_state.car_half_width,
                -drive_state.car_half_height,
                drive_state.car_half_length,
            ),
            false,
            true,
        ),
        // RR
        (
            Vec3::new(
                drive_state.car_half_width,
                -drive_state.car_half_height,
                drive_state.car_half_length,
            ),
            false,
            false,
        ),
    ];

    let mass_per_wheel = drive_state.car_mass / 4.0f32;
    let mut total_engaged_height = 0.0f32;
    let mut engaged_wheels_count = 0;
    let mut avg_normal = Vec3::ZERO;
    let mut max_deficit = 0.0f32;

    for (wheel_idx, (offset, is_front, _is_left)) in wheel_offsets.into_iter().enumerate() {
        let mut adjusted_offset = offset;
        adjusted_offset.y += drive_state.wheel_y_offset;
        let mount_world = car_transform.transform_point(adjusted_offset);

        let velocity_at_wheel = car_forces.linear_velocity()
            + car_forces.angular_velocity().cross(mount_world - com_world);
        let w_contact = &contact_data.wheels[wheel_idx];

        // --- Suspension Engagement & Length Check ---
        let mut sum_dist = 0.0f32;
        let mut engaged_rays = 0;
        for &d in &w_contact.ray_distances {
            if d != f32::MAX && d <= drive_state.traction_loss_threshold {
                sum_dist += d;
                engaged_rays += 1;
            }
        }

        if engaged_rays == 0 {
            // Suspension is fully disengaged, no force is put on the car from this wheel
            continue;
        }

        let avg_length = sum_dist / engaged_rays as f32;
        total_engaged_height += avg_length;
        engaged_wheels_count += 1;
        avg_normal += w_contact.contact_normal;

        // Comfortable length threshold check: (min + rest) / 2
        let threshold = (drive_state.suspension_min + drive_state.suspension_rest) / 2.0f32;
        if avg_length < threshold {
            let deficit = threshold - avg_length;
            if deficit > max_deficit {
                max_deficit = deficit;
            }
        }

        // --- Suspension Force ---
        let gravity_force = mass_per_wheel * 9.81f32;
        let base_stiffness = gravity_force / drive_state.extra_spring_length.max(0.01f32);
        let stiffness = base_stiffness * (drive_state.suspension_stiffness / 8.0f32);

        let displacement =
            (drive_state.suspension_rest + drive_state.extra_spring_length) - avg_length;

        // More aggressive scaling if length is shorter than rest position (but avoid division by zero)
        let scaling = if avg_length < drive_state.suspension_rest {
            let remaining = (avg_length - drive_state.suspension_min).max(0.001f32);
            ((drive_state.suspension_rest - drive_state.suspension_min) / remaining).max(1.0f32)
        } else {
            1.0f32
        };

        let spring_force = stiffness * displacement * scaling;

        let force_dir = w_contact.contact_normal;
        let speed_along_suspension = velocity_at_wheel.dot(force_dir);

        // Critical damping coefficient dynamically computed based on stiffness
        let damping_coef =
            2.0f32 * (stiffness * mass_per_wheel).sqrt() * drive_state.suspension_damping;
        let damping_force = -damping_coef * speed_along_suspension * scaling;

        let mut total_suspension_force = (spring_force + damping_force).max(0.0f32);
        // Suspension force limit: 5G max suspension force per wheel
        let max_susp_force = mass_per_wheel * 9.81f32 * 5.0f32;
        total_suspension_force = total_suspension_force.min(max_susp_force);

        let suspension_force_vec = force_dir * total_suspension_force;

        total_linear_force += suspension_force_vec;
        total_torque += (mount_world - com_world).cross(suspension_force_vec);

        // --- Driving / Traction Force (Front Wheels only) ---
        if is_front && drive_force_mag > 0.0f32 {
            let drive_dir = if drive_state.is_reverse {
                -steer_dir_world
            } else {
                steer_dir_world
            };
            let drive_dir_plane =
                (drive_dir - force_dir * drive_dir.dot(force_dir)).normalize_or_zero();
            let wheel_drive_force = drive_dir_plane * (drive_force_mag / 2.0f32);

            total_linear_force += wheel_drive_force;
            total_torque += (mount_world - com_world).cross(wheel_drive_force);
        }

        // --- Braking Force ---
        if drive_state.avg_brake > 0.0f32 && grounded_wheels >= 2 {
            let forward_dir = if is_front {
                steer_dir_world
            } else {
                car_forward
            };
            let forward_dir_plane =
                (forward_dir - force_dir * forward_dir.dot(force_dir)).normalize_or_zero();
            let speed_forward = velocity_at_wheel.dot(forward_dir_plane);
            let raw_brake_force = -forward_dir_plane
                * speed_forward
                * (drive_state.avg_brake * mass_per_wheel * 4.0f32);

            // Braking force limit: 1.2G limit per wheel
            let max_brake_force = mass_per_wheel * 9.81f32 * 1.2f32;
            let wheel_brake_force = raw_brake_force.clamp_length_max(max_brake_force);

            total_linear_force += wheel_brake_force;
            total_torque += (mount_world - com_world).cross(wheel_brake_force);
        }

        // --- Lateral Grip / Anti-Skid Force ---
        let lateral_dir = if is_front {
            Vec3::new(-steer_dir_world.z, 0.0, steer_dir_world.x).normalize_or_zero()
        } else {
            car_right
        };
        let lateral_dir_plane =
            (lateral_dir - force_dir * lateral_dir.dot(force_dir)).normalize_or_zero();
        let speed_lateral = velocity_at_wheel.dot(lateral_dir_plane);

        let mut lateral_force_mag = Vec3::ZERO;
        if grounded_wheels >= 2 {
            let raw_lateral_force = -lateral_dir_plane * speed_lateral * mass_per_wheel * 10.0f32;
            let max_lateral_force = mass_per_wheel * 9.81f32 * 1.2f32; // tire grip limit
            lateral_force_mag = raw_lateral_force.clamp_length_max(max_lateral_force);
        }

        total_linear_force += lateral_force_mag;
        total_torque += (mount_world - com_world).cross(lateral_force_mag);
    }

    // Apply the average height and push-up depenetration
    if engaged_wheels_count > 0 {
        drive_state.avg_suspension_height = total_engaged_height / engaged_wheels_count as f32;

        if max_deficit > 0.0 {
            let push_dir = (avg_normal / engaged_wheels_count as f32).normalize_or_zero();
            if push_dir.length_squared() > 0.001f32 {
                car_transform.translation += push_dir * max_deficit;
            }
        }
    } else {
        drive_state.avg_suspension_height = 0.0f32;
    }

    // Apply the accumulated forces & torques
    car_forces.apply_force(total_linear_force);
    car_forces.apply_torque(total_torque);

    // Dynamic rotation correction to align velocity with steering direction at speed
    if speed > 0.5f32 {
        let current_velocity = car_forces.linear_velocity();
        let vel_xz = Vec3::new(current_velocity.x, 0.0, current_velocity.z);
        if vel_xz.length_squared() > 0.001f32 {
            let speed_xz = vel_xz.length();
            let vel_dir_xz = vel_xz / speed_xz;
            let drive_dir = if drive_state.is_reverse {
                -steer_dir_world
            } else {
                steer_dir_world
            };
            let drive_xz = Vec3::new(drive_dir.x, 0.0, drive_dir.z).normalize_or_zero();
            let correction_factor = 4.5f32;
            let new_dir_xz =
                Vec3::lerp(vel_dir_xz, drive_xz, correction_factor * dt).normalize_or_zero();
            let new_vel_xz = new_dir_xz * speed_xz;

            let mut target_velocity = current_velocity;
            target_velocity.x = new_vel_xz.x;
            target_velocity.z = new_vel_xz.z;

            let delta_vel = target_velocity - current_velocity;
            let impulse = delta_vel * drive_state.car_mass;
            car_forces.apply_linear_impulse(impulse);
        }
    }
}
}

#[derive(Component)]
pub struct CosmeticWheel {
    pub wheel_idx: usize,
    pub parent_car: Entity,
    pub accumulated_rotation: f32,
}

pub fn update_cosmetic_wheels(
    mut q_wheels: Query<(&mut Transform, &mut CosmeticWheel)>,
    q_cars: Query<
        (&Transform, &CarDriveState, &CarWheelsContactData, &LinearVelocity),
        (With<Car>, Without<CosmeticWheel>),
    >,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut wheel_transform, mut cosmetic_wheel) in q_wheels.iter_mut() {
        if let Ok((car_transform, drive_state, contact_data, lin_vel)) =
            q_cars.get(cosmetic_wheel.parent_car)
        {
            let wheel_data = &contact_data.wheels[cosmetic_wheel.wheel_idx];

            let is_front = cosmetic_wheel.wheel_idx == 0 || cosmetic_wheel.wheel_idx == 1;
            let is_right = cosmetic_wheel.wheel_idx == 1 || cosmetic_wheel.wheel_idx == 3;

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
            for &dist in &wheel_data.ray_distances {
                if dist != f32::MAX && dist <= drive_state.traction_loss_threshold {
                    engaged_dist += dist;
                    engaged_count += 1;
                }
            }

            let mut y_offset = drive_state.ray_start_y_offset
                - drive_state.traction_loss_threshold
                + drive_state.wheel_radius;
            if engaged_count > 0 {
                let avg_dist = engaged_dist / engaged_count as f32;
                y_offset = drive_state.ray_start_y_offset - avg_dist + drive_state.wheel_radius;
            }

            let local_pos = Vec3::new(x_offset, y_offset, z_offset);
            wheel_transform.translation = car_transform.transform_point(local_pos);

            let car_forward = car_transform.rotation * Vec3::NEG_Z;
            let forward_speed = lin_vel.0.dot(car_forward);
            let rot_speed = forward_speed / (drive_state.wheel_radius * 0.6);

            cosmetic_wheel.accumulated_rotation += rot_speed * dt;

            let steer_angle = if is_front {
                let speed = lin_vel.0.length();
                let max_steer = 1.2f32 / (1.0f32 + 0.3f32 * speed);
                drive_state.current_steer_integrated * max_steer
            } else {
                0.0
            };

            let base_y_rot = if is_right {
                std::f32::consts::FRAC_PI_2
            } else {
                -std::f32::consts::FRAC_PI_2
            };

            let total_y_rot = base_y_rot - steer_angle;

            let rot_y = Quat::from_rotation_y(total_y_rot);
            let rot_x = Quat::from_rotation_x(cosmetic_wheel.accumulated_rotation);

            wheel_transform.rotation = car_transform.rotation * rot_y * rot_x;
        }
    }
}


pub fn update_wheel_contact_normals(
    spatial_query: SpatialQuery,
    mut q_cars: Query<(Entity, &Transform, &CarDriveState, &mut CarWheelsContactData), With<Car>>,
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
            let mut normals = [Vec3::ZERO; 9];
            let mut min_hit_dist = f32::MAX;
            let mut raw_hits_count = 0;

            for i in 0..9 {
                if let Some(hit) =
                    spatial_query.cast_ray(world_origins[i], ray_dir, max_dist, solid, &filter)
                {
                    distances[i] = hit.distance;
                    hit_points[i] = world_origins[i] + *ray_dir * hit.distance;
                    normals[i] = hit.normal;
                    min_hit_dist = min_hit_dist.min(hit.distance);
                    raw_hits_count += 1;
                }
            }

            let mut hits_count = 0;
            let mut sum_normals = Vec3::ZERO;

            if raw_hits_count > 0 {
                // Ignore rays that pass through cracks in the top surface mesh (> min_hit_dist + 0.25)
                let max_valid_dist = min_hit_dist + 0.25;
                for i in 0..9 {
                    if distances[i] <= max_valid_dist {
                        sum_normals += normals[i];
                        hits_count += 1;
                    } else {
                        distances[i] = f32::MAX;
                        hit_points[i] = Vec3::ZERO;
                    }
                }
            }

            let w_contact = &mut contact_data.wheels[wheel_idx];
            w_contact.ray_distances = distances;
            w_contact.hit_points = hit_points;
            w_contact.hits_count = hits_count;

            w_contact.contact_normal = Vec3::Y;
        }
    }
}

pub fn draw_car_gizmos(
    mut gizmos: Gizmos,
    q_car: Query<(&Transform, &CarDriveState, &CarWheelsContactData), With<Car>>,
) {
    for (car_transform, _drive_state, contact_data) in q_car.iter() {

    // Draw orange lines for 9 rays for each of the 4 virtual wheels
    let ray_color = Color::srgb(1.0, 0.5, 0.0);
    let star_color = Color::srgb(0.0, 0.0, 1.0);
    let sphere_color = Color::srgb(0.0, 0.0, 1.0);
    let local_down = car_transform.rotation * Vec3::NEG_Y;

    for wheel_idx in 0..4 {
        let wheel_contact = &contact_data.wheels[wheel_idx];

        for i in 0..9 {
            let start = wheel_contact.ray_origins[i];
            let end = if wheel_contact.ray_distances[i] > 1.0f32 {
                start + local_down * 1.0f32
            } else {
                wheel_contact.hit_points[i]
            };
            gizmos.line(start, end, ray_color);

            // If hit ground and engaged (<= 1.0m), draw blue star and sphere
            if wheel_contact.ray_distances[i] <= 1.0f32 {
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
            if wheel_contact.ray_distances[i] <= 1.0f32 {
                centroid += wheel_contact.hit_points[i];
                engaged_count += 1;
            }
        }

        let has_contact = engaged_count > 0;
        let plane_center = if has_contact {
            centroid / engaged_count as f32
        } else {
            wheel_contact.ray_origins.iter().sum::<Vec3>() / 9.0f32 + local_down * 1.0f32
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
}
}
