use crate::plugins::crack_plugin::{CrackClient, CrackTasks};
use crate::plugins::map_plugin::map_lod::{
    PendingTileReveal, TileShouldMerge, TileShouldSplit, TileSwapRequests, TreeMapTile,
};
use crate::plugins::map_plugin::{MapLODState, MapTree, MapTreeNodePath};
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::futures_lite::future;
use game_logic::api::ComputeLodChanges;
use std::collections::BTreeSet;

pub fn spawn_lod_task(
    map_tree: Res<MapTree>,
    lod_state: Res<MapLODState>,
    q_merge: Query<&TileShouldMerge>,
    q_split: Query<&TileShouldSplit>,
    q_pending: Query<&PendingTileReveal>,
    q_nodes: Query<&TreeMapTile>,
    mut last: Local<Option<(BTreeSet<MapTreeNodePath>, Vec<Vec3>, u32)>>,
    q_camera: Query<&Transform, With<Camera3d>>,
    res_tiles: Res<TileSwapRequests>,
    mut tasks: ResMut<CrackTasks>,
    client: Res<CrackClient>,
    camera_rig: Option<Res<crate::plugins::pedestrians::pedestrian_controller_plugin::CameraRig>>,
    q_vehicle: Query<&Transform, With<crate::plugins::cars_driving::driving_plugin::spawn_car::ActivePlayerVehicle>>,
    controlled_char: Option<Res<crate::plugins::pedestrians::pedestrian_controller_plugin::ControlledCharacter>>,
    q_character: Query<&Transform, With<crate::plugins::pedestrians::pedestrian_controller_plugin::CharacterController>>,
) {
    if tasks.lod.is_some() {
        return;
    }
    if !q_merge.is_empty()
        || !q_split.is_empty()
        || !q_pending.is_empty()
        || !res_tiles.merge_requests.is_empty()
        || !res_tiles.split_requests.is_empty()
    {
        return;
    }
    if !map_tree.parsed || q_nodes.is_empty() {
        return;
    }

    let nodes = q_nodes
        .iter()
        .map(|x| x.node_path.clone())
        .collect::<BTreeSet<_>>();

    let budget = lod_state.lod_budget;
    let mut refs = lod_state
        .reference_points
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(camera) = q_camera.iter().next() {
        refs.push(camera.translation);
    }

    let quantize = |v: Vec3| {
        Vec3::new(
            (v.x / 2.0).round() * 2.0,
            (v.y / 2.0).round() * 2.0,
            (v.z / 2.0).round() * 2.0,
        )
    };
    let quantized_refs = refs.iter().map(|&v| quantize(v)).collect::<Vec<_>>();

    if let Some(last_val) = &*last {
        if nodes == last_val.0 && quantized_refs == last_val.1 && budget == last_val.2 {
            return;
        }
    }
    *last = Some((nodes.clone(), quantized_refs, budget));

    // Calculate camera range/reachable radius based on active camera controller
    let mut camera_range = 32.0;
    let mut is_vehicle = false;
    if !q_vehicle.is_empty() {
        camera_range = 32.0;
        is_vehicle = true;
    } else if let Some(ref rig) = camera_rig {
        camera_range = rig.current_distance * 2.0;
    } else if let Some(camera_transform) = q_camera.iter().next() {
        let height = camera_transform.translation.y;
        let sprint_speed = height.clamp(5.0, 500.0) * 5.0;
        camera_range = sprint_speed * 2.0;
    }

    let mut cameras = Vec::new();
    if is_vehicle {
        if let Ok(car_transform) = q_vehicle.single() {
            cameras.push(game_logic::lod::CameraReference {
                center: car_transform.translation,
                max_range: camera_range,
            });
        }
    } else if let (Some(controlled), Some(_rig)) = (controlled_char.as_ref(), camera_rig.as_ref()) {
        if let Some(char_entity) = controlled.controller {
            if let Ok(char_transform) = q_character.get(char_entity) {
                cameras.push(game_logic::lod::CameraReference {
                    center: char_transform.translation,
                    max_range: camera_range,
                });
            }
        }
    } else if let Some(camera_transform) = q_camera.iter().next() {
        cameras.push(game_logic::lod::CameraReference {
            center: camera_transform.translation,
            max_range: camera_range,
        });
    }

    for &ref_pos in &lod_state.reference_points {
        cameras.push(game_logic::lod::CameraReference {
            center: ref_pos,
            max_range: 5.0,
        });
    }

    let max_lod = lod_state.max_lod;
    let tiles_per_diagonal = lod_state.tiles_per_diagonal;

    let args = game_logic::lod::LodComputeRequest {
        spawned_nodes: nodes,
        reference_points: refs,
        cameras,
        lod_budget: budget,
        max_lod,
        tiles_per_diagonal,
        enable_visibility_cull: lod_state.enable_visibility_cull,
    };

    let api_client = client.0.clone();
    let task = AsyncComputeTaskPool::get()
        .spawn(async move { api_client.call::<ComputeLodChanges>(args).await });
    tasks.lod = Some(task);
}

pub fn poll_lod_task(mut tasks: ResMut<CrackTasks>, mut res_tiles: ResMut<TileSwapRequests>) {
    if let Some(mut task) = tasks.lod.take() {
        if let Some(res) = future::block_on(future::poll_once(&mut task)) {
            match res {
                Ok(response) => {
                    res_tiles.split_requests = response.split_requests;
                    res_tiles.merge_requests = response.merge_requests;
                }
                Err(e) => {
                    tracing::error!("LOD RPC error: {e:?}");
                }
            }
        } else {
            tasks.lod = Some(task);
        }
    }
}
