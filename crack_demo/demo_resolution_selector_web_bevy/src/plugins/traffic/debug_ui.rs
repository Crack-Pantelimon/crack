use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use super::road_graph::TrafficRoadGraph;
use super::{
    SpawnTrafficCarEvent, SpawnTrafficPedestrianEvent, TrafficCar, TrafficConfig, TrafficPedestrian,
};
use crate::ui_egui::UiState;

pub fn traffic_debug_ui(
    mut contexts: EguiContexts,
    mut config: ResMut<TrafficConfig>,
    ui_state: Option<ResMut<UiState>>,
    q_traffic: Query<Entity, With<TrafficCar>>,
    q_traffic_peds: Query<Entity, With<TrafficPedestrian>>,
    graph: Res<TrafficRoadGraph>,
    q_camera: Query<&GlobalTransform, With<Camera3d>>,
    mut commands: Commands,
) {
    let show = ui_state.map(|s| s.show_traffic_debug).unwrap_or(true);
    if !show {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Traffic Manager")
        .default_open(true)
        .show(ctx, |ui| {
            ui.colored_label(egui::Color32::from_rgb(180, 220, 255), "Vehicles");
            ui.checkbox(&mut config.enabled, "Cars Enabled");
            ui.add(egui::Slider::new(&mut config.spawn_radius, 50.0..=500.0).text("Spawn Radius (m)"));
            ui.add(egui::Slider::new(&mut config.max_cars, 10..=100).text("Max Cars"));
            ui.add(egui::Slider::new(&mut config.speed_kmh, 10.0..=100.0).text("Speed (km/h)"));

            let current_cars = q_traffic.iter().count();
            ui.label(format!("Cars: {} / {}", current_cars, config.max_cars));

            ui.horizontal(|ui| {
                if ui.button("Spawn one car").clicked() {
                    if let Some(cam_gt) = q_camera.iter().next() {
                        let camera_pos = cam_gt.translation();
                        let mut spawned = false;
                        let num_segments = graph.segments.len();
                        if num_segments > 0 {
                            for _ in 0..50 {
                                let seg_idx = (rand::random::<f32>() * num_segments as f32) as usize;
                                let seg = &graph.segments[seg_idx];
                                if seg.points.is_empty() {
                                    continue;
                                }
                                let pt_idx = (rand::random::<f32>() * seg.points.len() as f32) as usize;
                                let pt = seg.points[pt_idx];
                                if camera_pos.distance(pt) <= config.spawn_radius {
                                    commands.trigger(SpawnTrafficCarEvent { position: pt });
                                    spawned = true;
                                    break;
                                }
                            }
                        }
                        if !spawned {
                            warn!("Spawn one: road graph not ready or no segments in spawn radius.");
                        }
                    }
                }

                if ui.button("Despawn all cars").clicked() {
                    for ent in q_traffic.iter() {
                        commands.entity(ent).despawn();
                    }
                }
            });

            ui.separator();

            ui.colored_label(egui::Color32::from_rgb(180, 220, 255), "Pedestrians");
            ui.checkbox(&mut config.ped_enabled, "Peds Enabled");
            ui.add(egui::Slider::new(&mut config.max_peds, 0..=100).text("Max Peds"));

            let current_peds = q_traffic_peds.iter().count();
            ui.label(format!("Peds: {} / {}", current_peds, config.max_peds));

            ui.horizontal(|ui| {
                if ui.button("Spawn one ped").clicked() {
                    if let Some(cam_gt) = q_camera.iter().next() {
                        let camera_pos = cam_gt.translation();
                        let mut spawned = false;
                        let num_segments = graph.segments.len();
                        if num_segments > 0 {
                            for _ in 0..50 {
                                let seg_idx = (rand::random::<f32>() * num_segments as f32) as usize;
                                let seg = &graph.segments[seg_idx];
                                if seg.points.is_empty() {
                                    continue;
                                }
                                let pt_idx = (rand::random::<f32>() * seg.points.len() as f32) as usize;
                                let pt = seg.points[pt_idx];
                                if camera_pos.distance(pt) <= config.spawn_radius {
                                    commands.trigger(SpawnTrafficPedestrianEvent { position: pt });
                                    spawned = true;
                                    break;
                                }
                            }
                        }
                        if !spawned {
                            warn!("Spawn one ped: road graph not ready or no segments in spawn radius.");
                        }
                    }
                }

                if ui.button("Despawn all peds").clicked() {
                    for ent in q_traffic_peds.iter() {
                        commands.entity(ent).despawn();
                    }
                }
            });

            ui.separator();
            ui.checkbox(&mut config.draw_road_gizmos, "Draw Road/Path Gizmos");

            if !graph.built {
                ui.colored_label(egui::Color32::YELLOW, "Waiting for OSM + map load...");
            }
        });
}

pub fn draw_traffic_gizmos(
    mut gizmos: Gizmos,
    graph: Res<TrafficRoadGraph>,
    config: Res<TrafficConfig>,
    q_cars: Query<(&Transform, &TrafficCar)>,
    q_peds: Query<(&Transform, &TrafficPedestrian)>,
) {
    if !config.enabled || !config.draw_road_gizmos || !graph.built {
        return;
    }

    // Draw road segments
    let road_color = Color::srgb(0.0, 0.8, 1.0);
    for seg in &graph.segments {
        for w in seg.points.windows(2) {
            gizmos.line(w[0], w[1], road_color);
        }
    }

    // Draw remaining path and lookahead for active traffic cars
    let car_path_color = Color::srgb(0.9, 0.9, 0.0);
    for (transform, traffic_car) in q_cars.iter() {
        let car_pos = transform.translation;
        let mut prev = car_pos;
        for &pt in traffic_car
            .state
            .path
            .iter()
            .skip(traffic_car.state.next_idx)
        {
            gizmos.line(prev, pt, car_path_color);
            prev = pt;
        }

        // Draw lookahead point
        if traffic_car.state.next_idx < traffic_car.state.path.len() {
            let target = traffic_car.state.path[traffic_car
                .state
                .next_idx
                .min(traffic_car.state.path.len() - 1)];
            gizmos.sphere(target, 0.4, Color::srgb(1.0, 0.0, 0.0));
        }
    }

    // Draw remaining path and lookahead for active traffic pedestrians
    let ped_path_color = Color::srgb(0.0, 0.9, 0.0);
    for (transform, traffic_ped) in q_peds.iter() {
        let ped_pos = transform.translation;
        let mut prev = ped_pos;
        for &pt in traffic_ped
            .state
            .path
            .iter()
            .skip(traffic_ped.state.next_idx)
        {
            gizmos.line(prev, pt, ped_path_color);
            prev = pt;
        }

        // Draw lookahead target
        if traffic_ped.state.next_idx < traffic_ped.state.path.len() {
            let target = traffic_ped.state.path[traffic_ped
                .state
                .next_idx
                .min(traffic_ped.state.path.len() - 1)];
            gizmos.sphere(target, 0.25, Color::srgb(0.0, 0.0, 1.0));
        }
    }
}
