use crate::plugins::map_plugin::{BBox, MapLODState, MapTileAssetId, MapTree, MapTreeNodePath};
use crate::plugins::states::InitialMapLoadFinished;
use _crack_utils::get_timestamp_now_ms;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use bevy_egui::egui::emath::OrderedFloat;
use std::collections::BTreeMap;
use std::collections::{BTreeSet, BinaryHeap};

#[derive(Component)]
pub struct TreeMapTile {
    pub node_path: MapTreeNodePath,
    pub asset_id: MapTileAssetId,
}

fn get_node_assets_and_handles(
    data_res: &Res<MapTree>,
    asset_server: &Res<AssetServer>,
    node_path: &MapTreeNodePath,
) -> Vec<(MapTileAssetId, Handle<WorldAsset>)> {
    let Some(node) = data_res.all_nodes.get(node_path) else {
        tracing::warn!("Node {:?} not found in data_res.nodes", node_path);
        return Vec::new();
    };
    let mut assets_and_handles = Vec::new();
    for asset_id in &node.assets {
        let Some(asset_info) = data_res.assets.get(asset_id) else {
            continue;
        };
        let Some(ref glb_path) = asset_info.glb_path else {
            continue;
        };

        let glb_url = format!("{}/3d_data_v2/{}", crate::config::DATA_BASE_URL, glb_path);
        let asset_path = GltfAssetLabel::Scene(0).from_asset(glb_url);
        assets_and_handles.push((asset_id.clone(), asset_server.load(asset_path)));
    }
    assets_and_handles
}

fn spawn_node_tiles(
    commands: &mut Commands,
    assets: &[(MapTileAssetId, Handle<WorldAsset>)],
    node_path: &MapTreeNodePath,
) {
    // tracing::info!(
    //     "spawn_node_tiles({:?}, assets count: {})",
    //     node_path,
    //     assets.len()
    // );
    for (asset_id, handle) in assets {
        commands.spawn((
            WorldAssetRoot(handle.clone()),
            Transform::from_xyz(0.0, 0.0, 0.0),
            TreeMapTile {
                node_path: node_path.clone(),
                asset_id: asset_id.clone(),
            },
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::ColliderConstructorHierarchy::new(
                avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
            ),
            avian3d::prelude::Restitution::ZERO
                .with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
        ));
    }
}

pub fn spawn_root_map_tiles(
    mut commands: Commands,
    data_res: Res<MapTree>,
    asset_server: Res<AssetServer>,
) {
    if !data_res.is_changed() {
        return;
    }
    if !data_res.parsed {
        return;
    }
    for node_path in data_res.roots.iter() {
        let assets_and_handles = get_node_assets_and_handles(&data_res, &asset_server, node_path);
        spawn_node_tiles(&mut commands, &assets_and_handles, node_path);
    }
}

#[inline]
fn compute_distance_to_aabb(bbox: &BBox, p: Vec3) -> f32 {
    let cx =
        p.x.clamp(bbox.min.x.min(bbox.max.x), bbox.min.x.max(bbox.max.x));
    let cy =
        p.y.clamp(bbox.min.y.min(bbox.max.y), bbox.min.y.max(bbox.max.y));
    let cz =
        p.z.clamp(bbox.min.z.min(bbox.max.z), bbox.min.z.max(bbox.max.z));
    let d1 = p.distance(Vec3::new(cx, cy, cz));
    let middle = Vec3::new(
        (bbox.min.x + bbox.max.x) / 2.0,
        (bbox.min.y + bbox.max.y) / 2.0,
        (bbox.min.z + bbox.max.z) / 2.0,
    );
    d1 + p.distance(middle)
}

#[derive(Component, Debug)]
pub struct TileShouldMerge {
    pub drop_children: BTreeSet<MapTreeNodePath>,
    pub load_parent: (MapTreeNodePath, Vec<(MapTileAssetId, Handle<WorldAsset>)>),
}

#[derive(Component, Debug)]
pub struct TileShouldSplit {
    pub load_children: Vec<(MapTreeNodePath, Vec<(MapTileAssetId, Handle<WorldAsset>)>)>,
    pub drop_parent: MapTreeNodePath,
}

#[derive(Resource, Default)]
pub struct TileSwapRequests {
    pub split_requests: BTreeSet<MapTreeNodePath>,
    pub merge_requests: BTreeSet<MapTreeNodePath>,
}

pub fn recompute_lod_mark_changes(
    data_res: Res<MapTree>,
    lod_state: Res<MapLODState>,
    q_merge: Query<&TileShouldMerge>,
    q_split: Query<&TileShouldSplit>,
    q_nodes: Query<(&TreeMapTile, Entity)>,
    mut last: Local<Option<(BTreeSet<MapTreeNodePath>, Vec<Vec3>, u32)>>,
    q_camera: Query<&Transform, With<Camera3d>>,
    mut res_tiles: ResMut<TileSwapRequests>,
) {
    if !q_merge.is_empty()
        || !q_split.is_empty()
        || !res_tiles.merge_requests.is_empty()
        || !res_tiles.split_requests.is_empty()
    {
        return;
    }
    if data_res.all_nodes.is_empty() || q_nodes.is_empty() {
        return;
    }
    let t0 = get_timestamp_now_ms();
    let nodes = q_nodes
        .iter()
        .map(|x| x.0.node_path.clone())
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

    tracing::info!(
        "recompute_lod_mark_changes(nodes: {} , refs: {}, budget: {} ) .... ",
        nodes.len(),
        refs.len(),
        budget
    );

    let tile_bbox = |node_path: &MapTreeNodePath| {
        let Some(node) = data_res.all_nodes.get(node_path) else {
            tracing::warn!("Cannot find tile {:?}", node_path);
            return BBox::default();
        };
        node.bbox
    };
    let mut score_cache = BTreeMap::new();
    let mut tile_score = |node_path: &MapTreeNodePath| {
        if let Some(cached) = score_cache.get(node_path) {
            return *cached;
        }
        let bbox = tile_bbox(node_path);
        let bbox_diagonal = bbox.min.distance(bbox.max).clamp(0.00001, 100000.0);
        let mut distance = f32::INFINITY;
        for point in refs.iter() {
            distance = distance.min(compute_distance_to_aabb(&bbox, *point));
        }
        distance += 50.0;
        // negative, so it's max-score
        let score = -distance / bbox_diagonal;
        score_cache.insert(node_path.clone(), score);
        score
    };

    let parents = data_res.roots.clone();
    // let mut parents = BTreeSet::new();
    // for _path in nodes.iter() {
    //     if let Some(p) =data_res.parents.get(_path)  {
    //         parents.insert(p.clone());
    //     } else {
    //         parents.insert(_path.clone());
    //     }
    // }
    tracing::info!("restarting tree from {} parents", parents.len());

    // put all parents into the max-heap
    let mut heap = BinaryHeap::new();

    let mut current_budget = 0;
    for p in parents.iter() {
        if let Some(node) = data_res.all_nodes.get(p) {
            current_budget += node.assets.len();
        }
    }

    let mut proposed_nodes = parents.clone();
    for p in parents.iter() {
        heap.push((OrderedFloat(tile_score(&p)), p.clone()));
    }
    tracing::info!(
        "starting with {} items in heap, current budget: {}",
        heap.len(),
        current_budget
    );
    let mut proposed_splits = BTreeSet::new();
    while let Some((_score, node_path)) = heap.pop() {
        let children = data_res.children.get(&node_path);
        let children = match children {
            Some(c) => c.clone(),
            None => BTreeSet::new(),
        };

        if !children.is_empty() {
            let parent_cost = data_res
                .all_nodes
                .get(&node_path)
                .map(|n| n.assets.len())
                .unwrap_or(0);
            let mut children_cost = 0;
            for child_path in &children {
                children_cost += data_res
                    .all_nodes
                    .get(child_path)
                    .map(|n| n.assets.len())
                    .unwrap_or(0);
            }
            let new_budget = current_budget - parent_cost + children_cost;
            if new_budget <= budget as usize {
                proposed_nodes.remove(&node_path);
                proposed_splits.insert(node_path.clone());
                current_budget = new_budget;
                for c in children {
                    heap.push((OrderedFloat(tile_score(&c)), c.clone()));
                    proposed_nodes.insert(c.clone());
                }
            }
        }
    }

    tracing::info!(
        "After iterating heap, there are {} proposed nodes (budget used: {}) and {} proposed splits",
        proposed_nodes.len(),
        current_budget,
        proposed_splits.len()
    );

    // intersection of nodes and proposed_splits is the list of split requests we make.
    let mut split_requests = vec![];
    for item in &proposed_splits {
        if nodes.contains(item) {
            split_requests.push(item.clone());
        }
    }

    // A merge is needed for any proposed node that is not currently spawned,
    // but has descendants that are currently spawned.
    let mut merge_requests = BTreeSet::new();
    for proposed in &proposed_nodes {
        if !nodes.contains(proposed) {
            let has_spawned_descendants = nodes
                .iter()
                .any(|n| n.0.starts_with(&proposed.0) && n.0 != proposed.0);
            if has_spawned_descendants {
                merge_requests.insert(proposed.clone());
            }
        }
    }
    // merges need to only add the base ancestor, so if for any two merges we have Some(a) = b.get_parent() then we remove b.
    let mut _rem = vec![];
    for a in merge_requests.iter() {
        for b in merge_requests.iter() {
            if Some(a.clone()) == b.get_parent() {
                _rem.push(b.clone());
            }
        }
    }
    for b in _rem {
        merge_requests.remove(&b);
    }
    tracing::info!(
        "{} split requests / {} merge reuqests.",
        split_requests.len(),
        merge_requests.len()
    );

    res_tiles.split_requests = split_requests.into_iter().collect();
    res_tiles.merge_requests = merge_requests.into_iter().collect();
    let t1 = _crack_utils::get_timestamp_now_ms();
    let dt = t1 - t0;
    tracing::info!("recompute_lod_mark_changes took {} ms", dt);
}

pub fn start_tile_swap_requests(
    mut commands: Commands,
    mut res_tiles: ResMut<TileSwapRequests>,
    asset_server: Res<AssetServer>,
    q_split: Query<&TileShouldSplit>,
    q_merge: Query<&TileShouldMerge>,

    data_res: Res<MapTree>,
) {
    if res_tiles.merge_requests.is_empty() && res_tiles.split_requests.is_empty() {
        return;
    }

    const PARALLEL_SPLIT_FETCH: i32 = 3;
    const PARALLEL_MERGE_FETCH: i32 = 3;
    let current_splits = q_split.iter().len() as i32;
    let current_merges = q_merge.iter().len() as i32;
    let mut split_budget = PARALLEL_SPLIT_FETCH - current_splits;
    let mut merge_budget = PARALLEL_MERGE_FETCH - current_merges;

    let mut split_done = BTreeSet::new();
    for split in res_tiles.split_requests.iter() {
        if split_budget <= 0 {
            break;
        }
        split_budget -= 1;
        let children: Vec<_> = data_res
            .children
            .get(&split)
            .map(|x| x.iter().cloned().collect())
            .unwrap_or_default();
        let children = children
            .iter()
            .map(|x| {
                (
                    x.clone(),
                    get_node_assets_and_handles(&data_res, &asset_server, &x),
                )
            })
            .collect::<Vec<_>>();

        commands.spawn(TileShouldSplit {
            load_children: children,
            drop_parent: split.clone(),
        });
        split_done.insert(split.clone());
    }

    let mut merge_done = BTreeSet::new();
    for merge in res_tiles.merge_requests.iter() {
        if merge_budget <= 0 {
            break;
        }
        merge_budget -= 1;

        let drop_children = data_res.children.get(&merge).cloned().unwrap_or_default();

        // let drop_children = nodes
        //     .iter()
        //     .filter(|n| n.0.starts_with(&merge.0) && n.0 != merge.0)
        //     .cloned()
        //     .collect::<Vec<_>>();

        let parent_handles = get_node_assets_and_handles(&data_res, &asset_server, &merge);

        commands.spawn(TileShouldMerge {
            drop_children,
            load_parent: (merge.clone(), parent_handles),
        });

        merge_done.insert(merge.clone());
    }

    for item in split_done {
        res_tiles.split_requests.remove(&item);
    }
    for item in merge_done {
        res_tiles.merge_requests.remove(&item);
    }
}

const SPLIT_PER_FRAME: usize = 1;
const MERGE_PER_FRAME: usize = 1;

pub fn do_split_requests(
    mut commands: Commands,
    q_split: Query<(&TileShouldSplit, Entity)>,
    asset_server: Res<AssetServer>,
    q_nodes: Query<(&TreeMapTile, Entity), Without<TileShouldMerge>>,
) {
    let mut split_finished = vec![];

    let mut entity_map: BTreeMap<MapTreeNodePath, Vec<Entity>> = BTreeMap::new();
    for (tile, ent) in q_nodes.iter() {
        entity_map
            .entry(tile.node_path.clone())
            .or_default()
            .push(ent);
    }

    let mut k = 0;
    for (split_req, _req_ent) in q_split.iter() {
        let assets_ready = split_req.load_children.iter().all(|x| {
            x.1.iter().all(|(_, handle)| {
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
                    .filter_map(|(_, handle)| match asset_server.get_load_state(handle) {
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
        if let Some(split_entities) = entity_map.get(&split_req.drop_parent) {
            for entity in split_entities {
                commands.entity(*entity).despawn();
            }
        } else {
            tracing::warn!(
                "Split: Did not find parent entity to despawn: {:?}",
                split_req.drop_parent
            );
        }
        // let xxx: Vec<_> = split_req
        //     .load_children
        //     .iter()
        //     .map(|x| x.0.clone())
        //     .collect();
        // tracing::info!("XXX Split: {:?} -> {:?}", split_req.drop_parent, xxx);
        for (child_path, child_assets) in split_req.load_children.iter() {
            spawn_node_tiles(&mut commands, child_assets, child_path);
        }
    }
}

pub fn do_merge_requests(
    mut commands: Commands,
    q_merge: Query<(&TileShouldMerge, Entity)>,
    asset_server: Res<AssetServer>,
    q_nodes: Query<(&TreeMapTile, Entity), Without<TileShouldMerge>>,
) {
    let mut merge_finished = vec![];

    let mut entity_map: BTreeMap<MapTreeNodePath, Vec<Entity>> = BTreeMap::new();
    for (tile, ent) in q_nodes.iter() {
        entity_map
            .entry(tile.node_path.clone())
            .or_default()
            .push(ent);
    }

    let mut k = 0;
    for (merge_req, req_ent) in q_merge.iter() {
        let parent_ready = merge_req.load_parent.1.iter().all(|(_, handle)| {
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
            .filter_map(|(_, handle)| match asset_server.get_load_state(handle) {
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
    let mut drop_children_1 = BTreeSet::new();
    for merge_req in merge_finished {
        for child_path in merge_req.drop_children.iter() {
            drop_children_1.insert(child_path);

            // let descendant_paths = q_nodes.iter().map(|x| x.0.clone());
            // if let Some(child_entities) = entity_map.get(child_path) {
            //     for entity in child_entities {
            //         commands.entity(*entity).despawn();
            //     }
            // } else {
            //     tracing::warn!(
            //         "Merge: Did not find child entity to despawn: {:?}",
            //         child_path
            //     );
            // }
        }
        // tracing::info!(
        //     "XXX Merge: {:?} -> {:?}",
        //     merge_req.drop_children,
        //     merge_req.load_parent.0
        // );
        spawn_node_tiles(
            &mut commands,
            &merge_req.load_parent.1,
            &merge_req.load_parent.0,
        );
    }
    let mut drop_children2 = BTreeSet::new();
    for drop in drop_children_1 {
        for (node, node_ent) in q_nodes.iter() {
            if node.node_path.0.starts_with(&drop.0) {
                drop_children2.insert(node_ent);
            }
        }
    }
    for drop in drop_children2 {
        commands.entity(drop).despawn();
    }
}

pub fn check_map_loaded_status(
    tiles_query: Query<&TreeMapTile>,
    lod_state: Res<MapLODState>,
    loading_status: Option<ResMut<crate::plugins::geojson::GameLoadingStatus>>,
    tooltip_state: Option<ResMut<crate::plugins::geojson::TooltipNotificationState>>,
    mut next_state: ResMut<NextState<InitialMapLoadFinished>>,
) {
    let Some(mut loading_status) = loading_status else {
        return;
    };
    if loading_status.map_loaded {
        return;
    }

    let loaded_count = tiles_query.iter().count();
    let target = (lod_state.lod_budget / 2) as usize;

    if loaded_count >= target && target > 0 {
        loading_status.map_loaded = true;
        if let Some(mut tooltip_state) = tooltip_state {
            tooltip_state.map_loaded_timer = 3.0;
        }
        info!(
            "Initial map load complete: {} / {} tiles loaded.",
            loaded_count, target
        );
        next_state.set(InitialMapLoadFinished::Finished);
    }
}
