//! AI debug gizmos and egui UI.

use bevy::prelude::*;
use bevy_egui::EguiContexts;

use super::{AiPedestrian, AiPerception, AiSteer, faction::{Faction, Health}};

/// Toggle for AI debug visualization.
#[derive(Resource, Default)]
pub struct AiDebug {
    pub show_rays: bool,
}

/// Egui window showing AI debug controls and faction status.
pub fn ai_debug_ui(
    mut contexts: EguiContexts,
    mut ai_debug: ResMut<AiDebug>,
    query: Query<(&Faction, &Health), With<AiPedestrian>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    bevy_egui::egui::Window::new("Pedestrian AI")
        .default_open(false)
        .show(ctx, |ui| {
            ui.checkbox(&mut ai_debug.show_rays, "Show AI rays");
            ui.separator();

            // Count peds per faction.
            let mut counts = std::collections::HashMap::new();
            let mut total = 0u32;
            for (faction, health) in &query {
                if health.current > 0.0 {
                    *counts.entry(*faction).or_insert(0u32) += 1;
                    total += 1;
                }
            }

            ui.label(format!("Alive: {}", total));
            for faction in Faction::COMBATANTS {
                let count = counts.get(&faction).copied().unwrap_or(0);
                ui.colored_label(
                    faction_to_egui_color(faction),
                    format!("  {}: {}", faction.label(), count),
                );
            }
        });
}

fn faction_to_egui_color(f: Faction) -> bevy_egui::egui::Color32 {
    match f {
        Faction::Red => bevy_egui::egui::Color32::from_rgb(255, 60, 60),
        Faction::Green => bevy_egui::egui::Color32::from_rgb(60, 255, 60),
        Faction::Blue => bevy_egui::egui::Color32::from_rgb(80, 100, 255),
        Faction::Yellow => bevy_egui::egui::Color32::from_rgb(255, 230, 60),
        Faction::Neutral => bevy_egui::egui::Color32::GRAY,
    }
}

/// Draw AI debug gizmos: LOS rays, probe rays, faction markers, HP bars.
pub fn draw_ai_gizmos(
    ai_debug: Res<AiDebug>,
    mut gizmos: Gizmos,
    query: Query<
        (&GlobalTransform, &Faction, &Health, &AiPerception, &AiSteer),
        With<AiPedestrian>,
    >,
) {
    if !ai_debug.show_rays {
        return;
    }

    for (gt, faction, health, perception, steer) in &query {
        let pos = gt.translation();

        // Faction-tinted marker sphere.
        gizmos.sphere(pos + Vec3::Y * 2.0, 0.15, faction.color());

        // HP bar: a horizontal line above the head, scaled by health percentage.
        let hp_frac = (health.current / health.max).clamp(0.0, 1.0);
        let bar_width = 0.8;
        let bar_y = pos.y + 2.3;
        let bar_left = pos - Vec3::X * (bar_width / 2.0);
        let bar_right = bar_left + Vec3::X * (bar_width * hp_frac);
        let hp_color = if hp_frac > 0.5 {
            Color::srgb(0.2, 1.0, 0.2)
        } else if hp_frac > 0.25 {
            Color::srgb(1.0, 0.8, 0.2)
        } else {
            Color::srgb(1.0, 0.2, 0.2)
        };
        gizmos.line(
            Vec3::new(bar_left.x, bar_y, pos.z),
            Vec3::new(bar_right.x, bar_y, pos.z),
            hp_color,
        );

        // LOS ray.
        if let Some((from, to, is_enemy)) = perception.last_los {
            let color = if is_enemy {
                Color::srgb(1.0, 0.2, 0.2)
            } else {
                Color::srgb(0.2, 1.0, 0.2)
            };
            gizmos.line(from, to, color);
        }

        // Probe rays from movement/steering.
        for (from, to, color) in &steer.last_probes {
            gizmos.line(*from, *to, *color);
        }
    }
}
