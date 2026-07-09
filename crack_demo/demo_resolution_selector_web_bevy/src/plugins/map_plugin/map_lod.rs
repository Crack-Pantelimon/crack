use crate::basic_app::MemoryDir;
use crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer;
use crate::plugins::map_plugin::{MapLODState, MapTileAssetId, MapTree, MapTreeNodePath};
use crate::plugins::states::{InitialMapLoadFinished, OsmDatabaseLoadFinished};
use avian3d::collision::collider::CollisionMargin;
use avian3d::prelude::CollisionLayers;
use bevy::prelude::*;
use bevy::tasks::futures_lite::future;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use std::collections::{BTreeSet, HashMap};

#[derive(Component)]
pub struct TreeMapTile {
    pub node_path: MapTreeNodePath,
    pub asset_id: MapTileAssetId,
}

fn spawn_node_tiles(
    commands: &mut Commands,
    assets: &[(
        MapTileAssetId,
        Handle<WorldAsset>,
        Option<avian3d::prelude::Collider>,
    )],
    node_path: &MapTreeNodePath,
    hidden: bool,
) -> Vec<Entity> {
    let visibility = if hidden {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    let mut spawned = Vec::with_capacity(assets.len());
    for (asset_id, handle, collider_opt) in assets {
        let mut entity_cmds = commands.spawn((
            WorldAssetRoot(handle.clone()),
            visibility,
            Transform::from_xyz(0.0, 0.0, 0.0),
            TreeMapTile {
                node_path: node_path.clone(),
                asset_id: asset_id.clone(),
            },
            avian3d::prelude::RigidBody::Static,
            CollisionMargin(0.2),
            avian3d::prelude::Restitution::ZERO
                .with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
            avian3d::prelude::Friction::new(0.9),
            CollisionLayers::new(
                [GamePhysicsLayer::Map],
                [
                    // GamePhysicsLayer::Map,
                    GamePhysicsLayer::Car,
                    GamePhysicsLayer::Wheel,
                ],
            ),
        ));
        if let Some(collider) = collider_opt {
            entity_cmds.insert(collider.clone());
        }
        spawned.push(entity_cmds.id());
    }
    spawned
}

pub fn spawn_root_map_tiles(
    mut commands: Commands,
    data_res: Res<MapTree>,
    client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,
) {
    let Some(client) = client else {
        return;
    };
    if !data_res.is_changed() {
        return;
    }
    if !data_res.parsed {
        return;
    }

    let mut tasks = Vec::new();
    let mut asset_ids = Vec::new();
    let mut asset_to_node = HashMap::new();

    for root in &data_res.roots {
        for asset in &root.assets {
            let api_client = client.0.clone();
            let base_url = crate::config::DATA_BASE_URL.to_string();
            let glb_path = asset.glb_path.clone();
            let tile_id = asset.name.0.clone();
            let asset_id = asset.name.clone();

            let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
                api_client
                    .call::<game_logic::api::FetchMapTile>(game_logic::tile::FetchTileRequest {
                        base_url,
                        glb_path,
                        tile_id,
                    })
                    .await
            });
            tasks.push(Some(task));
            asset_ids.push(asset_id.clone());
            asset_to_node.insert(asset_id, root.path.clone());
        }
    }

    if !tasks.is_empty() {
        commands.spawn(PendingTileGroupFetch {
            purpose: TileGroupFetchPurpose::Root { asset_to_node },
            tasks,
            asset_ids,
            results: Vec::new(),
        });
    }
}

#[derive(Component, Debug)]
pub struct TileShouldMerge {
    pub drop_children: BTreeSet<MapTreeNodePath>,
    pub load_parent: (
        MapTreeNodePath,
        Vec<(
            MapTileAssetId,
            Handle<WorldAsset>,
            Option<avian3d::prelude::Collider>,
        )>,
    ),
}

#[derive(Component, Debug)]
pub struct TileShouldSplit {
    pub load_children: Vec<(
        MapTreeNodePath,
        Vec<(
            MapTileAssetId,
            Handle<WorldAsset>,
            Option<avian3d::prelude::Collider>,
        )>,
    )>,
    pub drop_parent: MapTreeNodePath,
}

#[derive(Resource, Default)]
pub struct TileSwapRequests {
    pub split_requests: Vec<game_logic::lod::SplitRequestSummary>,
    pub merge_requests: Vec<game_logic::lod::MergeRequestSummary>,
}

const TILE_REVEAL_DELAY_FRAMES: u8 = 3;

#[derive(Component)]
pub struct PendingTileReveal {
    pub new_tiles: Vec<Entity>,
    pub drop_parent: Option<MapTreeNodePath>,
    pub drop_descendants_of: Vec<MapTreeNodePath>,
    pub countdown: u8,
}

#[derive(Debug, Clone)]
pub enum TileGroupFetchPurpose {
    Root {
        asset_to_node: HashMap<MapTileAssetId, MapTreeNodePath>,
    },
    Split {
        split_summary: game_logic::lod::SplitRequestSummary,
    },
    Merge {
        drop_children: BTreeSet<MapTreeNodePath>,
        parent_path: MapTreeNodePath,
        merge_summary: game_logic::lod::MergeRequestSummary,
    },
}

#[derive(Component)]
pub struct PendingTileGroupFetch {
    pub purpose: TileGroupFetchPurpose,
    pub tasks: Vec<Option<bevy::tasks::Task<anyhow::Result<game_logic::tile::FetchTileResponse>>>>,
    pub asset_ids: Vec<MapTileAssetId>,
    pub results: Vec<(MapTileAssetId, game_logic::tile::FetchTileResponse)>,
}

pub fn start_tile_swap_requests(
    mut commands: Commands,
    mut res_tiles: ResMut<TileSwapRequests>,
    client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,
    q_split: Query<&TileShouldSplit>,
    q_merge: Query<&TileShouldMerge>,
    q_fetch: Query<&PendingTileGroupFetch>,
) {
    let Some(client) = client else {
        return;
    };
    if res_tiles.merge_requests.is_empty() && res_tiles.split_requests.is_empty() {
        return;
    }

    const PARALLEL_SPLIT_FETCH: i32 = 12;
    const PARALLEL_MERGE_FETCH: i32 = 8;
    let current_splits = q_split.iter().len() as i32;
    let current_merges = q_merge.iter().len() as i32;
    let current_fetches = q_fetch.iter().len() as i32;
    let mut split_budget = PARALLEL_SPLIT_FETCH - (current_splits + current_fetches);
    let mut merge_budget = PARALLEL_MERGE_FETCH - (current_merges + current_fetches);

    let mut split_done = Vec::new();
    for split in &res_tiles.split_requests {
        if split_budget <= 0 {
            break;
        }
        split_budget -= 1;

        let mut tasks = Vec::new();
        let mut asset_ids = Vec::new();
        for child in &split.children {
            for asset in &child.assets {
                let api_client = client.0.clone();
                let base_url = crate::config::DATA_BASE_URL.to_string();
                let glb_path = asset.glb_path.clone();
                let tile_id = asset.name.0.clone();
                let asset_id = asset.name.clone();

                let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
                    api_client
                        .call::<game_logic::api::FetchMapTile>(game_logic::tile::FetchTileRequest {
                            base_url,
                            glb_path,
                            tile_id,
                        })
                        .await
                });
                tasks.push(Some(task));
                asset_ids.push(asset_id);
            }
        }

        commands.spawn(PendingTileGroupFetch {
            purpose: TileGroupFetchPurpose::Split {
                split_summary: split.clone(),
            },
            tasks,
            asset_ids,
            results: Vec::new(),
        });
        split_done.push(split.parent_path.clone());
    }

    let mut merge_done = Vec::new();
    for merge in &res_tiles.merge_requests {
        if merge_budget <= 0 {
            break;
        }
        merge_budget -= 1;

        let mut tasks = Vec::new();
        let mut asset_ids = Vec::new();
        for asset in &merge.parent_assets {
            let api_client = client.0.clone();
            let base_url = crate::config::DATA_BASE_URL.to_string();
            let glb_path = asset.glb_path.clone();
            let tile_id = asset.name.0.clone();
            let asset_id = asset.name.clone();

            let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
                api_client
                    .call::<game_logic::api::FetchMapTile>(game_logic::tile::FetchTileRequest {
                        base_url,
                        glb_path,
                        tile_id,
                    })
                    .await
            });
            tasks.push(Some(task));
            asset_ids.push(asset_id);
        }

        commands.spawn(PendingTileGroupFetch {
            purpose: TileGroupFetchPurpose::Merge {
                drop_children: merge.drop_children.clone(),
                parent_path: merge.parent_path.clone(),
                merge_summary: merge.clone(),
            },
            tasks,
            asset_ids,
            results: Vec::new(),
        });

        merge_done.push(merge.parent_path.clone());
    }

    res_tiles
        .split_requests
        .retain(|x| !split_done.contains(&x.parent_path));
    res_tiles
        .merge_requests
        .retain(|x| !merge_done.contains(&x.parent_path));
}

pub fn poll_tile_group_fetches(
    mut commands: Commands,
    mut q_fetches: Query<(Entity, &mut PendingTileGroupFetch)>,
    memory_dir: ResMut<MemoryDir>,
    asset_server: Res<AssetServer>,
) {
    for (entity, mut fetch) in q_fetches.iter_mut() {
        for i in 0..fetch.tasks.len() {
            if let Some(mut task) = fetch.tasks[i].take() {
                if let Some(res) = future::block_on(future::poll_once(&mut task)) {
                    match res {
                        Ok(response) => {
                            let asset_id = fetch.asset_ids[i].clone();
                            fetch.results.push((asset_id, response));
                        }
                        Err(e) => {
                            tracing::error!("Tile fetch worker RPC error: {e:?}");
                        }
                    }
                } else {
                    fetch.tasks[i] = Some(task);
                }
            }
        }

        let all_done = fetch.tasks.iter().all(|t| t.is_none());
        if all_done {
            let mut loaded_assets = Vec::new();
            for (asset_id, response) in &fetch.results {
                let sanitized_id = response
                    .tile_id
                    .replace('/', "_")
                    .replace('\\', "_")
                    .replace('.', "_");
                let memory_path = format!("{}.glb", sanitized_id);

                memory_dir.dir.insert_asset(
                    std::path::Path::new(&memory_path),
                    response.glb_bytes.clone(),
                );

                let asset_path =
                    GltfAssetLabel::Scene(0).from_asset(format!("memory://{}", memory_path));
                let handle = asset_server.load(asset_path);

                let mut collider_opt = None;
                if let Some(mesh_data) = &response.collider_mesh {
                    let vertices: Vec<Vec3> = mesh_data
                        .vertices
                        .iter()
                        .map(|v| Vec3::new(v[0], v[1], v[2]))
                        .collect();
                    if let Ok(trimesh) =
                        avian3d::prelude::Collider::try_trimesh(vertices, mesh_data.indices.clone())
                    {
                        collider_opt = Some(trimesh);
                    } else {
                        tracing::warn!(
                            "Failed to build trimesh collider for tile {}",
                            response.tile_id
                        );
                    }
                }

                loaded_assets.push((asset_id.clone(), handle, collider_opt));
            }

            match &fetch.purpose {
                TileGroupFetchPurpose::Root { asset_to_node } => {
                    let mut node_to_assets = HashMap::new();
                    for (asset_id, handle, collider) in loaded_assets {
                        if let Some(node_path) = asset_to_node.get(&asset_id) {
                            node_to_assets
                                .entry(node_path.clone())
                                .or_insert_with(Vec::new)
                                .push((asset_id, handle, collider));
                        }
                    }

                    let mut new_tiles = Vec::new();
                    for (node_path, assets) in node_to_assets {
                        new_tiles.extend(spawn_node_tiles(
                            &mut commands,
                            &assets,
                            &node_path,
                            true,
                        ));
                    }

                    if !new_tiles.is_empty() {
                        commands.spawn(PendingTileReveal {
                            new_tiles,
                            drop_parent: None,
                            drop_descendants_of: Vec::new(),
                            countdown: TILE_REVEAL_DELAY_FRAMES,
                        });
                    }
                }
                TileGroupFetchPurpose::Split { split_summary } => {
                    let mut children_data = Vec::new();
                    for child_summary in &split_summary.children {
                        let mut child_assets = Vec::new();
                        for (asset_id, handle, collider) in &loaded_assets {
                            if child_summary.assets.iter().any(|a| &a.name == asset_id) {
                                child_assets.push((
                                    asset_id.clone(),
                                    handle.clone(),
                                    collider.clone(),
                                ));
                            }
                        }
                        children_data.push((child_summary.path.clone(), child_assets));
                    }

                    commands.spawn(TileShouldSplit {
                        load_children: children_data,
                        drop_parent: split_summary.parent_path.clone(),
                    });
                }
                TileGroupFetchPurpose::Merge {
                    drop_children,
                    parent_path,
                    merge_summary: _,
                } => {
                    commands.spawn(TileShouldMerge {
                        drop_children: drop_children.clone(),
                        load_parent: (parent_path.clone(), loaded_assets),
                    });
                }
            }

            commands.entity(entity).despawn();
        }
    }
}

const SPLIT_PER_FRAME: usize = 1;
const MERGE_PER_FRAME: usize = 1;

pub fn do_split_requests(
    mut commands: Commands,
    q_split: Query<(&TileShouldSplit, Entity)>,
    asset_server: Res<AssetServer>,
) {
    let mut split_finished = vec![];

    let mut k = 0;
    for (split_req, _req_ent) in q_split.iter() {
        let assets_ready = split_req.load_children.iter().all(|x| {
            x.1.iter().all(|(_, handle, _)| {
                matches!(
                    asset_server.get_load_state(handle),
                    Some(bevy::asset::LoadState::Loaded)
                )
            })
        });

        if assets_ready {
            split_finished.push(split_req);
            commands.entity(_req_ent).despawn();
            k += 1;
            if k >= SPLIT_PER_FRAME {
                break;
            }
        }

        let asset_errors = split_req
            .load_children
            .iter()
            .flat_map(|x| {
                x.1.iter()
                    .filter_map(|(_, handle, _)| match asset_server.get_load_state(handle) {
                        Some(bevy::asset::LoadState::Failed(_e)) => Some(_e),
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        if !asset_errors.is_empty() {
            for item in asset_errors.iter() {
                tracing::error!(
                    "Got Asset Loading error on Map Tile Split! {:?} {:?}",
                    split_req,
                    item
                );
            }
        }
    }
    for split_req in split_finished {
        let mut new_tiles = Vec::new();
        for (child_path, child_assets) in split_req.load_children.iter() {
            new_tiles.extend(spawn_node_tiles(
                &mut commands,
                child_assets,
                child_path,
                true,
            ));
        }
        commands.spawn(PendingTileReveal {
            new_tiles,
            drop_parent: Some(split_req.drop_parent.clone()),
            drop_descendants_of: Vec::new(),
            countdown: TILE_REVEAL_DELAY_FRAMES,
        });
    }
}

pub fn do_merge_requests(
    mut commands: Commands,
    q_merge: Query<(&TileShouldMerge, Entity)>,
    asset_server: Res<AssetServer>,
) {
    let mut merge_finished = vec![];

    let mut k = 0;
    for (merge_req, req_ent) in q_merge.iter() {
        let parent_ready = merge_req.load_parent.1.iter().all(|(_, handle, _)| {
            matches!(
                asset_server.get_load_state(handle),
                Some(bevy::asset::LoadState::Loaded)
            )
        });
        if parent_ready {
            merge_finished.push(merge_req);
            commands.entity(req_ent).despawn();
            k += 1;
            if k >= MERGE_PER_FRAME {
                break;
            }
        }
        let asset_errors = merge_req
            .load_parent
            .1
            .iter()
            .filter_map(|(_, handle, _)| match asset_server.get_load_state(handle) {
                Some(bevy::asset::LoadState::Failed(error)) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>();
        for error in asset_errors {
            tracing::error!(
                "Got Asset Loading error on Map Tile Merge! {:?} {:?}",
                merge_req,
                error
            );
        }
    }
    for merge_req in merge_finished {
        let new_tiles = spawn_node_tiles(
            &mut commands,
            &merge_req.load_parent.1,
            &merge_req.load_parent.0,
            true,
        );
        commands.spawn(PendingTileReveal {
            new_tiles,
            drop_parent: None,
            drop_descendants_of: merge_req.drop_children.iter().cloned().collect(),
            countdown: TILE_REVEAL_DELAY_FRAMES,
        });
    }
}

pub fn reveal_pending_tiles(
    mut commands: Commands,
    mut q_pending: Query<(Entity, &mut PendingTileReveal)>,
    mut q_vis: Query<&mut Visibility>,
    q_nodes: Query<(&TreeMapTile, Entity)>,
) {
    for (pending_ent, mut pending) in q_pending.iter_mut() {
        if pending.countdown > 0 {
            pending.countdown -= 1;
            continue;
        }

        for tile_ent in &pending.new_tiles {
            if let Ok(mut vis) = q_vis.get_mut(*tile_ent) {
                *vis = Visibility::Visible;
            }
        }

        if let Some(parent) = &pending.drop_parent {
            for (tile, ent) in q_nodes.iter() {
                if &tile.node_path == parent {
                    commands.entity(ent).despawn();
                }
            }
        }
        for drop in &pending.drop_descendants_of {
            for (tile, ent) in q_nodes.iter() {
                if tile.node_path.0.starts_with(&drop.0) {
                    commands.entity(ent).despawn();
                }
            }
        }

        commands.entity(pending_ent).despawn();
    }
}

pub fn check_map_loaded_status(
    tiles_query: Query<&TreeMapTile>,
    lod_state: Res<MapLODState>,
    loading_status: Option<ResMut<crate::plugins::geojson::GameLoadingStatus>>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<InitialMapLoadFinished>>,
    mut osm_state: ResMut<NextState<OsmDatabaseLoadFinished>>,
) {
    let Some(mut loading_status) = loading_status else {
        return;
    };
    if loading_status.map_loaded {
        return;
    }

    let loaded_count = tiles_query.iter().count();
    let target = 1 + (lod_state.lod_budget / 15) as usize;

    if loaded_count >= target && target > 0 {
        loading_status.map_loaded = true;
        commands.trigger(crate::plugins::notifications::NotificationEvent::MapLoaded);
        info!(
            "Initial map load complete: {} / {} tiles loaded.",
            loaded_count, target
        );
        next_state.set(InitialMapLoadFinished::Finished);
        osm_state.set(OsmDatabaseLoadFinished::MapFinished);
    }
}
