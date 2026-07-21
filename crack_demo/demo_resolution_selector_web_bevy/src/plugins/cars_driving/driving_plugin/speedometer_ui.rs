use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::egui_theme::{ACCENT, HUD_FAMILY};
use crate::plugins::cars_driving::driving_plugin::CarDriveState;
use crate::plugins::cars_driving::driving_plugin::spawn_car::ActivePlayerVehicle;
use crate::ui_egui::UiState;

/// speedometer ui.
pub fn speedometer_ui(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState>,
    mut q_car: Query<
        (&avian3d::prelude::LinearVelocity, &mut CarDriveState),
        With<ActivePlayerVehicle>,
    >,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let Ok((linear_velocity, mut drive_state)) = q_car.single_mut() else {
        return;
    };

    let speed_kmh = linear_velocity.0.length() * 3.6;

    draw_dashboard(ctx, speed_kmh, &drive_state);
    draw_tuning_window(ctx, &mut ui_state, &mut drive_state);
}

/// Permanent, minimal HUD in the bottom-right corner: speed, gear, RPM.
fn draw_dashboard(ctx: &egui::Context, speed_kmh: f32, drive_state: &CarDriveState) {
    let hud = egui::FontFamily::Name(HUD_FAMILY.into());

    egui::Area::new(egui::Id::new("speedometer_overlay"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(190))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 55, 45)))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.vertical(|ui| {
                        // Big speed readout + unit.
                        ui.horizontal(|ui| {
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!("{:.0}", speed_kmh.max(0.0)))
                                    .family(hud.clone())
                                    .color(egui::Color32::WHITE)
                                    .size(46.0),
                            );
                            ui.vertical(|ui| {
                                ui.add_space(10.0);
                                ui.label(
                                    egui::RichText::new("KM/H")
                                        .family(hud.clone())
                                        .color(ACCENT)
                                        .size(15.0),
                                );
                            });
                        });

                        // Gear + RPM row.
                        ui.horizontal(|ui| {
                            let gear_txt = if drive_state.is_reverse {
                                "R".to_string()
                            } else {
                                format!("G{}", drive_state.current_gear)
                            };
                            ui.label(
                                egui::RichText::new(gear_txt)
                                    .family(hud.clone())
                                    .color(ACCENT)
                                    .size(22.0),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!("{:.0}", drive_state.engine_rpm))
                                    .family(hud.clone())
                                    .color(egui::Color32::from_rgb(200, 200, 205))
                                    .size(22.0),
                            );
                            ui.label(
                                egui::RichText::new("RPM")
                                    .color(egui::Color32::from_rgb(130, 130, 135))
                                    .size(11.0),
                            );
                        });
                    });
                });
        });
}

/// Suspension tuning sliders + control feedback bars. Hidden by default, toggled
/// from the Debug menu ("Vehicle Tuning").
fn draw_tuning_window(
    ctx: &egui::Context,
    ui_state: &mut UiState,
    drive_state: &mut CarDriveState,
) {
    if !ui_state.show_vehicle_tuning {
        return;
    }

    let mut open = ui_state.show_vehicle_tuning;
    egui::Window::new("Vehicle Tuning")
        .open(&mut open)
        .default_width(300.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.spacing_mut().slider_width = 130.0;

            // --- Suspension & engine tuning sliders ---
            ui.group(|ui| {
                ui.label(
                    egui::RichText::new("SUSPENSION TUNING")
                        .color(ACCENT)
                        .size(11.0)
                        .strong(),
                );
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Max Ray Length:").size(10.0));
                    ui.add(
                        egui::Slider::new(&mut drive_state.max_ray_length, 0.60..=1.80)
                            .text("m")
                            .step_by(0.02),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Rest Length (%):").size(10.0));
                    ui.add(
                        egui::Slider::new(&mut drive_state.rest_length_pct, 10.0..=90.0)
                            .text("%")
                            .step_by(1.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Height response:").size(10.0));
                    ui.add(
                        egui::Slider::new(&mut drive_state.height_response, 0.05..=0.50)
                            .text("s")
                            .step_by(0.01),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Tilt response:").size(10.0));
                    ui.add(
                        egui::Slider::new(&mut drive_state.tilt_response, 0.05..=0.50)
                            .text("s")
                            .step_by(0.01),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Grip:").size(10.0));
                    ui.add(egui::Slider::new(&mut drive_state.grip, 0.5..=10.0).step_by(0.1));
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Max Speed:").size(10.0));
                    ui.add(
                        egui::Slider::new(&mut drive_state.car_max_speed, 40.0..=300.0)
                            .text("km/h")
                            .step_by(5.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Horsepower:").size(10.0));
                    ui.add(
                        egui::Slider::new(&mut drive_state.horsepower, 50.0..=1000.0)
                            .text("HP")
                            .step_by(10.0),
                    );
                });
            });

            ui.add_space(6.0);

            // --- Control feedback bars ---
            ui.group(|ui| {
                ui.label(
                    egui::RichText::new("CONTROL FEEDBACK")
                        .color(ACCENT)
                        .size(11.0)
                        .strong(),
                );
                feedback_bar(
                    ui,
                    "ACC",
                    drive_state.avg_accelerate,
                    drive_state.avg_accelerate,
                    ACCENT,
                );
                feedback_bar(
                    ui,
                    "BRK",
                    drive_state.avg_brake,
                    drive_state.avg_brake,
                    egui::Color32::from_rgb(220, 50, 50),
                );
                feedback_bar(
                    ui,
                    "STR",
                    (drive_state.avg_steer + 1.0) / 2.0,
                    drive_state.avg_steer,
                    egui::Color32::from_rgb(220, 200, 60),
                );
                feedback_bar(
                    ui,
                    "INT",
                    (drive_state.current_steer_integrated + 1.0) / 2.0,
                    drive_state.current_steer_integrated,
                    egui::Color32::from_rgb(90, 200, 120),
                );
            });
        });

    ui_state.show_vehicle_tuning = open;
}

/// `frac` drives the bar fill (0..1); `raw` is the numeric value shown as text.
fn feedback_bar(ui: &mut egui::Ui, label: &str, frac: f32, raw: f32, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(10.0)
                .color(egui::Color32::LIGHT_GRAY),
        );
        ui.add(
            egui::ProgressBar::new(frac.clamp(0.0, 1.0))
                .text(format!("{:.2}", raw))
                .fill(color),
        );
    });
}
