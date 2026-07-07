use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::plugins::{
    cars_driving::car_info::get_random_car_type,
    cars_driving::driving_plugin::spawn_car::{Car, spawn_physics_car},
    geojson::query_point_ground_y,
    map_plugin::MapTree,
    pedestrian_ai::faction::{DEFAULT_HP, Faction, Health},
    pedestrians::pedestrian_controller_plugin::{CarSeatOffset, DriverMesh},
    pedestrians::{
        ManualAnimation, ModelRoot, PedestrianManifest,
        spawn_pedestrian::{NeedAlignment, PedestrianGltf},
    },
};

use super::road_graph::TrafficRoadGraph;
use super::{
    CAR_SPAWN_SPACING, SPAWN_INTERVAL_S, SPAWN_MIN_CAMERA_DIST, SpawnTrafficCarEvent, TrafficCar,
    TrafficConfig,
};

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
        if let Some(hit) =
            spatial_query.cast_ray(ray_origin, bevy::prelude::Dir3::NEG_Y, 100.0, true, &filter)
        {
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

    let count = q_traffic.iter().count();
    if count >= config.max_cars {
        return;
    }

    let fast_fill = count < (config.max_cars as f32 * super::FAST_FILL_FRACTION) as usize;
    let now = time.elapsed_secs();
    if !fast_fill && now - *last_spawn < SPAWN_INTERVAL_S {
        return;
    }

    let Some((camera, cam_gt)) = q_camera.iter().next() else {
        return;
    };

    let existing = q_all_cars
        .iter()
        .map(|tf| tf.translation)
        .collect::<Vec<_>>();

    if let Some(candidate_point) = super::common::pick_spawn_candidate(
        &graph,
        camera,
        cam_gt,
        config.spawn_radius,
        SPAWN_MIN_CAMERA_DIST,
        CAR_SPAWN_SPACING,
        &existing,
        fast_fill,
    ) {
        commands.trigger(SpawnTrafficCarEvent {
            position: candidate_point,
        });
        if !fast_fill {
            *last_spawn = now;
        }
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

    let Some((closest_seg_idx, path_points)) = super::common::build_path_from(&graph, req_pos)
    else {
        return;
    };

    let closest_pt = path_points[0];

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
    let car_entity = spawn_physics_car(
        &mut commands,
        &asset_server,
        car_spawn_pos,
        car_rot,
        car_type,
    );

    // Initial speed
    let init_speed = 30.0 / 3.6; // 30 km/h
    commands.entity(car_entity).insert((
        avian3d::prelude::LinearVelocity(dir * init_speed),
        TrafficCar {
            state: super::common::TrafficAgentState::new(path_points, closest_seg_idx),
            half_height: 0.5,
            mode: super::TrafficDriveMode::Normal,
        },
    ));

    // 5. Spawn Pedestrian Driver
    if let Some(manifest) = manifest {
        if manifest.loaded && !manifest.urls.is_empty() {
            let url = manifest.urls.choose(&mut rand::rng()).cloned().unwrap();
            let scene_url = bevy::prelude::GltfAssetLabel::Scene(0).from_asset(url.0.clone());
            let handle = asset_server.load::<bevy::world_serialization::WorldAsset>(scene_url);
            let gltf_handle = asset_server.load::<bevy::gltf::Gltf>(url.0.clone());

            let model_name = url
                .0
                .split('/')
                .last()
                .unwrap_or(&url.0)
                .replace(".glb", "");
            let seat = CarSeatOffset::default();

            let faction = match rand::random::<u32>() % 5 {
                0 => Faction::Neutral,
                1 => Faction::Red,
                2 => Faction::Green,
                3 => Faction::Blue,
                _ => Faction::Yellow,
            };

            commands
                .spawn((
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
                    faction,
                    Health::full(DEFAULT_HP),
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
