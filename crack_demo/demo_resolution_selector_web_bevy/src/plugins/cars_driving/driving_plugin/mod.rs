
pub mod speedometer_ui;
pub mod keybinds_control;
pub mod spawn_car;
pub mod camera_follow;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use avian3d::prelude::{
    PhysicsLayer, LinearVelocity, AngularVelocity,
    PrismaticJoint, MotorModel,
    DistanceJoint, LinearMotor,
    Collider, Friction, Forces, WriteRigidBodyForces,
    Mass, AngularInertia, CenterOfMass,
    ComputeMassProperties3d, MassPropertiesExt
};
use avian3d::math::Scalar;
use crate::plugins::cars_driving::driving_plugin::{camera_follow::camera_follows_car, spawn_car::Car};
use {keybinds_control::keybinds_control_car, speedometer_ui::speedometer_ui};

pub struct DrivingPlugin<S: States> {
    pub state: S,
}

impl<S: States> Plugin for DrivingPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                camera_follows_car,
                keybinds_control_car,
                draw_car_gizmos,
                cap_car_velocities,
                update_vehicle_physics_from_tuning,
                apply_car_steering_and_drive,
            ).run_if(in_state(self.state.clone())),
        );
        app.add_systems(
            EguiPrimaryContextPass,
            (speedometer_ui,).run_if(in_state(self.state.clone())),
        );
    }
}

#[derive(PhysicsLayer, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamePhysicsLayer {
    #[default]
    Map,
    Car,
    Wheel,
}

#[derive(Component, Clone, Copy)]
pub struct Wheel {
    pub is_front: bool,
    pub is_left: bool,
}

#[derive(Component, Clone, Copy)]
pub struct SuspensionPrismaticJoint {
    pub is_front: bool,
    pub is_left: bool,
}

#[derive(Component, Clone, Copy)]
pub struct SuspensionDistanceJoint {
    pub is_front: bool,
    pub is_left: bool,
}

#[derive(EntityEvent, Clone, Debug)]
pub struct Drive {
    pub entity: Entity,
    pub accelerate: f32, // 0.0 ..= 1.0
    pub brake: f32,      // 0.0 ..= 1.0
    pub steer: f32,      // -1.0 ..= 1.0
}

#[derive(Component, Clone)]
pub struct CarDriveState {
    pub history: Vec<(f32, Drive)>,
    pub current_steer_integrated: f32,
    pub avg_accelerate: f32,
    pub avg_brake: f32,
    pub avg_steer: f32,
    
    // Spawn position for reset functionality
    pub spawn_position: Option<Vec3>,

    // Sliders
    pub suspension_min: f32,
    pub suspension_max: f32,
    pub suspension_rest: f32,
    pub suspension_stiffness: f32,
    pub suspension_damping: f32,

    pub car_mass: f32,
    pub wheel_mass: f32,

    pub car_half_width: f32,
    pub car_half_length: f32,
    pub car_half_height: f32,

    pub wheel_radius: f32,
    pub wheel_width: f32,
    pub wheel_y_offset: f32,
}

impl Default for CarDriveState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            current_steer_integrated: 0.0,
            avg_accelerate: 0.0,
            avg_brake: 0.0,
            avg_steer: 0.0,
            spawn_position: None,

            suspension_min: 0.1,
            suspension_max: 0.5,
            suspension_rest: 0.3,
            suspension_stiffness: 8.0,
            suspension_damping: 0.8,

            car_mass: 1200.0,
            wheel_mass: 25.0,

            car_half_width: 0.9,
            car_half_length: 1.52,
            car_half_height: 0.5,

            wheel_radius: 0.45,
            wheel_width: 0.35,
            wheel_y_offset: 0.9,
        }
    }
}

pub fn cap_car_velocities(
    mut q_car: Query<(&mut LinearVelocity, &mut AngularVelocity), With<Car>>,
) {
    for (mut lin_vel, mut ang_vel) in q_car.iter_mut() {
        // Max speed: 80 km/h = 22.22 m/s
        let max_speed = 22.222;
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

    // 1. Accumulate drive inputs and average over 0.2s
    drive_state.history.push((current_time, drive_input));
    let threshold = current_time - 0.2;
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
    mut q_prismatic: Query<(&mut PrismaticJoint, &SuspensionPrismaticJoint)>,
    mut q_distance: Query<(&mut DistanceJoint, &SuspensionDistanceJoint)>,
    mut q_body: Query<(&mut Mass, &mut AngularInertia, &mut CenterOfMass), (With<Car>, Without<Wheel>)>,
    mut q_wheel: Query<(&mut Collider, &mut Mass, &mut AngularInertia, &mut CenterOfMass, &mut Mesh3d), (With<Wheel>, Without<Car>)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (car_entity, drive_state) in q_car.iter() {
        // 1. Update body mass, inertia, and center of mass
        if let Ok((mut body_mass, mut body_inertia, mut body_center)) = q_body.get_mut(car_entity) {
            let volume = (drive_state.car_half_width * 2.0) * (drive_state.car_half_height * 2.0) * (drive_state.car_half_length * 2.0);
            let shape = Cuboid::new(
                drive_state.car_half_width * 2.0,
                drive_state.car_half_height * 2.0,
                drive_state.car_half_length * 2.0,
            );
            
            let mprops = shape.mass_properties(drive_state.car_mass / volume).to_bundle();
            *body_mass = mprops.mass;
            *body_inertia = mprops.angular_inertia;
            *body_center = mprops.center_of_mass;
        }

        // 2. Update wheel colliders, masses, and meshes
        for (mut wheel_collider, mut wheel_mass, mut wheel_inertia, mut wheel_center, mut wheel_mesh) in q_wheel.iter_mut() {
            let volume = std::f32::consts::PI * drive_state.wheel_radius * drive_state.wheel_radius * drive_state.wheel_width;
            let shape = Cylinder::new(drive_state.wheel_radius, drive_state.wheel_width);
            *wheel_collider = Collider::cylinder(drive_state.wheel_radius, drive_state.wheel_width);
            
            let mprops = shape.mass_properties(drive_state.wheel_mass / volume).to_bundle();
            *wheel_mass = mprops.mass;
            *wheel_inertia = mprops.angular_inertia;
            *wheel_center = mprops.center_of_mass;

            *wheel_mesh = Mesh3d(meshes.add(Cylinder::new(drive_state.wheel_radius, drive_state.wheel_width)));
        }

        // 3. Update Prismatic and Distance joints (anchors & limits & motor)
        for (mut joint, prism) in q_prismatic.iter_mut() {
            let is_front = prism.is_front;
            let is_left = prism.is_left;
            let x_offset = if is_left { -drive_state.car_half_width } else { drive_state.car_half_width + if is_front { 0.1 } else { 0.0 } };
            let y_offset = -drive_state.car_half_height + drive_state.wheel_y_offset;
            let z_offset = if is_front { -drive_state.car_half_length } else { drive_state.car_half_length };
            let anchor1 = Vec3::new(x_offset, y_offset, z_offset);

            joint.frame1.anchor = avian3d::prelude::JointAnchor::Local(anchor1);
            joint.limits = Some(avian3d::prelude::DistanceLimit::new(drive_state.suspension_min, drive_state.suspension_max));
            joint.motor = LinearMotor::new(MotorModel::SpringDamper {
                frequency: drive_state.suspension_stiffness,
                damping_ratio: drive_state.suspension_damping,
            })
            .with_target_position(drive_state.suspension_rest)
            .with_max_force(Scalar::MAX);
        }

        for (mut joint, dist) in q_distance.iter_mut() {
            let is_front = dist.is_front;
            let is_left = dist.is_left;
            let x_offset = if is_left { -drive_state.car_half_width } else { drive_state.car_half_width + if is_front { 0.1 } else { 0.0 } };
            let y_offset = -drive_state.car_half_height + drive_state.wheel_y_offset;
            let z_offset = if is_front { -drive_state.car_half_length } else { drive_state.car_half_length };
            let anchor1 = Vec3::new(x_offset, y_offset, z_offset);

            joint.anchor1 = avian3d::prelude::JointAnchor::Local(anchor1);
            joint.limits = avian3d::prelude::DistanceLimit::new(drive_state.suspension_min, drive_state.suspension_max);
        }
    }
}

pub fn apply_car_steering_and_drive(
    q_car: Query<(&Transform, &LinearVelocity, &CarDriveState), With<Car>>,
    mut q_wheels: Query<(Entity, &Wheel, &mut Friction)>,
    mut forces: Query<Forces, Without<Car>>,
) {
    let Ok((car_transform, car_velocity, drive_state)) = q_car.single() else {
        return;
    };

    let speed = car_velocity.length();
    let max_steer = 1.2 / (1.0 + 0.3 * speed);
    
    // Use integrated steering
    let steer_angle = drive_state.current_steer_integrated * max_steer;
    let steer_dir_world = car_transform.rotation * Vec3::new(steer_angle.sin(), 0.0, -steer_angle.cos());

    // Drive target velocity / throttle
    let throttle = drive_state.avg_accelerate - drive_state.avg_brake;

    // Friction control
    let target_friction = if throttle < 0.0 { 0.9 } else { 0.05 };
    for (_, _, mut friction) in &mut q_wheels {
        friction.dynamic_coefficient = target_friction;
        friction.static_coefficient = target_friction;
    }

    // Force control
    let total_mass = drive_state.car_mass + 4.0 * drive_state.wheel_mass;

    // Lateral friction to prevent sliding/spinning
    let steer_side_world = Vec3::new(-steer_dir_world.z, 0.0, steer_dir_world.x).normalize_or_zero();
    let slide_speed = car_velocity.dot(steer_side_world);
    let total_lateral_force = -steer_side_world * (slide_speed * total_mass * 5.0);
    let lateral_force_per_wheel = total_lateral_force / 4.0;

    // Forward drive force
    let mut drive_force_per_wheel = Vec3::ZERO;
    if throttle > 0.0 {
        let target_speed = 120.0f32 / 3.6f32; // ~33.33 m/s
        let current_speed = car_velocity.dot(steer_dir_world);
        let acc = ((target_speed - current_speed) / 4.0f32).max(0.0f32);
        drive_force_per_wheel = steer_dir_world * (total_mass * acc / 2.0f32) * throttle;
    }

    for (wheel_entity, wheel, _) in &q_wheels {
        if let Ok(mut wheel_forces) = forces.get_mut(wheel_entity) {
            let mut wheel_force = lateral_force_per_wheel;
            if wheel.is_front {
                wheel_force += drive_force_per_wheel;
            }
            wheel_forces.apply_force(wheel_force);
        }
    }
}

pub fn draw_car_gizmos(
    mut gizmos: Gizmos,
    q_car: Query<(&Transform, &LinearVelocity, &CarDriveState), With<Car>>,
    q_wheels: Query<(&Wheel, &Transform)>,
) {
    let Ok((car_transform, car_velocity, drive_state)) = q_car.single() else {
        return;
    };

    // Green steer direction lines
    let speed = car_velocity.length();
    let max_steer = 1.2 / (1.0 + 0.3 * speed);
    let steer_angle = drive_state.current_steer_integrated * max_steer;
    let steer_dir_world = car_transform.rotation * Vec3::new(steer_angle.sin(), 0.0, -steer_angle.cos());

    for (wheel, wheel_transform) in &q_wheels {
        if wheel.is_front {
            let start = wheel_transform.translation;
            let end = start + steer_dir_world * 1.5;
            gizmos.line(start, end, Color::srgb(0.0, 1.0, 0.0));
        }
    }
}

