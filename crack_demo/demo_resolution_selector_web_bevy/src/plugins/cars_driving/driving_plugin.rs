use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use avian3d::prelude::{
    PhysicsLayer, LinearVelocity, AngularVelocity,
    PrismaticJoint, RevoluteJoint, MotorModel, DistanceLimit
};
use crate::plugins::cars_driving::click_spawn_select_controls::Car;


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
            (driving_ui, speedometer_ui).run_if(in_state(self.state.clone())),
        );
    }
}

pub fn camera_follows_car(
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, Without<Car>)>,
    car_query: Query<(&Transform, &LinearVelocity), (With<Car>, Without<Camera3d>)>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut contexts: EguiContexts,
    mut local_orbit: Local<Option<(f32, f32)>>, // (yaw, pitch)
) {
    let Ok((car_transform, linear_velocity)) = car_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }

    let egui_focused = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
    } else {
        false
    };

    // Center point is just above the top of the car
    let center = car_transform.translation + Vec3::Y * 1.5;

    // Get car yaw (Y-rotation) in world space
    let (car_yaw, _, _) = car_transform.rotation.to_euler(EulerRot::YXZ);

    // Default behind-the-car positions
    let default_yaw = car_yaw;
    let default_pitch = 15.0f32.to_radians();

    let (mut yaw, mut pitch) = local_orbit.unwrap_or((default_yaw, default_pitch));

    // Mouse drag updates yaw and pitch
    let drag_active = !egui_focused && mouse_button.pressed(MouseButton::Left);
    if drag_active {
        let sensitivity = 0.003;
        for event in mouse_motion.read() {
            yaw -= event.delta.x * sensitivity;
            pitch += event.delta.y * sensitivity;
        }
        pitch = pitch.clamp(-80.0f32.to_radians(), 80.0f32.to_radians());
    } else {
        for _ in mouse_motion.read() {}
    }

    // Auto-centering when speed > 1.0 m/s
    let speed = linear_velocity.0.length();
    if speed > 1.0 {
        let yaw_diff = (default_yaw - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let pitch_diff = default_pitch - pitch;

        let reset_speed = 2.0;
        let decay = (-reset_speed * dt).exp();

        yaw = default_yaw - yaw_diff * decay;
        pitch = default_pitch - pitch_diff * decay;
    }

    *local_orbit = Some((yaw, pitch));

    // Position camera
    let r = 16.0;
    let offset = Vec3::new(
        r * yaw.sin() * pitch.cos(),
        r * pitch.sin(),
        r * yaw.cos() * pitch.cos(),
    );
    camera_transform.translation = center + offset;
    camera_transform.look_at(center, Vec3::Y);
}

pub fn keybinds_control_car(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut q_car: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
            &CarDriveState,
            &Car,
        ),
        With<Car>,
    >,
    mut q_struts: Query<(&mut Transform, &mut LinearVelocity, &mut AngularVelocity, &Strut), (Without<Car>, Without<Wheel>)>,
    mut q_wheels: Query<(&mut Transform, &mut LinearVelocity, &mut AngularVelocity, &Wheel), (Without<Car>, Without<Strut>)>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<crate::plugins::states::GameControlState>>,
) {
    // If escape or F is pressed, exit car
    if keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::KeyF) {
        next_state.set(crate::plugins::states::GameControlState::MapFreecam);
        if let Ok((car_entity, _, _, _, _, car)) = q_car.single() {
            for &child_entity in &car.physics_children {
                commands.entity(child_entity).despawn();
            }
            commands.entity(car_entity).despawn();
        }
        return;
    }

    let Ok((car_entity, mut transform, mut lin_vel, mut ang_vel, drive_state, _car)) = q_car.single_mut() else {
        return;
    };

    // Respawn / Reset car
    if keyboard.just_pressed(KeyCode::Space) {
        lin_vel.0 = Vec3::ZERO;
        ang_vel.0 = Vec3::ZERO;
        transform.rotation = Quat::IDENTITY;
        if let Some(spawn_pos) = drive_state.spawn_position {
            transform.translation = spawn_pos;
            
            // Define geometry variables
            let half_width = 0.9f32;
            let half_length = 1.8f32;
            let suspension_len = 0.3f32;

            // Reset all struts
            for (mut s_transform, mut s_lin_vel, mut s_ang_vel, strut) in q_struts.iter_mut() {
                let x = if strut.is_left { -half_width } else { half_width };
                let y = 0.3;
                let z = if strut.is_front { -half_length } else { half_length };
                let offset = Vec3::new(x, y, z);
                
                s_transform.translation = spawn_pos + offset - Vec3::Y * suspension_len;
                s_transform.rotation = Quat::IDENTITY;
                s_lin_vel.0 = Vec3::ZERO;
                s_ang_vel.0 = Vec3::ZERO;
            }

            // Reset all wheels
            for (mut w_transform, mut w_lin_vel, mut w_ang_vel, wheel) in q_wheels.iter_mut() {
                let x = if wheel.is_left { -half_width } else { half_width };
                let y = 0.3;
                let z = if wheel.is_front { -half_length } else { half_length };
                let offset = Vec3::new(x, y, z);
                
                w_transform.translation = spawn_pos + offset - Vec3::Y * suspension_len;
                w_transform.rotation = Quat::IDENTITY;
                w_lin_vel.0 = Vec3::ZERO;
                w_ang_vel.0 = Vec3::ZERO;
            }
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

/// Directly rotates front wheel transforms to achieve steering.
/// No physics joint needed — we just set the Y-axis rotation on front wheel entities.
pub fn steer_front_wheels(
    q_car: Query<(Entity, &CarDriveState), With<Car>>,
    mut q_wheels: Query<(&mut Transform, &Wheel)>,
) {
    for (_car_entity, drive_state) in q_car.iter() {
        let steer_angle = drive_state.current_steer_integrated * 30.0f32.to_radians();
        
        for (mut transform, wheel) in q_wheels.iter_mut() {
            if wheel.is_front {
                // Apply steering rotation around Y axis
                // Preserve the wheel's existing translation, only change rotation
                transform.rotation = Quat::from_rotation_y(steer_angle);
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
