use super::settings::VfxSettings;
use crate::ui_egui::UiState;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

pub fn vfx_controls_window(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState>,
    mut s: ResMut<VfxSettings>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !ui_state.show_vfx_shaders {
        return;
    }

    egui::Window::new("VFX Shaders Controls")
        .open(&mut ui_state.show_vfx_shaders)
        .default_open(true)
        .show(ctx, |ui| {
            ui.collapsing("Car explosion", |ui| {
                ui.checkbox(&mut s.car_fireball, "Fireball");
                ui.checkbox(&mut s.car_smoke, "Explosion smoke");
                ui.checkbox(&mut s.car_black_smoke, "Wreck black smoke");
                ui.checkbox(&mut s.car_explosion_gizmos, "Damage gizmos (3 spheres)");
                ui.checkbox(&mut s.disabled_car_gizmos, "Wreck warning sphere (green)");
                ui.add(
                    egui::Slider::new(&mut s.fireball_lifetime, 0.1..=3.0)
                        .text("Fireball lifetime"),
                );
                ui.add(
                    egui::Slider::new(&mut s.fireball_radius, 1.0..=12.0).text("Fireball radius"),
                );
                ui.add(egui::Slider::new(&mut s.smoke_lifetime, 0.5..=5.0).text("Smoke lifetime"));
                ui.add(egui::Slider::new(&mut s.smoke_opacity, 0.0..=1.0).text("Smoke opacity"));
            });

            ui.collapsing("Gun", |ui| {
                ui.checkbox(&mut s.gun_gizmos, "Gizmos (alpha 0.3)");
                ui.checkbox(&mut s.gun_tracer, "Tracer shader");
                ui.checkbox(&mut s.gun_hit_sparks, "Hit spark burst");
                ui.checkbox(&mut s.gun_muzzle_flash, "Muzzle flash");
                ui.checkbox(&mut s.gun_muzzle_smoke, "Muzzle smoke");
                ui.add(egui::Slider::new(&mut s.tracer_width, 0.01..=0.3).text("Tracer width"));
                ui.add(egui::Slider::new(&mut s.spark_count_scale, 0.1..=3.0).text("Spark scale"));
                ui.add(
                    egui::Slider::new(&mut s.muzzle_smoke_every, 1..=10)
                        .text("Smoke every N shots"),
                );
            });

            ui.collapsing("Clouds", |ui| {
                ui.checkbox(&mut s.clouds, "Enabled");
                ui.checkbox(&mut s.debug_solid, "Debug solid flat white");
                ui.add(egui::Slider::new(&mut s.cloud_coverage, 0.0..=1.0).text("Coverage"));
                ui.add(egui::Slider::new(&mut s.cloud_opacity, 0.0..=1.0).text("Opacity"));
                ui.add(egui::Slider::new(&mut s.cloud_wind_x, -0.05..=0.05).text("Wind X"));
                ui.add(egui::Slider::new(&mut s.cloud_wind_y, -0.05..=0.05).text("Wind Y"));
                ui.add(egui::Slider::new(&mut s.cloud_scale, 0.0005..=0.02).text("Scale"));
            });
        });
}
