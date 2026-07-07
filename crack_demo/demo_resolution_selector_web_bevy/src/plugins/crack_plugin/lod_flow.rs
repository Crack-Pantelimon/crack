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

    if let Some(last_val) = &*last {
        if nodes == last_val.0 && refs == last_val.1 && budget == last_val.2 {
            return;
        }
    }
    *last = Some((nodes.clone(), refs.clone(), budget));

    let max_lod = lod_state.max_lod;
    let tiles_per_diagonal = lod_state.tiles_per_diagonal;

    let args = game_logic::lod::LodComputeRequest {
        spawned_nodes: nodes,
        reference_points: refs,
        lod_budget: budget,
        max_lod,
        tiles_per_diagonal,
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
