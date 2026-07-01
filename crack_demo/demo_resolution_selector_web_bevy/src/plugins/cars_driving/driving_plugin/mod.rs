pub mod camera_follow;
pub mod keybinds_control;
pub mod spawn_car;
pub mod speedometer_ui;

use crate::plugins::cars_driving::driving_plugin::{
    camera_follow::camera_follows_car, spawn_car::Car,
};
use avian3d::math::Scalar;
use avian3d::prelude::{
    AngularInertia, AngularVelocity, CenterOfMass, Collider, ComputeMassProperties3d,
    DistanceJoint, Forces, Friction, LinearMotor, LinearVelocity, Mass, MassPropertiesExt,
    MotorModel, PhysicsLayer, PrismaticJoint, ReadRigidBodyForces, WriteRigidBodyForces,
    SpatialQuery, SpatialQueryFilter, RevoluteJoint,
};
use bevy::prelude::*;
use avian3d::debug_render::{PhysicsGizmos, PhysicsGizmoExt};
use bevy_egui::EguiPrimaryContextPass;
use {keybinds_control::keybinds_control_car, speedometer_ui::speedometer_ui};

pub struct DrivingPlugin<S: States> {
    pub state: S,
}

impl<S: States> Plugin for DrivingPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, configure_gizmo_depth);
        app.add_systems(
            Update,
            (
                camera_follows_car,
                keybinds_control_car,
                update_wheel_contact_normals,
                apply_car_steering_and_drive,
                draw_car_gizmos,
                always_debug_render_wheels,
                cap_car_velocities,
                update_vehicle_physics_from_tuning,
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

#[derive(Component, Clone, Copy)]
pub struct Wheel {
    pub is_front: bool,
    pub is_left: bool,
}

#[derive(Component, Clone, Debug)]
pub struct WheelContactData {
    pub ray_distances: [f32; 4],
    pub hit_points: [Vec3; 4],
    pub ray_origins: [Vec3; 4],
    pub contact_normal: Vec3,
    pub hits_count: u8,
}

impl Default for WheelContactData {
    fn default() -> Self {
        Self {
            ray_distances: [f32::MAX; 4],
            hit_points: [Vec3::ZERO; 4],
            ray_origins: [Vec3::ZERO; 4],
            contact_normal: Vec3::Y,
            hits_count: 0,
        }
    }
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

#[derive(Component, Clone, Copy)]
pub struct WheelRevoluteJoint {
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
    pub is_reverse: bool,

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

    pub revolute_frequency: f32,
    pub revolute_damping: f32,
    pub revolute_max_torque: f32,
    pub car_max_speed: f32,
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

            revolute_frequency: 10.0,
            revolute_damping: 1.0,
            revolute_max_torque: 800.0,
            car_max_speed: 120.0,
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
    mut q_revolute: Query<(&mut RevoluteJoint, &WheelRevoluteJoint)>,
    mut q_body: Query<
        (&mut Mass, &mut AngularInertia, &mut CenterOfMass),
        (With<Car>, Without<Wheel>),
    >,
    mut q_wheel: Query<
        (
            &mut Collider,
            &mut Mass,
            &mut AngularInertia,
            &mut CenterOfMass,
            &mut Mesh3d,
        ),
        (With<Wheel>, Without<Car>),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (car_entity, drive_state) in q_car.iter() {
        // 1. Update body mass, inertia, and center of mass
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

        // 2. Update wheel colliders, masses, and meshes
        for (
            mut wheel_collider,
            mut wheel_mass,
            mut wheel_inertia,
            mut wheel_center,
            mut wheel_mesh,
        ) in q_wheel.iter_mut()
        {
            let volume = std::f32::consts::PI
                * drive_state.wheel_radius
                * drive_state.wheel_radius
                * drive_state.wheel_width;
            let shape = Cylinder::new(drive_state.wheel_radius, drive_state.wheel_width);
            *wheel_collider = Collider::cylinder(drive_state.wheel_radius, drive_state.wheel_width);

            let mprops = shape
                .mass_properties(drive_state.wheel_mass / volume)
                .to_bundle();
            *wheel_mass = mprops.mass;
            *wheel_inertia = mprops.angular_inertia;
            *wheel_center = mprops.center_of_mass;

            *wheel_mesh = Mesh3d(meshes.add(Cylinder::new(
                drive_state.wheel_radius,
                drive_state.wheel_width,
            )));
        }

        // 3. Update Prismatic and Distance joints (anchors & limits & motor)
        for (mut joint, prism) in q_prismatic.iter_mut() {
            let is_front = prism.is_front;
            let is_left = prism.is_left;
            let x_offset = if is_left {
                -drive_state.car_half_width
            } else {
                drive_state.car_half_width + if is_front { 0.1 } else { 0.0 }
            };
            let y_offset = -drive_state.car_half_height + drive_state.wheel_y_offset;
            let z_offset = if is_front {
                -drive_state.car_half_length
            } else {
                drive_state.car_half_length
            };
            let anchor1 = Vec3::new(x_offset, y_offset, z_offset);

            joint.frame1.anchor = avian3d::prelude::JointAnchor::Local(anchor1);
            joint.limits = Some(avian3d::prelude::DistanceLimit::new(
                drive_state.suspension_min,
                drive_state.suspension_max,
            ));
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
            let x_offset = if is_left {
                -drive_state.car_half_width
            } else {
                drive_state.car_half_width + if is_front { 0.1 } else { 0.0 }
            };
            let y_offset = -drive_state.car_half_height + drive_state.wheel_y_offset;
            let z_offset = if is_front {
                -drive_state.car_half_length
            } else {
                drive_state.car_half_length
            };
            let anchor1 = Vec3::new(x_offset, y_offset, z_offset);

            joint.anchor1 = avian3d::prelude::JointAnchor::Local(anchor1);
            joint.limits = avian3d::prelude::DistanceLimit::new(
                drive_state.suspension_min,
                drive_state.suspension_max,
            );
        }

        // 4. Update Revolute joints (motor parameters and anchors)
        for (mut joint, rev) in q_revolute.iter_mut() {
            let is_front = rev.is_front;
            let is_left = rev.is_left;
            let x_offset = if is_left {
                -drive_state.car_half_width
            } else {
                drive_state.car_half_width + if is_front { 0.1 } else { 0.0 }
            };
            let y_offset = -drive_state.car_half_height + drive_state.wheel_y_offset;
            let z_offset = if is_front {
                -drive_state.car_half_length
            } else {
                drive_state.car_half_length
            };
            let anchor1 = Vec3::new(x_offset, y_offset, z_offset);

            joint.frame1.anchor = avian3d::prelude::JointAnchor::Local(anchor1);
            joint.motor.max_torque = drive_state.revolute_max_torque as Scalar;
            joint.motor.motor_model = MotorModel::SpringDamper {
                frequency: drive_state.revolute_frequency as Scalar,
                damping_ratio: drive_state.revolute_damping as Scalar,
            };
        }
    }
}

pub fn apply_car_steering_and_drive(
    mut q_car: Query<(&Transform, &mut LinearVelocity, &CarDriveState), With<Car>>,
    mut q_wheels: Query<(Entity, &Wheel, &mut Friction, &WheelContactData), Without<Car>>,
    mut forces: Query<Forces, Without<Car>>,
    mut q_revolute: Query<&mut RevoluteJoint, With<WheelRevoluteJoint>>,
    time: Res<Time>,
) {
    let Ok((car_transform, mut car_velocity, drive_state)) = q_car.single_mut() else {
        return;
    };

    // Synchronize wheel motor target velocity with desired speed (km/h)
    let signed_speed_kmh = car_velocity.0.dot(car_transform.rotation * Vec3::NEG_Z) * 3.6;
    let acc_term = if drive_state.avg_accelerate > 0.0 { 0.5 } else { 0.0 };
    let desired_speed_kmh = if drive_state.is_reverse {
        signed_speed_kmh - acc_term
    } else {
        signed_speed_kmh + acc_term
    };
    let desired_speed_mps = desired_speed_kmh / 3.6;
    let target_ang_vel = desired_speed_mps / drive_state.wheel_radius;

    for mut joint in &mut q_revolute {
        joint.motor.target_velocity = target_ang_vel as Scalar;
    }

    let speed = car_velocity.length();
    let max_steer = 1.2 / (1.0 + 0.3 * speed);

    // Use integrated steering
    let steer_angle = drive_state.current_steer_integrated * max_steer;
    let steer_dir_world =
        car_transform.rotation * Vec3::new(steer_angle.sin(), 0.0, -steer_angle.cos());

    // Drive target velocity / throttle
    let throttle = drive_state.avg_accelerate - drive_state.avg_brake;

    // Friction control
    let target_friction = if throttle < 0.0 { 0.9 } else { 0.05 };
    for (_, _, mut friction, _) in &mut q_wheels {
        friction.dynamic_coefficient = target_friction;
        friction.static_coefficient = target_friction;
    }

    // Force control
    let total_mass = drive_state.car_mass + 4.0 * drive_state.wheel_mass;

    // Determine target driving direction (forward or reverse)
    let drive_dir = if drive_state.is_reverse {
        -steer_dir_world
    } else {
        steer_dir_world
    };

    // Rotate speed vector to align more with the steering/drive direction to reduce the "on ice" feel
    if speed > 0.5 {
        let vel_xz = Vec3::new(car_velocity.0.x, 0.0, car_velocity.0.z);
        if vel_xz.length_squared() > 0.001 {
            let speed_xz = vel_xz.length();
            let vel_dir_xz = vel_xz / speed_xz;
            let drive_xz = Vec3::new(drive_dir.x, 0.0, drive_dir.z).normalize_or_zero();
            let dt = time.delta_secs().min(0.1);
            let correction_factor = 4.5;
            let new_dir_xz =
                Vec3::lerp(vel_dir_xz, drive_xz, correction_factor * dt).normalize_or_zero();
            let new_vel_xz = new_dir_xz * speed_xz;
            car_velocity.0.x = new_vel_xz.x;
            car_velocity.0.z = new_vel_xz.z;
        }
    }

    // Forward/reverse drive force magnitude per wheel
    let mut drive_force_magnitude = 0.0;
    if throttle > 0.0 {
        let target_speed = if drive_state.is_reverse {
            40.0f32 / 3.6f32 // Max speed: 40 km/h in reverse
        } else {
            120.0f32 / 3.6f32 // Max speed: 120 km/h forward
        };
        let current_speed = car_velocity.0.dot(drive_dir);
        let acc = ((target_speed - current_speed) / 4.0f32).max(0.0f32);
        drive_force_magnitude = (total_mass * acc / 2.0f32) * throttle;
    }

    // Apply anti-skid forces individually per wheel and drive forces to front wheels
    for (wheel_entity, wheel, _, wheel_contact) in &q_wheels {
        if let Ok(mut wheel_forces) = forces.get_mut(wheel_entity) {
            // Retrieve the wheel's linear velocity directly from wheel_forces (implementing ReadRigidBodyForces)
            let wheel_velocity = wheel_forces.linear_velocity();

            // Determine wheel steer angle: front wheels steer, rear wheels are straight
            let wheel_steer_angle = if wheel.is_front { steer_angle } else { 0.0 };

            // Calculate wheel direction and its lateral axis
            let wheel_dir_world = car_transform.rotation
                * Vec3::new(wheel_steer_angle.sin(), 0.0, -wheel_steer_angle.cos());
            let wheel_side_world =
                Vec3::new(-wheel_dir_world.z, 0.0, wheel_dir_world.x).normalize_or_zero();

            // Project forces onto the plane defined by the wheel's contact normal
            let contact_normal = wheel_contact.contact_normal;
            let project_on_plane = |v: Vec3, normal: Vec3| -> Vec3 {
                v - normal * v.dot(normal)
            };

            let wheel_dir_plane = project_on_plane(wheel_dir_world, contact_normal).normalize_or_zero();
            let wheel_side_plane = project_on_plane(wheel_side_world, contact_normal).normalize_or_zero();

            // Compute lateral velocity of the wheel and counter it individually in the plane of contact
            let wheel_slide_speed = wheel_velocity.dot(wheel_side_plane);
            let wheel_mass_val = drive_state.wheel_mass + (drive_state.car_mass / 4.0);
            let mut wheel_force = -wheel_side_plane * (wheel_slide_speed * wheel_mass_val * 6.5);

            // Add drive force to front wheels in the plane of contact
            if wheel.is_front {
                let wheel_drive_dir = if drive_state.is_reverse {
                    -wheel_dir_plane
                } else {
                    wheel_dir_plane
                };
                wheel_force += wheel_drive_dir * drive_force_magnitude;
            }
            wheel_forces.apply_force(wheel_force);
        }
    }
}

pub fn update_wheel_contact_normals(
    spatial_query: SpatialQuery,
    q_cars: Query<(Entity, &Transform, &CarDriveState, &Car)>,
    mut q_wheels: Query<(Entity, &Wheel, &mut WheelContactData), Without<Car>>,
) {
    for (wheel_entity, wheel, mut contact_data) in q_wheels.iter_mut() {
        // Find parent car
        let mut parent_car = None;
        for (_car_entity, car_transform, drive_state, car) in q_cars.iter() {
            if car.physics_children.contains(&wheel_entity) {
                parent_car = Some((car_transform, drive_state));
                break;
            }
        }

        let Some((car_transform, drive_state)) = parent_car else {
            continue;
        };

        // Compute local suspension base center for this wheel
        let is_front = wheel.is_front;
        let is_left = wheel.is_left;
        let x_offset = if is_left {
            -drive_state.car_half_width
        } else {
            drive_state.car_half_width + if is_front { 0.1 } else { 0.0 }
        };
        let y_offset = -drive_state.car_half_height + drive_state.wheel_y_offset;
        let z_offset = if is_front {
            -drive_state.car_half_length
        } else {
            drive_state.car_half_length
        };
        let base_center_local = Vec3::new(x_offset, y_offset, z_offset);

        // Compute 4 corner offsets in local space
        let w = drive_state.wheel_width * 0.5;
        let r = drive_state.wheel_radius * 0.5;

        let local_corners = [
            base_center_local + Vec3::new(-w, 0.0, -r),
            base_center_local + Vec3::new(w, 0.0, -r),
            base_center_local + Vec3::new(-w, 0.0, r),
            base_center_local + Vec3::new(w, 0.0, r),
        ];

        // Transform corners to world space
        let mut world_origins = [Vec3::ZERO; 4];
        for i in 0..4 {
            world_origins[i] = car_transform.transform_point(local_corners[i]);
            contact_data.ray_origins[i] = world_origins[i];
        }

        let ray_dir = Dir3::NEG_Y;
        let max_dist = 3.0;
        let solid = true;
        let filter = SpatialQueryFilter::from_mask([GamePhysicsLayer::Map]);

        let mut distances = [f32::MAX; 4];
        let mut hit_points = [Vec3::ZERO; 4];
        let mut hits_count = 0;

        for i in 0..4 {
            if let Some(hit) = spatial_query.cast_ray(world_origins[i], ray_dir, max_dist, solid, &filter) {
                distances[i] = hit.distance;
                hit_points[i] = world_origins[i] + *ray_dir * hit.distance;
                hits_count += 1;
            } else {
                distances[i] = f32::MAX;
                hit_points[i] = Vec3::ZERO;
            }
        }

        contact_data.ray_distances = distances;
        contact_data.hit_points = hit_points;
        contact_data.hits_count = hits_count;

        // Compute average contact plane normal if we have at least 3 hits
        let mut valid_hits = Vec::new();
        for i in 0..4 {
            if distances[i] != f32::MAX {
                valid_hits.push(hit_points[i]);
            }
        }

        if valid_hits.len() >= 3 {
            let v0 = valid_hits[1] - valid_hits[0];
            let v1 = valid_hits[2] - valid_hits[0];
            let mut normal = v0.cross(v1).normalize_or_zero();

            if valid_hits.len() == 4 {
                let v2 = valid_hits[3] - valid_hits[0];
                let normal2 = v0.cross(v2).normalize_or_zero();
                normal = (normal + normal2).normalize_or_zero();
            }

            if normal.y < 0.0 {
                normal = -normal;
            }

            if normal == Vec3::ZERO || normal.is_nan() {
                contact_data.contact_normal = Vec3::Y;
            } else {
                contact_data.contact_normal = normal;
            }
        } else {
            contact_data.contact_normal = Vec3::Y;
        }
    }
}

pub fn draw_car_gizmos(
    mut gizmos: Gizmos,
    q_car: Query<(&Transform, &LinearVelocity, &CarDriveState), With<Car>>,
    q_wheels: Query<(&Wheel, &Transform, &WheelContactData)>,
) {
    let Ok((car_transform, car_velocity, drive_state)) = q_car.single() else {
        return;
    };

    // Green steer direction lines
    let speed = car_velocity.length();
    let max_steer = 1.2 / (1.0 + 0.3 * speed);
    let steer_angle = drive_state.current_steer_integrated * max_steer;
    let steer_dir_world =
        car_transform.rotation * Vec3::new(steer_angle.sin(), 0.0, -steer_angle.cos());

    for (wheel, wheel_transform, wheel_contact) in &q_wheels {
        let contact_normal = wheel_contact.contact_normal;
        let project_on_plane = |v: Vec3, normal: Vec3| -> Vec3 {
            v - normal * v.dot(normal)
        };

        if wheel.is_front {
            let start = wheel_transform.translation;
            let steer_dir_plane = project_on_plane(steer_dir_world, contact_normal).normalize_or_zero();
            let end = start + steer_dir_plane * 1.5;
            gizmos.line(start, end, Color::srgb(0.0, 1.0, 0.0));
        }

        // Draw cylinder wireframe and rotation indicator line for each wheel
        let wheel_cylinder = Cylinder::new(drive_state.wheel_radius, drive_state.wheel_width);
        gizmos.primitive_3d(
            &wheel_cylinder,
            Isometry3d::new(wheel_transform.translation, wheel_transform.rotation),
            Color::srgb(0.2, 0.6, 1.0),
        );

        let radial_point_local = Vec3::new(0.0, drive_state.wheel_radius, 0.0);
        let radial_point_world = wheel_transform.transform_point(radial_point_local);
        gizmos.line(
            wheel_transform.translation,
            radial_point_world,
            Color::srgb(1.0, 0.0, 1.0),
        );

        // Draw orange lines for 4 rays
        let ray_color = Color::srgb(1.0, 0.5, 0.0);
        let star_color = Color::srgb(0.0, 0.0, 1.0);
        let sphere_color = Color::srgb(0.0, 0.0, 1.0);

        for i in 0..4 {
            let start = wheel_contact.ray_origins[i];
            let end = if wheel_contact.ray_distances[i] == f32::MAX {
                start + Vec3::NEG_Y * 3.0
            } else {
                wheel_contact.hit_points[i]
            };
            gizmos.line(start, end, ray_color);

            // If hit ground, draw blue star and sphere
            if wheel_contact.ray_distances[i] != f32::MAX {
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

        // Draw plane defining the plane segment
        let is_any_ray_too_long = wheel_contact.ray_distances.iter().any(|&d| d > drive_state.suspension_max);
        
        let mut centroid = Vec3::ZERO;
        let mut valid_count = 0;
        for i in 0..4 {
            if wheel_contact.ray_distances[i] != f32::MAX {
                centroid += wheel_contact.hit_points[i];
                valid_count += 1;
            }
        }

        let (plane_center, box_color) = if is_any_ray_too_long {
            // Draw plane in red at the max suspension length
            let origin_centroid = wheel_contact.ray_origins.iter().sum::<Vec3>() / 4.0;
            let clamped_center = origin_centroid + Vec3::NEG_Y * drive_state.suspension_max;
            (clamped_center, Color::srgb(1.0, 0.0, 0.0))
        } else {
            // Use hit centroid and gray color
            let center = if valid_count > 0 {
                centroid / valid_count as f32
            } else {
                wheel_contact.ray_origins.iter().sum::<Vec3>() / 4.0 + Vec3::NEG_Y * drive_state.suspension_max
            };
            (center, Color::srgb(0.5, 0.5, 0.5))
        };

        // Rotated to the contact normal
        let plane_rotation = Quat::from_rotation_arc(Vec3::Y, contact_normal);

        // Draw a nice plane segment using flat cuboid
        let cuboid = Cuboid::new(0.5, 0.01, 0.5);
        gizmos.primitive_3d(
            &cuboid,
            Isometry3d::new(plane_center, plane_rotation),
            box_color,
        );
    }
}

pub fn always_debug_render_wheels(
    q_wheels: Query<(&Collider, &GlobalTransform), With<Wheel>>,
    mut gizmos: Gizmos<PhysicsGizmos>,
) {
    for (collider, transform) in &q_wheels {
        let position = avian3d::prelude::Position::from(transform);
        let rotation = avian3d::prelude::Rotation::from(transform);
        gizmos.draw_collider(collider, position, rotation, Color::srgb(0.2, 0.6, 1.0));
    }
}
