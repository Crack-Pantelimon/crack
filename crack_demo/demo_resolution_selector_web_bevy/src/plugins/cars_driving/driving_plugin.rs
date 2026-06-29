use crate::plugins::cars_driving::click_spawn_select_controls::Car;
use avian3d::prelude::{
    AngularVelocity, Forces, LinearVelocity, PhysicsLayer, ReadRigidBodyForces, SpatialQuery,
    SpatialQueryFilter, WriteRigidBodyForces,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

#[derive(PhysicsLayer, Default, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamePhysicsLayer {
    #[default]
    Map,
    Car,
    Wheel,
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

    // Wheel spin angles
    pub wheel_spin_fl: f32,
    pub wheel_spin_fr: f32,
    pub wheel_spin_rl: f32,
    pub wheel_spin_rr: f32,

    // Current visual suspension lengths (to place wheels correctly)
    pub visual_len_fl: f32,
    pub visual_len_fr: f32,
    pub visual_len_rl: f32,
    pub visual_len_rr: f32,
}

impl Default for CarDriveState {
    fn default() -> Self {
        Self {
            history: Vec::new(),
            current_steer_integrated: 0.0,
            avg_accelerate: 0.0,
            avg_brake: 0.0,
            avg_steer: 0.0,
            suspension_stiffness: 45000.0,
            engine_hp: 150.0,
            suspension_height_front: 0.3,
            suspension_height_back: 0.3,
            wheel_spin_fl: 0.0,
            wheel_spin_fr: 0.0,
            wheel_spin_rl: 0.0,
            wheel_spin_rr: 0.0,
            visual_len_fl: 0.3,
            visual_len_fr: 0.3,
            visual_len_rl: 0.3,
            visual_len_rr: 0.3,
        }
    }
}

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
                car_clip_detection,
            )
                .run_if(in_state(self.state.clone())),
        );
        app.add_systems(
            EguiPrimaryContextPass,
            (driving_ui, speedometer_ui).run_if(in_state(self.state.clone())),
        );
    }
}

pub fn camera_follows_car(
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, Without<Car>)>,
    car_query: Query<&Transform, (With<Car>, Without<Camera3d>)>,
) {
    let Ok(car_transform) = car_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }

    // Camera follow parameters
    let follow_distance = 16.0;
    let follow_height = 5.0;
    let speed = 4.0; // Translation lerp speed

    // Nose of the car is towards -z
    let car_forward = *car_transform.forward();

    // Target position is behind the car and slightly above it
    let target_pos =
        car_transform.translation - car_forward * follow_distance + Vec3::Y * follow_height;

    // Exponential decay translation
    let decay = (-speed * dt).exp();
    camera_transform.translation = target_pos + (camera_transform.translation - target_pos) * decay;

    // Look at the car (slightly above the center)
    let look_at_target = car_transform.translation + Vec3::Y * 1.5;

    // Create target rotation
    let mut temp = Transform::from_translation(camera_transform.translation);
    temp.look_at(look_at_target, Vec3::Y);

    // Slerp camera rotation with exponential decay
    let rot_speed = 5.0;
    let rot_decay = (-rot_speed * dt).exp();
    camera_transform.rotation = temp.rotation.slerp(camera_transform.rotation, rot_decay);
}

pub fn keybinds_control_car(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut q_car: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<Car>,
    >,
    spatial_query: SpatialQuery,
    mut commands: Commands,
    mut next_state: ResMut<NextState<crate::plugins::states::GameControlState>>,
) {
    // If escape or F is pressed, exit car
    if keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::KeyF) {
        next_state.set(crate::plugins::states::GameControlState::MapFreecam);
        if let Ok((car_entity, _, _, _)) = q_car.single() {
            commands.entity(car_entity).despawn();
        }
        return;
    }

    let Ok((car_entity, mut transform, mut lin_vel, mut ang_vel)) = q_car.single_mut() else {
        return;
    };

    // Respawn / Reset car
    if keyboard.just_pressed(KeyCode::Space) {
        lin_vel.0 = Vec3::ZERO;
        ang_vel.0 = Vec3::ZERO;
        transform.rotation = Quat::IDENTITY;

        let start_y = transform.translation.y + 100.0;
        let ray_origin = Vec3::new(transform.translation.x, start_y, transform.translation.z);
        let filter = SpatialQueryFilter::from_excluded_entities([car_entity]);

        if let Some(hit) = spatial_query.cast_ray(ray_origin, Dir3::NEG_Y, 1000.0, true, &filter) {
            let ground_y = start_y - hit.distance;
            transform.translation.y = ground_y + 9.0;
        } else {
            transform.translation.y += 9.0;
        }
    }

    let mut accelerate = 0.0;
    let mut brake = 0.0;
    let mut steer = 0.0;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        accelerate = 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        brake = 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        steer -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        steer += 1.0;
    }

    commands.entity(car_entity).trigger(|entity| Drive {
        entity,
        accelerate,
        brake,
        steer,
    });
}

pub fn car_clip_detection(
    mut q_car: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
        ),
        With<Car>,
    >,
    spatial_query: SpatialQuery,
) {
    let Ok((car_entity, mut transform, mut lin_vel, mut ang_vel)) = q_car.single_mut() else {
        return;
    };

    let filter = SpatialQueryFilter::from_excluded_entities([car_entity]);

    // Void detection
    if transform.translation.y < -100.0 {
        info!("Void detected! Resetting car to map height.");
        let start_y = 3500.0; // high above map
        let ray_origin = Vec3::new(transform.translation.x, start_y, transform.translation.z);
        if let Some(hit) = spatial_query.cast_ray(ray_origin, Dir3::NEG_Y, 4000.0, true, &filter) {
            let ground_y = start_y - hit.distance;
            lin_vel.0 = Vec3::ZERO;
            ang_vel.0 = Vec3::ZERO;
            transform.rotation = Quat::IDENTITY;
            transform.translation.y = ground_y + 9.0;
            return;
        }
    }

    // // Clip detection: Cast a ray down from slightly above the car
    // let start_y = transform.translation.y + 5.0;
    // let ray_origin = Vec3::new(transform.translation.x, start_y, transform.translation.z);

    // if let Some(hit) = spatial_query.cast_ray(ray_origin, Dir3::NEG_Y, 100.0, true, &filter) {
    //     let ground_y = start_y - hit.distance;
    //     // If the car's center is below the ground level, it has clipped
    //     if transform.translation.y < ground_y - 0.2 {
    //         info!("Ground clip detected! Resetting car above terrain.");
    //         lin_vel.0 = Vec3::ZERO;
    //         ang_vel.0 = Vec3::ZERO;
    //         transform.rotation = Quat::IDENTITY;
    //         transform.translation.y = ground_y + 9.0;
    //     }
    // }
}

pub fn car_drive_observer(
    trigger: On<Drive>,
    mut query: Query<(&Transform, Forces, &mut CarDriveState)>,
    spatial_query: SpatialQuery,
    time: Res<Time>,
) {
    let car_entity = trigger.event_target();
    let drive_input = trigger.event().clone();

    let Ok((transform, mut forces, mut drive_state)) = query.get_mut(car_entity) else {
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

    // 3. FWD Suspension and Drive physics
    let half_width = 0.9f32;
    let half_length = 1.8f32;
    let wheel_radius = 0.35f32;

    let wheels_offsets = [
        (Vec3::new(-half_width, 0.0, -half_length), true, true), // FL
        (Vec3::new(half_width, 0.0, -half_length), true, false), // FR
        (Vec3::new(-half_width, 0.0, half_length), false, true), // RL
        (Vec3::new(half_width, 0.0, half_length), false, false), // RR
    ];

    let filter = SpatialQueryFilter::from_excluded_entities([car_entity]);
    let ray_dir = transform.rotation * -Vec3::Y;
    let ray_dir_dir = Dir3::new(ray_dir).unwrap_or(Dir3::NEG_Y);

    let max_torque = drive_state.engine_hp * 30.0;
    let engine_torque = drive_state.avg_accelerate * max_torque;
    let steer_angle = drive_state.current_steer_integrated * 30.0f32.to_radians();

    for (offset, is_front, is_left) in wheels_offsets {
        let suspension_len = if is_front {
            drive_state.suspension_height_front
        } else {
            drive_state.suspension_height_back
        };
        let world_attach = transform.transform_point(offset);

        let mut visual_len = suspension_len;

        if let Some(hit) = spatial_query.cast_ray(
            world_attach,
            ray_dir_dir,
            suspension_len + wheel_radius,
            true,
            &filter,
        ) {
            visual_len = (hit.distance - wheel_radius).max(0.0);
            let compression = (suspension_len - visual_len).clamp(0.0, suspension_len);

            // Spring force
            let spring_force = compression * drive_state.suspension_stiffness;

            // Damping force
            let point_velocity = forces.velocity_at_point(world_attach);
            let vel_along_suspension = point_velocity.dot(ray_dir);
            let damping = drive_state.suspension_stiffness * 0.06;
            let damping_force = vel_along_suspension * damping;

            let total_force = (spring_force + damping_force).max(0.0);

            // Apply upward push force to chassis
            let force_vec = -ray_dir * total_force;
            forces.apply_force_at_point(force_vec, world_attach);

            // Apply FWD engine torque on front wheels
            if is_front {
                let local_wheel_forward = Quat::from_rotation_y(steer_angle) * Vec3::NEG_Z;
                let world_wheel_forward = transform.rotation * local_wheel_forward;

                let traction_force = engine_torque / wheel_radius;
                let max_traction = total_force * 0.8;
                let final_traction = traction_force.clamp(-max_traction, max_traction);

                forces.apply_force_at_point(world_wheel_forward * final_traction, world_attach);
            }

            // Apply Braking force
            if drive_state.avg_brake > 0.0 {
                let wheel_forward = if is_front {
                    let local_f = Quat::from_rotation_y(steer_angle) * Vec3::NEG_Z;
                    transform.rotation * local_f
                } else {
                    transform.rotation * Vec3::NEG_Z
                };
                let wheel_vel = forces.velocity_at_point(world_attach);
                let speed_along_wheel = wheel_vel.dot(wheel_forward);

                let brake_force = -wheel_forward
                    * speed_along_wheel.signum()
                    * drive_state.avg_brake
                    * total_force
                    * 0.8;
                forces.apply_force_at_point(brake_force, world_attach);
            }
        }

        // Store visual suspension lengths
        if is_front {
            if is_left {
                drive_state.visual_len_fl = visual_len;
            } else {
                drive_state.visual_len_fr = visual_len;
            }
        } else {
            if is_left {
                drive_state.visual_len_rl = visual_len;
            } else {
                drive_state.visual_len_rr = visual_len;
            }
        }
    }

    // Update wheel spin angles based on actual local speeds
    let front_wheel_forward =
        transform.rotation * (Quat::from_rotation_y(steer_angle) * Vec3::NEG_Z);
    let rear_wheel_forward = transform.rotation * Vec3::NEG_Z;

    let speed_fl = forces
        .velocity_at_point(transform.transform_point(Vec3::new(-half_width, 0.0, -half_length)))
        .dot(front_wheel_forward);
    let speed_fr = forces
        .velocity_at_point(transform.transform_point(Vec3::new(half_width, 0.0, -half_length)))
        .dot(front_wheel_forward);
    let speed_rl = forces
        .velocity_at_point(transform.transform_point(Vec3::new(-half_width, 0.0, half_length)))
        .dot(rear_wheel_forward);
    let speed_rr = forces
        .velocity_at_point(transform.transform_point(Vec3::new(half_width, 0.0, half_length)))
        .dot(rear_wheel_forward);

    drive_state.wheel_spin_fl -= (speed_fl / wheel_radius) * dt;
    drive_state.wheel_spin_fr -= (speed_fr / wheel_radius) * dt;
    drive_state.wheel_spin_rl -= (speed_rl / wheel_radius) * dt;
    drive_state.wheel_spin_rr -= (speed_rr / wheel_radius) * dt;

    // Apply per-wheel lateral friction
    for (offset, is_front, _) in wheels_offsets {
        let world_attach = transform.transform_point(offset);
        let wheel_right = if is_front {
            let local_r = Quat::from_rotation_y(steer_angle) * Vec3::X;
            transform.rotation * local_r
        } else {
            transform.rotation * Vec3::X
        };

        let wheel_vel = forces.velocity_at_point(world_attach);
        let lateral_vel = wheel_vel.dot(wheel_right) * wheel_right;
        let lateral_damping_force = -lateral_vel * 350.0; // mass / 4 * friction factor
        forces.apply_force_at_point(lateral_damping_force, world_attach);
    }

    // Yaw/Angular damping
    let angular_damping_torque = -forces.angular_velocity() * 1200.0 * 1.5;
    forces.apply_torque(angular_damping_torque);

    // Constant drag force
    let drag_force = -forces.linear_velocity() * 50.0;
    forces.apply_force(drag_force);
}

pub fn draw_car_gizmos(mut gizmos: Gizmos, q_car: Query<(&Transform, &CarDriveState), With<Car>>) {
    let Ok((transform, drive_state)) = q_car.single() else {
        return;
    };

    let half_width = 0.9f32;
    let half_height = 0.4f32;
    let half_length = 1.8f32;
    let wheel_radius = 0.35f32; // wheel radius
    let wheel_width = 0.25f32; // wheel width

    // 1. Draw car bbox in white
    let cuboid = Cuboid::from_size(Vec3::new(
        half_width * 2.0,
        half_height * 2.0,
        half_length * 2.0,
    ));
    let isometry = Isometry3d::new(transform.translation, transform.rotation);
    gizmos.primitive_3d(&cuboid, isometry, Color::WHITE);

    // 2. Draw wheels and suspension lines
    let wheels_offsets = [
        (Vec3::new(-half_width, 0.0, -half_length), true, true), // FL
        (Vec3::new(half_width, 0.0, -half_length), true, false), // FR
        (Vec3::new(-half_width, 0.0, half_length), false, true), // RL
        (Vec3::new(half_width, 0.0, half_length), false, false), // RR
    ];

    let steer_angle = drive_state.current_steer_integrated * 30.0f32.to_radians();
    let ray_dir = transform.rotation * -Vec3::Y;

    for (offset, is_front, is_left) in wheels_offsets {
        let world_attach = transform.transform_point(offset);
        let visual_len = if is_front {
            if is_left {
                drive_state.visual_len_fl
            } else {
                drive_state.visual_len_fr
            }
        } else {
            if is_left {
                drive_state.visual_len_rl
            } else {
                drive_state.visual_len_rr
            }
        };

        let wheel_center = world_attach + ray_dir * visual_len;

        // Draw suspension line (green)
        gizmos.line(world_attach, wheel_center, Color::srgb(0.0, 1.0, 0.0));

        // Draw wheel (yellow cylinder)
        let spin = if is_front {
            if is_left {
                drive_state.wheel_spin_fl
            } else {
                drive_state.wheel_spin_fr
            }
        } else {
            if is_left {
                drive_state.wheel_spin_rl
            } else {
                drive_state.wheel_spin_rr
            }
        };

        let steer_quat = if is_front {
            Quat::from_rotation_y(steer_angle)
        } else {
            Quat::IDENTITY
        };
        let spin_quat = Quat::from_rotation_x(spin);

        // Cylinder default axis is Y, rotate by 90 deg around Z to point along local X (axle)
        let axle_quat = Quat::from_rotation_z(90.0f32.to_radians());
        let local_wheel_rot = steer_quat * spin_quat * axle_quat;
        let world_wheel_rot = transform.rotation * local_wheel_rot;

        let cylinder = Cylinder::new(wheel_radius, wheel_width);
        let wheel_isometry = Isometry3d::new(wheel_center, world_wheel_rot);
        gizmos.primitive_3d(&cylinder, wheel_isometry, Color::srgb(1.0, 1.0, 0.0));
    }
}

pub fn driving_ui(mut contexts: EguiContexts) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Draw driving instructions overlay in top-left corner
    egui::Area::new(egui::Id::new("driving_instructions"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(20.0, 50.0))
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .fill(egui::Color32::from_black_alpha(160))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(100, 100, 100),
                ))
                .corner_radius(6.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("DRIVING CONTROLS")
                            .color(egui::Color32::from_rgb(0, 220, 100))
                            .strong(),
                    );
                    ui.allocate_space(egui::Vec2::new(1.0, 5.0));
                    ui.label(
                        egui::RichText::new("• Accelerate: W / Arrow Up")
                            .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new("• Brake/Reverse: S / Arrow Down")
                            .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new("• Steer: A / D or Arrow Left / Right")
                            .color(egui::Color32::WHITE),
                    );
                    ui.label(
                        egui::RichText::new("• Respawn (9m above ground): Space")
                            .color(egui::Color32::from_rgb(0, 180, 255)),
                    );
                    ui.label(
                        egui::RichText::new("• Exit Car (Freecam): Escape / F")
                            .color(egui::Color32::from_rgb(255, 100, 100)),
                    );
                });
        });
}

pub fn speedometer_ui(
    mut contexts: EguiContexts,
    mut q_car: Query<(&avian3d::prelude::LinearVelocity, &mut CarDriveState), With<Car>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let Ok((linear_velocity, mut drive_state)) = q_car.single_mut() else {
        return;
    };

    let speed_kmh = linear_velocity.0.length() * 3.6;

    // Draw glassmorphic speedometer overlay in the bottom right corner
    egui::Area::new(egui::Id::new("speedometer_overlay"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .fill(egui::Color32::from_black_alpha(200))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 220, 255)))
                .corner_radius(10.0)
                .inner_margin(15.0)
                .show(ui, |ui| {
                    ui.set_max_width(280.0); // Constrain layout width so it's not wide and unusable
                    ui.spacing_mut().slider_width = 120.0; // Restrain slider width

                    ui.vertical(|ui| {
                        // Title
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("VEHICLE CONTROL PANEL")
                                    .color(egui::Color32::from_rgb(0, 180, 240))
                                    .size(12.0)
                                    .strong(),
                            );
                        });
                        ui.allocate_space(egui::Vec2::new(1.0, 5.0));

                        // Tuning Sliders
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new("Suspension & Engine Tuning")
                                    .color(egui::Color32::WHITE)
                                    .size(10.0)
                                    .strong(),
                            );

                            // Suspension Stiffness
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Stiffness:")
                                        .size(9.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.suspension_stiffness,
                                        10000.0..=120000.0,
                                    )
                                    .text("N/m")
                                    .step_by(1000.0),
                                );
                            });

                            // Engine HP
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Engine:   ")
                                        .size(9.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.add(
                                    egui::Slider::new(&mut drive_state.engine_hp, 10.0..=500.0)
                                        .text("HP")
                                        .step_by(10.0),
                                );
                            });

                            // Front Suspension Height
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Front H:  ")
                                        .size(9.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.suspension_height_front,
                                        0.2..=0.6,
                                    )
                                    .text("m")
                                    .step_by(0.05),
                                );
                            });

                            // Rear Suspension Height
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Rear H:   ")
                                        .size(9.0)
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.suspension_height_back,
                                        0.2..=0.6,
                                    )
                                    .text("m")
                                    .step_by(0.05),
                                );
                            });
                        });

                        ui.allocate_space(egui::Vec2::new(1.0, 5.0));

                        // Speedometer and input meters sharing the same row!
                        ui.horizontal(|ui| {
                            // Left Column: Speedometer Readout
                            ui.vertical_centered(|ui| {
                                ui.allocate_space(egui::Vec2::new(1.0, 5.0));
                                ui.label(
                                    egui::RichText::new(format!("{:.1}", speed_kmh))
                                        .color(egui::Color32::WHITE)
                                        .size(36.0)
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new("km/h")
                                        .color(egui::Color32::GRAY)
                                        .size(10.0),
                                );
                            });

                            ui.allocate_space(egui::Vec2::new(10.0, 1.0)); // spacing

                            // Right Column: Input Progress Bars
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("ACC")
                                            .size(9.0)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    );
                                    ui.add(
                                        egui::ProgressBar::new(drive_state.avg_accelerate)
                                            .text(format!("{:.2}", drive_state.avg_accelerate))
                                            .fill(egui::Color32::from_rgb(0, 180, 240)),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("BRK")
                                            .size(9.0)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    );
                                    ui.add(
                                        egui::ProgressBar::new(drive_state.avg_brake)
                                            .text(format!("{:.2}", drive_state.avg_brake))
                                            .fill(egui::Color32::from_rgb(220, 50, 50)),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("STR")
                                            .size(9.0)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    );
                                    let steer_val = (drive_state.avg_steer + 1.0) / 2.0;
                                    ui.add(
                                        egui::ProgressBar::new(steer_val)
                                            .text(format!("{:.2}", drive_state.avg_steer))
                                            .fill(egui::Color32::from_rgb(220, 220, 50)),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("INT")
                                            .size(9.0)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    );
                                    let int_steer_val =
                                        (drive_state.current_steer_integrated + 1.0) / 2.0;
                                    ui.add(
                                        egui::ProgressBar::new(int_steer_val)
                                            .text(format!(
                                                "{:.2}",
                                                drive_state.current_steer_integrated
                                            ))
                                            .fill(egui::Color32::from_rgb(50, 220, 100)),
                                    );
                                });
                            });
                        });
                    });
                });
        });
}
