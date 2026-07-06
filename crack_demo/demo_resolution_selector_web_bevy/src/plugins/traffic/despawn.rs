use bevy::prelude::*;
use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};

use crate::plugins::{
    cars_driving::driving_plugin::{CarDriveState},
};
use super::{TrafficConfig, TrafficCar};

pub fn despawn_traffic_cars(
    time: Res<Time>,
    config: Res<TrafficConfig>,
    mut q_cars: Query<(Entity, &Transform, &CarDriveState, &mut TrafficCar)>,
    q_children: Query<&Children>,
    q_camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
    mut raycast_timer: Local<f32>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    *raycast_timer += dt;
    let run_raycasts = if *raycast_timer >= 0.25 {
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

        // 1. End of path check
        if traffic_car.next_idx >= traffic_car.path.len() {
            commands.entity(entity).despawn();
            continue;
        }

        let dist_to_camera = car_pos.distance(camera_pos);

        // 2. Out of range check (with 1.25 hysteresis)
        if dist_to_camera > config.spawn_radius * 1.25 {
            commands.entity(entity).despawn();
            continue;
        }

        // 3. Stuck check
        if traffic_car.stuck_timer > 6.0 {
            commands.entity(entity).despawn();
            continue;
        }

        // 4. Out of view timer update
        traffic_car.out_of_view_timer += dt;

        if run_raycasts {
            let car_top = car_pos + Vec3::Y * (traffic_car.half_height * 2.0 * 0.95);
            
            // Check frustum first
            let in_frustum = if let Some(ndc) = camera.world_to_ndc(cam_gt, car_top) {
                ndc.x >= -1.0 && ndc.x <= 1.0 && ndc.y >= -1.0 && ndc.y <= 1.0 && ndc.z >= 0.0 && ndc.z <= 1.0
            } else {
                false
            };

            if !in_frustum {
                // Not in frustum -> count as out of view
            } else {
                // In frustum, run occlusion raycast
                let cam_to_car = car_top - camera_pos;
                let dist = cam_to_car.length();
                let dir_vec = cam_to_car.normalize_or_zero();

                if dir_vec != Vec3::ZERO {
                    let mut excluded = vec![entity];
                    if let Ok(children) = q_children.get(entity) {
                        excluded.extend(children.iter());
                    }
                    let filter = SpatialQueryFilter::default().with_excluded_entities(excluded);

                    if let Some(hit_dir) = bevy::prelude::Dir3::new(dir_vec).ok() {
                        if let Some(_hit) = spatial_query.cast_ray(camera_pos, hit_dir, dist - 0.1, true, &filter) {
                            // Hit something else (occluded)
                        } else {
                            // Line of sight clear -> visible!
                            traffic_car.out_of_view_timer = 0.0;
                        }
                    }
                }
            }
        }

        if traffic_car.out_of_view_timer > 4.0 {
            commands.entity(entity).despawn();
        }
    }
}
