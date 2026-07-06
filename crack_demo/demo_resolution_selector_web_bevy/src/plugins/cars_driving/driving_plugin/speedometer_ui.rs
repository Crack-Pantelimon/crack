use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::plugins::cars_driving::driving_plugin::CarDriveState;
use crate::plugins::cars_driving::driving_plugin::spawn_car::ActivePlayerVehicle;

pub fn speedometer_ui(
    mut contexts: EguiContexts,
    mut q_car: Query<(&avian3d::prelude::LinearVelocity, &mut CarDriveState), With<ActivePlayerVehicle>>,
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

                        // Tuning Sliders: Exactly 2 Sliders
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new("SUSPENSION TUNING")
                                    .color(egui::Color32::WHITE)
                                    .size(10.0)
                                    .strong(),
                            );

                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Max Ray Length:").size(9.0));
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.max_ray_length,
                                        0.60..=1.80,
                                    )
                                    .text("m")
                                    .step_by(0.02),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Rest Length (%):").size(9.0));
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.rest_length_pct,
                                        10.0..=90.0,
                                    )
                                    .text("%")
                                    .step_by(1.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Stiffness scale:").size(9.0));
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.stiffness_scale,
                                        0.0..=6.0,
                                    )
                                    .step_by(0.1),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Damping ratio:").size(9.0));
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.damping_ratio,
                                        0.2..=2.0,
                                    )
                                    .step_by(0.05),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Max Speed:").size(9.0));
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.car_max_speed,
                                        40.0..=300.0,
                                    )
                                    .text("km/h")
                                    .step_by(5.0),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Horsepower:").size(9.0));
                                ui.add(
                                    egui::Slider::new(
                                        &mut drive_state.horsepower,
                                        50.0..=1000.0,
                                    )
                                    .text("HP")
                                    .step_by(10.0),
                                );
                            });
                        });

                        ui.allocate_space(egui::Vec2::new(1.0, 5.0));

                        // Speedometer and input meters sharing the same row!
                        ui.horizontal(|ui| {
                            // Left Column: Speedometer Readout
                            ui.vertical_centered(|ui| {
                                ui.allocate_space(egui::Vec2::new(1.0, 5.0));
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{:.0}", speed_kmh))
                                            .color(egui::Color32::WHITE)
                                            .size(36.0)
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(if drive_state.is_reverse {
                                            "R".to_string()
                                        } else {
                                            format!("G{}", drive_state.current_gear)
                                        })
                                        .color(egui::Color32::from_rgb(0, 220, 255))
                                        .size(36.0)
                                        .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("{:.0} RPM", drive_state.engine_rpm))
                                            .color(egui::Color32::LIGHT_GRAY)
                                            .size(18.0)
                                    );
                                });
                                ui.label(
                                    egui::RichText::new("km/h  /  gear")
                                        .color(egui::Color32::GRAY)
                                        .size(10.0),
                                );
                            });

                            ui.allocate_space(egui::Vec2::new(10.0, 1.0)); // spacing

                            // Right Column: Input Progress Bars & RPM
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("RPM")
                                            .size(9.0)
                                            .color(egui::Color32::LIGHT_GRAY),
                                    );
                                    let rpm_ratio =
                                        (drive_state.engine_rpm / 6500.0).clamp(0.0, 1.0);
                                    let rpm_color = if drive_state.engine_rpm > 5500.0 {
                                        egui::Color32::from_rgb(220, 50, 50)
                                    } else if drive_state.engine_rpm > 4500.0 {
                                        egui::Color32::from_rgb(220, 220, 50)
                                    } else {
                                        egui::Color32::from_rgb(50, 220, 100)
                                    };
                                    ui.add(
                                        egui::ProgressBar::new(rpm_ratio)
                                            .text(format!("{:.0} RPM", drive_state.engine_rpm))
                                            .fill(rpm_color),
                                    );
                                });
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
