use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::plugins::cars_driving::driving_plugin::spawn_car::Car;
use crate::plugins::cars_driving::driving_plugin::CarDriveState;

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
                                egui::RichText::new("VEHICLE TUNING PARAMETERS")
                                    .color(egui::Color32::WHITE)
                                    .size(10.0)
                                    .strong(),
                            );

                            ui.collapsing(egui::RichText::new("Dimensions").size(9.0).color(egui::Color32::LIGHT_GRAY), |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Width:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.car_half_width, 0.3..=2.7).text("2x m").step_by(0.05));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Length:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.car_half_length, 0.73..=6.6).text("2x m").step_by(0.05));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Height:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.car_half_height, 0.2..=1.8).text("2x m").step_by(0.05));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Wheel R:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.wheel_radius, 0.15..=1.35).text("m").step_by(0.02));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Wheel W:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.wheel_width, 0.116..=1.05).text("m").step_by(0.02));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Wheel Y Off:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.wheel_y_offset, -1.0..=1.0).text("m").step_by(0.05));
                                });
                            });

                            ui.collapsing(egui::RichText::new("Masses").size(9.0).color(egui::Color32::LIGHT_GRAY), |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Car:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.car_mass, 400.0..=3600.0).text("kg").step_by(50.0));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Wheel:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.wheel_mass, 8.33..=75.0).text("kg").step_by(1.0));
                                });
                            });

                            ui.collapsing(egui::RichText::new("Suspension").size(9.0).color(egui::Color32::LIGHT_GRAY), |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Min:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.suspension_min, 0.033..=0.3).text("m").step_by(0.01));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Max:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.suspension_max, 0.166..=1.5).text("m").step_by(0.05));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Rest:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.suspension_rest, 0.133..=1.2).text("m").step_by(0.05));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Stiffness:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.suspension_stiffness, 4.0..=36.0).text("Hz").step_by(0.5));
                                });
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Damping:").size(9.0));
                                    ui.add(egui::Slider::new(&mut drive_state.suspension_damping, 0.266..=2.4).text("ratio").step_by(0.05));
                                });
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
