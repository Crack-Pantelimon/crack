use avian3d::prelude::SpatialQuery;
use bevy::prelude::*;

use super::{CAR_TOP_FUDGE, TrafficCar, TrafficConfig, VIEW_RAYCAST_HZ};
use crate::plugins::cars_driving::driving_plugin::CarDriveState;
use crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera;

/// despawn traffic cars.
pub fn despawn_traffic_cars(
    time: Res<Time>,
    config: Res<TrafficConfig>,
    mut q_cars: Query<(Entity, &Transform, &CarDriveState, &mut TrafficCar)>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_parent: Query<&ChildOf>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
    mut raycast_timer: Local<f32>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    *raycast_timer += dt;
    let run_raycasts = if *raycast_timer >= 1.0 / VIEW_RAYCAST_HZ {
        *raycast_timer = 0.0;
        true
    } else {
        false
    };

    let Some((camera, cam_gt)) = q_camera.iter().next() else {
        return;
    };
    let camera_pos = cam_gt.translation();

    for (entity, transform, drive_state, mut traffic_car) in q_cars.iter_mut() {
        let car_pos = transform.translation;
        traffic_car.half_height = drive_state.car_half_height;

        let dist_to_camera = car_pos.distance(camera_pos);

        // 1. Direct should_despawn check (fast path using cached visibility)
        if super::common::should_despawn(dist_to_camera, config.spawn_radius, &traffic_car.state) {
            commands.entity(entity).despawn();
            continue;
        }

        // 2. Out of view timer check
        if run_raycasts {
            let car_top = car_pos + Vec3::Y * (traffic_car.half_height * 2.0 * CAR_TOP_FUDGE);
            let visible = super::common::update_visibility(
                camera,
                cam_gt,
                &spatial_query,
                entity,
                car_top,
                &q_parent,
            );

            traffic_car.state.last_visible = visible;
            if visible {
                traffic_car.state.out_of_view_timer = 0.0;
            }
        }

        if !traffic_car.state.last_visible {
            traffic_car.state.out_of_view_timer += dt;
        } else {
            traffic_car.state.out_of_view_timer = 0.0;
        }

        // Recheck despawn condition after visibility update
        if super::common::should_despawn(dist_to_camera, config.spawn_radius, &traffic_car.state) {
            commands.entity(entity).despawn();
        }
    }
}
