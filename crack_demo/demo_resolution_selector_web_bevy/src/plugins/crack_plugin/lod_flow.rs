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

/// Smoothed world-space kinematics of the MainCamera, sampled every frame.
/// Feeds the velocity-predictive occlusion sampling in the worker.
#[derive(Resource, Default)]
pub struct CameraKinematics {
/// position field.
    pub position: Vec3,
/// velocity field.
    pub velocity: Vec3,
/// initialized field.
    pub initialized: bool,
}

/// track camera kinematics.
pub fn track_camera_kinematics(
    time: Res<Time>,
    q_camera: Query<
        &GlobalTransform,
        With<crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera>,
    >,
    mut kin: ResMut<CameraKinematics>,
) {
    let Some(cam) = q_camera.iter().next() else {
        return;
    };
    let pos = cam.translation();
    let dt = time.delta_secs();
    if !kin.initialized || dt <= 1e-6 {
        kin.position = pos;
        kin.velocity = Vec3::ZERO;
        kin.initialized = true;
        return;
    }
    let raw = (pos - kin.position) / dt;
    // Teleport guard: mode switches / respawns jump the camera; a bogus huge
    // velocity would poke sample origins through the whole map.
    if raw.length() > 500.0 {
        kin.velocity = Vec3::ZERO;
    } else {
        // EMA smoothing so a single jittery frame doesn't swing the lookahead.
        kin.velocity = kin.velocity.lerp(raw, 0.2);
    }
    kin.position = pos;
}

/// spawn lod task.
pub fn spawn_lod_task(
    map_tree: Res<MapTree>,
    lod_state: Res<MapLODState>,
    q_merge: Query<&TileShouldMerge>,
    q_split: Query<&TileShouldSplit>,
    q_pending: Query<&PendingTileReveal>,
    q_nodes: Query<&TreeMapTile>,
    mut last: Local<
        Option<(
            BTreeSet<MapTreeNodePath>,
            Vec<Vec3>,
            u32,
            bool,
            i32,
            (u32, u32),
            (u32, u32, u32),
        )>,
    >,
    q_camera: Query<
        &GlobalTransform,
        With<crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera>,
    >,
    res_tiles: Res<TileSwapRequests>,
    mut tasks: ResMut<CrackTasks>,
    client: Res<CrackClient>,
    control_state: Res<State<crate::plugins::states::GameControlState>>,
    kin: Res<CameraKinematics>,
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
        refs.push(camera.translation());
    }

    let quantize = |v: Vec3| {
        Vec3::new(
            (v.x / 2.0).round() * 2.0,
            (v.y / 2.0).round() * 2.0,
            (v.z / 2.0).round() * 2.0,
        )
    };
    let quantized_refs = refs.iter().map(|&v| quantize(v)).collect::<Vec<_>>();

    // The visibility-cull (BVH occluder) flag is part of the change key so toggling the debug
    // checkbox forces a fresh LOD recompute even when nothing else moved.
    let cull = lod_state.enable_visibility_cull;
    let max_lod = lod_state.max_lod;
    let tiles_per_diagonal_bits = (
        lod_state.min_tiles_per_diagonal.to_bits(),
        lod_state.max_tiles_per_diagonal.to_bits(),
    );
    let radius_bits = (
        lod_state.sample_radius_freecam.to_bits(),
        lod_state.sample_radius_car.to_bits(),
        lod_state.sample_radius_pedestrian.to_bits(),
    );
    if let Some(last_val) = &*last {
        if nodes == last_val.0
            && quantized_refs == last_val.1
            && budget == last_val.2
            && cull == last_val.3
            && max_lod == last_val.4
            && tiles_per_diagonal_bits == last_val.5
            && radius_bits == last_val.6
        {
            return;
        }
    }
    *last = Some((
        nodes.clone(),
        quantized_refs,
        budget,
        cull,
        max_lod,
        tiles_per_diagonal_bits,
        radius_bits,
    ));

    let sample_radius = match control_state.get() {
        crate::plugins::states::GameControlState::MapFreecam => lod_state.sample_radius_freecam,
        crate::plugins::states::GameControlState::DrivingCar => lod_state.sample_radius_car,
        crate::plugins::states::GameControlState::ControllingPedestrian => {
            lod_state.sample_radius_pedestrian
        }
    };

    let mut cameras = Vec::new();
    if let Some(camera) = q_camera.iter().next() {
        cameras.push(game_logic::lod::CameraReference {
            center: camera.translation(),
            velocity: kin.velocity,
            sample_radius,
        });
    }

    let args = game_logic::lod::LodComputeRequest {
        spawned_nodes: nodes,
        reference_points: refs,
        cameras,
        lod_budget: budget,
        max_lod,
        min_tiles_per_diagonal: lod_state.min_tiles_per_diagonal,
        max_tiles_per_diagonal: lod_state.max_tiles_per_diagonal,
        enable_visibility_cull: lod_state.enable_visibility_cull,
        base_url: crate::config::DATA_BASE_URL.to_string(),
    };

    let api_client = client.0.clone();
    let task = AsyncComputeTaskPool::get()
        .spawn(async move { api_client.call::<ComputeLodChanges>(args).await });
    tasks.lod = Some(task);
}

/// poll lod task.
pub fn poll_lod_task(mut tasks: ResMut<CrackTasks>, mut res_tiles: ResMut<TileSwapRequests>) {
    if let Some(mut task) = tasks.lod.take() {
        if let Some(res) = future::block_on(future::poll_once(&mut task)) {
            match res {
                Ok(response) => {
                    res_tiles.split_requests = response.split_requests;
                    res_tiles.merge_requests = response.merge_requests;
                    res_tiles.culled_nodes = response.culled_nodes;
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
