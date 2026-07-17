use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use super::settings::CloudSkySettings;

/// Always-open control window for the cloud skybox demo.
pub fn cloud_sky_window(
    mut contexts: EguiContexts,
    mut settings: ResMut<CloudSkySettings>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("☁ Clouds & Sky")
        .default_width(300.0)
        .show(ctx, |ui| {
            let s = &mut *settings;

            ui.collapsing("Sky", |ui| {
                ui.add(egui::Slider::new(&mut s.time_of_day, 0.0..=24.0).text("Time of day"));
                ui.add(
                    egui::Slider::new(&mut s.wind_speed, 0.0..=0.2)
                        .text("Wind speed")
                        .step_by(0.001),
                );
                ui.add(
                    egui::Slider::new(&mut s.wind_direction_deg, 0.0..=360.0)
                        .text("Wind direction"),
                );
                ui.add(egui::Slider::new(&mut s.cloud_scale, 0.2..=3.0).text("Cloud scale"));
            });

            ui.collapsing("Cumulus (puffy)", |ui| {
                ui.add(egui::Slider::new(&mut s.cumulus_amount, 0.0..=1.0).text("Amount"));
                ui.add(egui::Slider::new(&mut s.cumulus_detail, 1.0..=8.0).text("Detail"));
            });

            ui.collapsing("Cirrus (wispy)", |ui| {
                ui.add(egui::Slider::new(&mut s.cirrus_amount, 0.0..=1.0).text("Amount"));
                ui.add(egui::Slider::new(&mut s.cirrus_detail, 1.0..=8.0).text("Detail"));
            });

            ui.collapsing("Storm (dark)", |ui| {
                ui.add(egui::Slider::new(&mut s.storm_amount, 0.0..=1.0).text("Amount"));
                ui.add(egui::Slider::new(&mut s.storm_detail, 1.0..=8.0).text("Detail"));
            });

            ui.collapsing("Precipitation", |ui| {
                ui.add(egui::Slider::new(&mut s.rain_intensity, 0.0..=1.0).text("Rain"));
                ui.add(egui::Slider::new(&mut s.snow_intensity, 0.0..=1.0).text("Snow"));
            });

            ui.collapsing("Ground shadow", |ui| {
                ui.add(
                    egui::Slider::new(&mut s.cloud_shadow_intensity, 0.0..=1.0)
                        .text("Shadow intensity"),
                );
            });
        });
    Ok(())
}
