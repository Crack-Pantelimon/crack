use crate::plugins::
    cars_driving::{
        car_info::get_random_car_type, driving_plugin::
            spawn_car::SpawnCarRequestEvent
        ,
    }
;

use bevy::prelude::*;
use bevy_egui::EguiContexts;

pub fn handle_click_raycast_spawn_car(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: avian3d::prelude::SpatialQuery,
    mut contexts: EguiContexts,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
        return;
    }

    if mouse_button.just_pressed(MouseButton::Right) {
        let Ok(window) = window_query.single() else {
            return;
        };
        if let Some(cursor_pos) = window.cursor_position() {
            let Ok((camera, camera_transform)) = camera_query.single() else {
                return;
            };

            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if let Some(hit) = spatial_query.cast_ray(
                    ray.origin,
                    ray.direction,
                    10000.0,
                    true,
                    &avian3d::prelude::SpatialQueryFilter::default(),
                ) {
                    let hit_point = ray.origin + *ray.direction * hit.distance;
                    // lod_state.reference_points.push(hit_point);
                    info!("Spawn car at {:?}", hit_point);
                    commands.trigger(SpawnCarRequestEvent {
                        position: hit_point,
                        car_type: get_random_car_type().to_string(),
                    });
                }
            }
        }
    }
}
