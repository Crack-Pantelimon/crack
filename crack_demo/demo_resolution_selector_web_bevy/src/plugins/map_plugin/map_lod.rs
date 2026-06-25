use crate::plugins::map_plugin::{BBox, MapLODState, MapTree, MapTreeAssetInfo};
use _crack_utils::get_timestamp_now_ms;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use bevy_egui::egui::emath::OrderedFloat;
use std::collections::BTreeMap;
use std::collections::{BTreeSet, BinaryHeap};

#[derive(Component)]
pub struct TreeMapTile {
    pub node_name: String,
}

fn get_tile_handle(
    data_res: &Res<MapTree>,
    asset_server: &Res<AssetServer>,
    node_name: &str,
) -> Handle<WorldAsset> {
    let Some(node) = data_res.raw_nodes.get(node_name) else {
        tracing::warn!("Node {} not found in data_res.nodes", node_name);
        return Handle::default();
    };
    let Some(ref filename) = node.filename else {
        tracing::warn!("Node {} has no filename", node_name);
        return Handle::default();
    };

    let glb_url = format!("{}/3d_data/{}", crate::config::DATA_BASE_URL, filename);
    let asset_path = GltfAssetLabel::Scene(0).from_asset(glb_url);
    asset_server.load(asset_path)
}

fn spawn_tile_bundle(commands: &mut Commands, handle: &Handle<WorldAsset>, node_name: &str) {
    tracing::info!("spawn_tile_bundle({:?})", node_name);
    commands.spawn((
        WorldAssetRoot(handle.clone()),
        Transform::from_xyz(0.0, 0.0, 0.0),
        TreeMapTile {
            node_name: node_name.to_string(),
        },
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));
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
    for node_name in data_res.roots.iter() {
        let handle = get_tile_handle(&data_res, &asset_server, node_name);
        spawn_tile_bundle(&mut commands, &handle, node_name);
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
    pub drop_children: Vec<String>,
    pub load_parent: (String, Handle<WorldAsset>),
}

#[derive(Component, Debug)]
pub struct TileShouldSplit {
    pub load_children: Vec<(String, Handle<WorldAsset>)>,
    pub drop_parent: String,
}

pub fn recompute_lod_mark_changes(
    mut commands: Commands,
    data_res: Res<MapTree>,
    lod_state: Res<MapLODState>,
    q_merge: Query<&TileShouldMerge>,
    q_split: Query<&TileShouldSplit>,
    q_nodes: Query<(&TreeMapTile, Entity)>,
    mut last: Local<Option<(BTreeSet<String>, Vec<Vec3>, u32)>>,
    asset_server: Res<AssetServer>,
) {
    if !q_merge.is_empty() || !q_split.is_empty() {
        return;
    }
    if data_res.raw_nodes.is_empty() || lod_state.reference_points.is_empty() || q_nodes.is_empty() {
        return;
    }
    let t0 = get_timestamp_now_ms();
    let nodes = q_nodes
        .iter()
        .map(|x| x.0.node_name.clone())
        .collect::<BTreeSet<_>>();

    let budget = lod_state.lod_budget;
    let refs = lod_state
        .reference_points
        .iter()
        .cloned()
        .collect::<Vec<_>>();
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

    // let data_res2 = data_res.clone();
    let tile_bbox = |node_name: &str| {
        let Some(node) = data_res.raw_nodes.get(node_name) else {
            tracing::warn!("Cannot find tile {}", node_name);
            return BBox::default();
        };
        node.bbox
    };
    let mut score_cache = BTreeMap::new();
    let mut tile_score = |node_name: &str| {
        if let Some(cached) = score_cache.get(node_name) {
            return *cached;
        }
        let bbox = tile_bbox(node_name);
        // let center = (bbox.min + bbox.max) / 2.0;
        let bbox_diagonal = bbox.min.distance(bbox.max).clamp(0.00001, 100000.0);
        let mut distance = f32::INFINITY;
        for point in lod_state.reference_points.iter() {
            distance = distance.min(compute_distance_to_aabb(&bbox, *point));
        }
        // negative, so it's max-score
        let score = -distance / bbox_diagonal;
        score_cache.insert(node_name.to_string(), score);
        score
    };

    // get parent of all the nodes
    // let mut parents = BTreeSet::new();
    // for n in nodes.iter() {
    //     if let Some(p) = data_res.parents.get(n) {
    //         parents.insert(p.clone());
    //     } else {
    //         parents.insert(n.clone());
    //     }
    // }
    let parents = data_res.roots.clone();
    tracing::info!("restarting tree from {} parents", parents.len());

    // put all parents into the max-heap
    let mut heap = BinaryHeap::new();

    let mut proposed_nodes = parents.clone();
    for p in parents.iter() {
        heap.push((OrderedFloat(tile_score(&p)), p.clone()));
    }
    tracing::info!("starting with {} items in heap", heap.len());
    let mut proposed_splits = vec![];
    while proposed_nodes.len() < budget as usize {
        let Some((_score, node_name)) = heap.pop() else {
            break;
        };

        let children = data_res.children.get(&node_name);
        let children = match children {
            Some(c) => c.values().cloned().collect(),
            None => vec![],
        };

        if !children.is_empty() {
            proposed_nodes.remove(&node_name);
            proposed_splits.push(node_name.clone());
            for c in children {
                heap.push((OrderedFloat(tile_score(&c)), c.clone()));
                proposed_nodes.insert(c.clone());
            }
        }
    }

    tracing::info!(
        "After iterating heap, there are {} proposed nodes and {} proposed splits",
        proposed_nodes.len(),
        proposed_splits.len()
    );

    // intersection of nodes and proposed_splits is the list of split requests we make.
    let mut split_requests = vec![];
    for item in &proposed_splits {
        if nodes.contains(item) {
            split_requests.push(item.clone());
        }
    }

    // the list of parents that didn't split is the merge request we need to make.
    let mut merge_requests = vec![];
    for item in parents.iter() {
        if data_res.parents.contains_key(item) {
            if !proposed_splits.contains(item) {
                merge_requests.push(item.clone());
            }
        }
    }

    tracing::info!("Requesting split on {} items", split_requests.len());
    for split in split_requests {
        let children: Vec<_> = data_res
            .children
            .get(&split)
            .map(|x| x.values().cloned().collect())
            .unwrap_or_default();
        let children = children
            .iter()
            .map(|x| (x.clone(), get_tile_handle(&data_res, &asset_server, &x)))
            .collect::<Vec<_>>();

        commands.spawn(TileShouldSplit {
            load_children: children,
            drop_parent: split,
        });
    }

    tracing::info!("Requesting merge on {} items", merge_requests.len());
    for merge in merge_requests {
        let children_names: Vec<String> = data_res
            .children
            .get(&merge)
            .map(|x| x.values().cloned().collect())
            .unwrap_or_default();

        let mut drop_children = Vec::new();
        for child_name in children_names {
            drop_children.push(child_name);
        }

        let parent_handle = get_tile_handle(&data_res, &asset_server, &merge);

        commands.spawn(TileShouldMerge {
            drop_children,
            load_parent: (merge, parent_handle),
        });
    }

    let t1 = _crack_utils::get_timestamp_now_ms();
    let dt = t1 - t0;
    tracing::info!("recompute_lod_mark_changes took {} ms", dt);
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

    let entity_map = q_nodes
        .iter()
        .map(|x| (x.0.node_name.clone(), x.1))
        .collect::<BTreeMap<String, Entity>>();

    let mut k = 0;
    for (split_req, _req_ent) in q_split.iter() {
        let assets_ready = split_req.load_children.iter().all(|x| {
            matches!(
                asset_server.get_load_state(&x.1),
                Some(bevy::asset::LoadState::Loaded)
            )
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
            .filter_map(|x| match asset_server.get_load_state(&x.1) {
                Some(bevy::asset::LoadState::Failed(_e)) => Some(_e),
                _ => None,
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
        if let Some(split_entity) = entity_map.get(&split_req.drop_parent) {
            commands.entity(*split_entity).despawn();
        } else {
                tracing::warn!(
                    "Split: Did not find parent entity to despawn: {:?}",
                    split_req.drop_parent
                );
        }
        let xxx : Vec<_>= split_req.load_children.iter().map(|x| x.0.clone()).collect();
        tracing::info!("XXX Split: {:?} -> {:?}", split_req.drop_parent, xxx);
        for (child_name, child_handle) in split_req.load_children.iter() {
            spawn_tile_bundle(&mut commands, child_handle, child_name);
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

    let entity_map = q_nodes
        .iter()
        .map(|x| (x.0.node_name.clone(), x.1))
        .collect::<BTreeMap<String, Entity>>();

        let mut k = 0;
    for (merge_req, req_ent) in q_merge.iter() {
        let parent_ready = matches!(
            asset_server.get_load_state(&merge_req.load_parent.1),
            Some(bevy::asset::LoadState::Loaded)
        );
        if parent_ready {
            merge_finished.push(merge_req);
            commands.entity(req_ent).despawn();
            k += 1;
            if k >= MERGE_PER_FRAME {
                break;
            }
        }
        if let Some(error) = match asset_server.get_load_state(&merge_req.load_parent.1) {
            Some(bevy::asset::LoadState::Failed(error)) => Some(error),
            _ => None,
        } {
            tracing::error!(
                "Got Asset Loading error on Map Tile Merge! {:?} {:?}",
                merge_req,
                error
            );
        }
    }
    for merge_req in merge_finished {
        for child_name in merge_req.drop_children.iter() {
            if let Some(child_entity) = entity_map.get(child_name) {
                commands.entity(*child_entity).despawn();
            } else {
                tracing::warn!(
                    "Merge: Did not find child entity to despawn: {:?}",
                    child_name
                );
            }
        }
        tracing::info!("XXX Merge: {:?} -> {:?}", merge_req.drop_children, merge_req.load_parent.0);
        spawn_tile_bundle(
            &mut commands,
            &merge_req.load_parent.1,
            &merge_req.load_parent.0,
        );
    }
}
