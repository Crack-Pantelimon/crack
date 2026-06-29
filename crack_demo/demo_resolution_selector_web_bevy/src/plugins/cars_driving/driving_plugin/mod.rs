
pub mod speedometer_ui;
pub mod keybinds_control;
pub mod spawn_car;
pub mod camera_follow;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
use avian3d::prelude::{
    PhysicsLayer, LinearVelocity, AngularVelocity,
    PrismaticJoint, RevoluteJoint, MotorModel, DistanceLimit
};
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
                update_vehicle_physics,
                steer_front_wheels,
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

#[derive(Component)]
pub struct Wheel {
    pub is_front: bool,
    pub is_left: bool,
}

#[derive(Component)]
pub struct Strut {
    pub is_front: bool,
    pub is_left: bool,
}

#[derive(Component)]
pub struct SuspensionJoint {
    pub car_entity: Entity,
    pub is_front: bool,
    pub is_left: bool,
}



#[derive(Component)]
pub struct AxleJoint {
    pub car_entity: Entity,
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
    
    // Sliders
    pub suspension_stiffness: f32,
    pub engine_hp: f32,
    pub suspension_height_front: f32,
    pub suspension_height_back: f32,

    
    // Spawn position for reset functionality
    pub spawn_position: Option<Vec3>,
}

impl Default for CarDriveState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            current_steer_integrated: 0.0,
            avg_accelerate: 0.0,
            avg_brake: 0.0,
            avg_steer: 0.0,
            suspension_stiffness: 80000.0,
            engine_hp: 150.0,
            suspension_height_front: 0.3,
            suspension_height_back: 0.3,
            spawn_position: None,
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

pub fn update_vehicle_physics(
    q_car: Query<(Entity, &CarDriveState), With<Car>>,
    mut q_suspension: Query<(&mut PrismaticJoint, &SuspensionJoint)>,
    mut q_axle: Query<(&mut RevoluteJoint, &AxleJoint)>,
) {
    for (car_entity, drive_state) in q_car.iter() {
        // 1. Update suspension joints parameters (stiffness & height)
        for (mut joint, susp) in q_suspension.iter_mut() {
            if susp.car_entity == car_entity {
                let height = if susp.is_front {
                    drive_state.suspension_height_front
                } else {
                    drive_state.suspension_height_back
                };
                
                // Map stiffness to frequency
                // k = mass * (2 * pi * f)^2 => f = sqrt(k / mass) / (2 * pi)
                // mass per wheel is about 300.0kg
                let frequency = (drive_state.suspension_stiffness / 300.0).sqrt() / 6.283185;
                
                joint.frame1.basis = avian3d::prelude::JointBasis::Local(Quat::IDENTITY);
                // Update limits
                joint.limits = Some(DistanceLimit::new(0.0, height));
                
                // Update motor target and frequency
                joint.motor.target_position = height;
                if let MotorModel::SpringDamper { frequency: ref mut f, .. } = joint.motor.motor_model {
                    *f = frequency;
                }
            }
        }

        // 2. Update axle joints (driving & braking)
        for (mut joint, axle) in q_axle.iter_mut() {
            if axle.car_entity == car_entity {
                // Max angular speed: 63.5 rad/s (approx 80 km/h)
                // Negative because Bevy's coordinate system: +Z is backward,
                // so forward motion requires negative angular velocity around +X axis
                let max_ang_vel = -63.5;
                
                if drive_state.avg_brake > 0.0 {
                    // Apply brakes: target speed 0, high torque
                    joint.motor.target_velocity = 0.0;
                    joint.motor.max_torque = drive_state.avg_brake * 2000.0;
                } else if drive_state.avg_accelerate > 0.0 {
                    // Apply throttle
                    joint.motor.target_velocity = drive_state.avg_accelerate * max_ang_vel;
                    joint.motor.max_torque = drive_state.engine_hp * 5.0;
                } else {
                    // Coasting: neutral engine drag
                    joint.motor.target_velocity = 0.0;
                    joint.motor.max_torque = 5.0; // small drag
                }
            }
        }
    }
}

/// Steers the front wheels by rotating front strut transforms relative to the car's current orientation.
/// This preserves the car's freedom to tilt/topple while applying steering.
pub fn steer_front_wheels(
    q_car: Query<(&Transform, &CarDriveState), (With<Car>, Without<Strut>)>,
    mut q_struts: Query<(&mut Transform, &Strut), Without<Car>>,
) {
    for (car_transform, drive_state) in q_car.iter() {
        // Negate so D/Right produces a right turn (clockwise around local Y = negative angle)
        let steer_angle = -drive_state.current_steer_integrated * 30.0f32.to_radians();

        for (mut strut_transform, strut) in q_struts.iter_mut() {
            if strut.is_front {
                // Compose: car's current rotation + steering rotation around car-local Y
                strut_transform.rotation = car_transform.rotation * Quat::from_rotation_y(steer_angle);
            }
        }
    }
}

pub fn draw_car_gizmos(mut gizmos: Gizmos, q_car: Query<&Transform, With<Car>>) {
    let Ok(transform) = q_car.single() else {
        return;
    };

    let half_width = 0.9f32;
    let half_height = 0.4f32;
    let half_length = 1.8f32;

    // 1. Draw car bbox in white
    let cuboid = Cuboid::from_size(Vec3::new(
        half_width * 2.0,
        half_height * 2.0,
        half_length * 2.0,
    ));
    let isometry = Isometry3d::new(transform.translation, transform.rotation);
    gizmos.primitive_3d(&cuboid, isometry, Color::WHITE);
}
