use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use crate::plugins::mission_plugin::state::{MissionState, MissionStatus};
use crate::plugins::mission_plugin::config::MissionList;
use crate::plugins::gta_plugin::car::Car;

pub fn draw_mission_triggers(
    mut gizmos: Gizmos,
    mission_list: Res<MissionList>,
    mission_state: Res<MissionState>,
) {
    if let Some(current_id) = mission_state.current_mission {
        // Draw destination trigger for active mission
        if let Some(mission) = mission_list.missions.iter().find(|m| m.id == current_id) {
            let end_pos = Vec3::from(mission.end_coords);
            let sphere = Sphere::new(mission.radius);
            gizmos.primitive_3d(
                &sphere,
                Isometry3d::from_translation(end_pos),
                Color::srgba(0.0, 1.0, 0.0, 0.4), // Green for end point
            );
        }
    } else {
        // Draw start triggers for available missions
        for mission in &mission_list.missions {
            if mission_state.can_start_mission(mission.id, &mission_list) {
                let start_pos = Vec3::from(mission.start_coords);
                let sphere = Sphere::new(mission.radius);
                gizmos.primitive_3d(
                    &sphere,
                    Isometry3d::from_translation(start_pos),
                    Color::srgba(1.0, 0.5, 0.0, 0.4), // Orange/yellow for starting points
                );
            }
        }
    }
}

pub fn render_mission_hud(
    mut contexts: EguiContexts,
    mission_list: Res<MissionList>,
    mut mission_state: ResMut<MissionState>,
    mut car_query: Query<&mut Transform, With<Car>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Misiuni Pantelimon (HBO Umbre)").show(ctx, |ui| {
        if let Some(current_id) = mission_state.current_mission {
            if let Some(mission) = mission_list.missions.iter().find(|m| m.id == current_id) {
                ui.heading(format!("Misiunea {}: {}", mission.id, mission.title));
                ui.colored_label(egui::Color32::from_rgb(255, 140, 0), format!("Client: {}", mission.client));
                ui.separator();

                ui.label("Obiective:");
                for (idx, obj) in mission.objectives.iter().enumerate() {
                    let is_done = idx < mission_state.current_objective_idx;
                    let is_active = idx == mission_state.current_objective_idx;
                    
                    if is_done {
                        ui.horizontal(|ui| {
                            ui.label("☑");
                            ui.colored_label(egui::Color32::GRAY, obj);
                        });
                    } else if is_active {
                        ui.horizontal(|ui| {
                            ui.label("☐");
                            ui.colored_label(egui::Color32::GREEN, obj);
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("☐");
                            ui.label(obj);
                        });
                    }
                }

                if !mission.dialogues.is_empty() {
                    ui.separator();
                    ui.label("Replici / Dialoguri:");
                    ui.vertical(|ui| {
                        for line in &mission.dialogues {
                            ui.colored_label(egui::Color32::from_rgb(173, 216, 230), line);
                        }
                    });
                }
                
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Debug: Complete Objective").clicked() {
                        mission_state.current_objective_idx += 1;
                        if mission_state.current_objective_idx >= mission.objectives.len() {
                            mission_state.complete_current_mission();
                        }
                    }
                    if ui.button("Teleport to Destination").clicked() {
                        if let Ok(mut transform) = car_query.single_mut() {
                            transform.translation = Vec3::from(mission.end_coords);
                        }
                    }
                });
            }
        } else {
            ui.heading("Nicio misiune activă");
            ui.label("Mergi la marcajele portocalii din cartier pentru a începe o misiune.");
            ui.separator();
            
            ui.label("Misiuni deblocabile:");
            let mut available_missions = Vec::new();
            for mission in &mission_list.missions {
                if mission_state.can_start_mission(mission.id, &mission_list) {
                    available_missions.push(mission);
                }
            }

            if available_missions.is_empty() {
                ui.colored_label(egui::Color32::GOLD, "Ai terminat toate misiunile! Jocul este complet!");
            } else {
                for mission in available_missions {
                    ui.horizontal(|ui| {
                        ui.label(format!("- {}", mission.title));
                        if ui.button("Teleport to Start").clicked() {
                            if let Ok(mut transform) = car_query.single_mut() {
                                transform.translation = Vec3::from(mission.start_coords);
                            }
                        }
                        if ui.button("Force Start").clicked() {
                            mission_state.start_mission(mission.id);
                        }
                    });
                }
            }
        }
    });
}
