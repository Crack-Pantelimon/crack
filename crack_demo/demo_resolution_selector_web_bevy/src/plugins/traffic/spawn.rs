use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::plugins::{
    cars_driving::driving_plugin::spawn_car::{spawn_physics_car, Car},
    cars_driving::car_info::get_random_car_type,
    geojson::query_point_ground_y,
    map_plugin::MapTree,
    pedestrians::{
        ModelRoot, ManualAnimation, PedestrianManifest,
        spawn_pedestrian::{PedestrianGltf, NeedAlignment},
    },
    pedestrians::pedestrian_controller_plugin::{DriverMesh, CarSeatOffset},
};

use super::{TrafficConfig, TrafficCar, SpawnTrafficCarEvent};
use super::road_graph::{TrafficRoadGraph, quantize};

pub fn get_ground_y(
    pos: Vec3,
    map_tree: Option<&MapTree>,
    spatial_query: &avian3d::prelude::SpatialQuery,
) -> f32 {
    if let Some(map_tree) = map_tree {
        query_point_ground_y(pos.x, pos.z, map_tree, spatial_query)
    } else {
        // Fallback for binary: downward raycast from high up
        let start_y = 50.0;
        let ray_origin = Vec3::new(pos.x, start_y, pos.z);
        let filter = avian3d::prelude::SpatialQueryFilter::default();
        if let Some(hit) = spatial_query.cast_ray(
            ray_origin,
            bevy::prelude::Dir3::NEG_Y,
            100.0,
            true,
            &filter,
        ) {
            start_y - hit.distance
        } else {
            0.5
        }
    }
}

pub fn traffic_network_spawner(
    time: Res<Time>,
    mut last_spawn: Local<f32>,
    config: Res<TrafficConfig>,
    graph: Res<TrafficRoadGraph>,
    q_camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    q_traffic: Query<(), With<TrafficCar>>,
    q_all_cars: Query<&Transform, With<Car>>,
    mut commands: Commands,
) {
    if !config.enabled || !graph.built {
        return;
    }

    let now = time.elapsed_secs();
    if now - *last_spawn < 0.1 {
        return;
    }

    if q_traffic.iter().count() >= config.max_cars {
        return;
    }

    let Some((camera, cam_gt)) = q_camera.iter().next() else {
        return;
    };

    let camera_pos = cam_gt.translation();
    let num_segments = graph.segments.len();
    if num_segments == 0 {
        return;
    }

    // Try up to 10 candidates
    for _ in 0..10 {
        let seg_idx = (rand::random::<f32>() * num_segments as f32) as usize;
        let seg = &graph.segments[seg_idx];
        if seg.points.is_empty() {
            continue;
        }
        let pt_idx = (rand::random::<f32>() * seg.points.len() as f32) as usize;
        let candidate_point = seg.points[pt_idx];

        let dist = camera_pos.distance(candidate_point);
        if dist > config.spawn_radius || dist < 20.0 {
            continue;
        }

        // Check if inside frustum
        if let Some(ndc) = camera.world_to_ndc(cam_gt, candidate_point) {
            let inside_x = ndc.x >= -1.0 && ndc.x <= 1.0;
            let inside_y = ndc.y >= -1.0 && ndc.y <= 1.0;
            let inside_z = ndc.z >= 0.0 && ndc.z <= 1.0;
            if inside_x && inside_y && inside_z {
                // Reject visible candidate
                continue;
            }
        }

        // Check distance to existing cars
        let mut too_close = false;
        for car_tf in q_all_cars.iter() {
            if car_tf.translation.distance(candidate_point) < 8.0 {
                too_close = true;
                break;
            }
        }
        if too_close {
            continue;
        }

        // Success! Spawn it
        commands.trigger(SpawnTrafficCarEvent {
            position: candidate_point,
        });
        *last_spawn = now;
        break;
    }
}

pub fn spawn_traffic_car_observer(
    trigger: On<SpawnTrafficCarEvent>,
    mut commands: Commands,
    graph: Res<TrafficRoadGraph>,
    asset_server: Res<AssetServer>,
    map_tree: Option<Res<MapTree>>,
    spatial_query: avian3d::prelude::SpatialQuery,
    manifest: Option<Res<PedestrianManifest>>,
) {
    if !graph.built || graph.segments.is_empty() {
        warn!("SpawnTrafficCarEvent: road graph is not built or empty.");
        return;
    }

    let req_pos = trigger.event().position;

    // 1. Find closest road segment point
    let mut closest_pt = req_pos;
    let mut closest_dist = f32::MAX;
    let mut closest_seg_idx = 0;
    let mut closest_pt_idx = 0;

    for (s_idx, seg) in graph.segments.iter().enumerate() {
        for (p_idx, &pt) in seg.points.iter().enumerate() {
            let d = pt.distance(req_pos);
            if d < closest_dist {
                closest_dist = d;
                closest_pt = pt;
                closest_seg_idx = s_idx;
                closest_pt_idx = p_idx;
            }
        }
    }

    // 2. Build path from closest segment point in the direction of the longer side
    let seg = &graph.segments[closest_seg_idx];
    let mut forward_dist = 0.0;
    for w in seg.points[closest_pt_idx..].windows(2) {
        forward_dist += w[0].distance(w[1]);
    }
    let mut backward_dist = 0.0;
    for w in seg.points[..=closest_pt_idx].windows(2) {
        backward_dist += w[0].distance(w[1]);
    }
    let forward = forward_dist >= backward_dist;

    let mut path_points = if forward {
        seg.points[closest_pt_idx..].to_vec()
    } else {
        seg.points[..=closest_pt_idx].iter().cloned().rev().collect::<Vec<_>>()
    };

    // 3. OSM continuation: append one next segment
    if path_points.len() >= 2 {
        let end_node = quantize(*path_points.last().unwrap());
        if let Some(matching_segs) = graph.node_index.get(&end_node) {
            let mut next_seg_found = None;
            for &next_idx in matching_segs {
                if next_idx != closest_seg_idx {
                    next_seg_found = Some(next_idx);
                    break;
                }
            }
            if let Some(next_idx) = next_seg_found {
                let next_seg = &graph.segments[next_idx];
                if next_seg.points.len() >= 2 {
                    let start_quant = quantize(next_seg.points[0]);
                    let end_quant = quantize(*next_seg.points.last().unwrap());
                    if start_quant == end_node {
                        path_points.extend(next_seg.points[1..].iter().cloned());
                    } else if end_quant == end_node {
                        path_points.extend(next_seg.points[..next_seg.points.len() - 1].iter().cloned().rev());
                    }
                }
            }
        }
    }

    if path_points.len() < 2 {
        return;
    }

    // 4. Ground the spawn position
    let ground_y = get_ground_y(closest_pt, map_tree.as_ref().map(|r| &**r), &spatial_query);
    let car_spawn_pos = Vec3::new(closest_pt.x, ground_y + 1.5, closest_pt.z);

    // Rotation from path direction
    let dir = (path_points[1] - path_points[0]).normalize_or_zero();
    let car_rot = if dir != Vec3::ZERO {
        Quat::from_rotation_arc(Vec3::NEG_Z, dir)
    } else {
        Quat::IDENTITY
    };

    let car_type = get_random_car_type();
    let car_entity = spawn_physics_car(&mut commands, &asset_server, car_spawn_pos, car_rot, car_type);

    // Initial speed
    let init_speed = 30.0 / 3.6; // 30 km/h
    commands.entity(car_entity).insert((
        avian3d::prelude::LinearVelocity(dir * init_speed),
        TrafficCar {
            path: path_points,
            next_idx: 1,
            stuck_timer: 0.0,
            out_of_view_timer: 0.0,
            half_height: 0.5,
        },
    ));

    // 5. Spawn Pedestrian Driver
    if let Some(manifest) = manifest {
        if manifest.loaded && !manifest.urls.is_empty() {
            let url = manifest.urls.choose(&mut rand::rng()).cloned().unwrap();
            let scene_url = bevy::prelude::GltfAssetLabel::Scene(0).from_asset(url.0.clone());
            let handle = asset_server.load::<bevy::world_serialization::WorldAsset>(scene_url);
            let gltf_handle = asset_server.load::<bevy::gltf::Gltf>(url.0.clone());

            let model_name = url.0.split('/').last().unwrap_or(&url.0).replace(".glb", "");
            let seat = CarSeatOffset::default();

            commands.spawn((
                Name::new("TrafficDriver"),
                Transform::from_translation(seat.offset)
                    .with_rotation(Quat::from_rotation_y(seat.y_rot))
                    .with_scale(Vec3::splat(1.0)),
                Visibility::default(),
                InheritedVisibility::default(),
                ModelRoot {
                    index: 0,
                    name: model_name,
                    size: Vec3::ZERO,
                },
                PedestrianGltf {
                    handle: gltf_handle,
                },
                NeedAlignment,
                ManualAnimation,
                ChildOf(car_entity),
                DriverMesh {
                    car: car_entity,
                    anim_node: None,
                },
            ))
            .with_children(|parent| {
                parent.spawn((
                    bevy::world_serialization::WorldAssetRoot(handle),
                    Transform::IDENTITY,
                    Visibility::default(),
                    InheritedVisibility::default(),
                ));
            });
        }
    }
}
