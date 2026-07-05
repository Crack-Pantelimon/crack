//! Freecam right-click "spawn pedestrian / spawn car" choice popup.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use super::spawn::{SpawnChoicePopup, SpawnControlledPedestrianEvent};
use crate::plugins::cars_driving::{
    car_info::get_random_car_type, driving_plugin::spawn_car::SpawnCarRequestEvent,
};

/// On right-click in freecam, raycast to the map and open the choice popup at that point.
pub fn handle_freecam_right_click(
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: SpatialQuery,
    mut contexts: EguiContexts,
    mut popup: ResMut<SpawnChoicePopup>,
) {
    if !mouse_button.just_pressed(MouseButton::Right) {
        return;
    }
    if let Ok(ctx) = contexts.ctx_mut() {
        if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
            return;
        }
    }
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };
    if let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        10000.0,
        true,
        &SpatialQueryFilter::default(),
    ) {
        popup.active = true;
        popup.world_pos = ray.origin + *ray.direction * hit.distance;
        popup.screen_pos = cursor_pos;
    }
}

/// Draws the choice popup and dispatches the chosen spawn.
pub fn spawn_choice_popup_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut popup: ResMut<SpawnChoicePopup>,
) {
    if !popup.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut close = false;
    egui::Area::new(egui::Id::new("spawn_choice_popup"))
        .fixed_pos(egui::pos2(popup.screen_pos.x, popup.screen_pos.y))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label("Spawn here:");
                if ui.button("🚶 Controllable pedestrian").clicked() {
                    commands.trigger(SpawnControlledPedestrianEvent {
                        position: popup.world_pos,
                        url: None,
                        scale: None,
                    });
                    close = true;
                }
                if ui.button("🚗 Car").clicked() {
                    commands.trigger(SpawnCarRequestEvent {
                        position: popup.world_pos,
                        car_type: get_random_car_type().to_string(),
                        rotation: None,
                    });
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if close {
        popup.active = false;
    }
}
