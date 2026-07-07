use crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer;
use crate::plugins::map_plugin::{BBox, MapLODState, MapTileAssetId, MapTree, MapTreeNodePath};
use crate::plugins::states::{InitialMapLoadFinished, OsmDatabaseLoadFinished};
use _crack_utils::get_timestamp_now_ms;
use avian3d::collision::collider::CollisionMargin;
use avian3d::prelude::CollisionLayers;
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
    hidden: bool,
) -> Vec<Entity> {
    // tracing::info!(
    //     "spawn_node_tiles({:?}, assets count: {})",
    //     node_path,
    //     assets.len()
    // );
    let visibility = if hidden {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    let mut spawned = Vec::with_capacity(assets.len());
    for (asset_id, handle) in assets {
        let entity = commands
            .spawn((
                WorldAssetRoot(handle.clone()),
                visibility,
                Transform::from_xyz(0.0, 0.0, 0.0),
                TreeMapTile {
                    node_path: node_path.clone(),
                    asset_id: asset_id.clone(),
                },
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::ColliderConstructorHierarchy::new(
                    avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
                    // avian3d::prelude::ColliderConstructor::ConvexDecompositionFromMesh,
                )
                .with_default_layers(CollisionLayers::new(
                    [GamePhysicsLayer::Map],
                    [
                        GamePhysicsLayer::Map,
                        GamePhysicsLayer::Car,
                        GamePhysicsLayer::Wheel,
                    ],
                )),
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
            ))
            .id();
        spawned.push(entity);
    }
    spawned
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
    let mut new_tiles = Vec::new();
    for node_path in data_res.roots.iter() {
        let assets_and_handles = get_node_assets_and_handles(&data_res, &asset_server, node_path);
        new_tiles.extend(spawn_node_tiles(
            &mut commands,
            &assets_and_handles,
            node_path,
            true,
        ));
    }
    // Root tiles have nothing to replace, but still defer their reveal so the browser never shows
    // the one-frame default-material flash on first appearance.
    if !new_tiles.is_empty() {
        commands.spawn(PendingTileReveal {
            new_tiles,
            drop_parent: None,
            drop_descendants_of: Vec::new(),
            countdown: TILE_REVEAL_DELAY_FRAMES,
        });
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
    (d1 + p.distance(middle)) / 2.0
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

/// Frames to keep a freshly-spawned tile hidden before revealing it and dropping the tile it
/// replaces. During this window the tile's GLB scene instantiates and the material/texture fix-up
/// systems run, so when it is finally shown it already has its matte material — no one-frame
/// default-material flash (the bug was only visible on the single-threaded web build). Because the
/// old tile stays alive until the new one is revealed, the swap is atomic: there is never a frame
/// with neither the old nor the new tile present.
const TILE_REVEAL_DELAY_FRAMES: u8 = 3;

/// A batch of freshly-spawned (hidden) tiles waiting to be revealed, along with the tiles they
/// replace (dropped atomically at reveal time). While any of these exist, `recompute_lod_mark_changes`
/// is blocked so no new swap is started mid-transition.
#[derive(Component)]
pub struct PendingTileReveal {
    /// Newly-spawned, currently-hidden tile entities to reveal.
    new_tiles: Vec<Entity>,
    /// Split case: the exact parent node path whose tiles are dropped on reveal.
    drop_parent: Option<MapTreeNodePath>,
    /// Merge case: descendant paths whose tiles are dropped on reveal.
    drop_descendants_of: Vec<MapTreeNodePath>,
    countdown: u8,
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
) {
    let mut split_finished = vec![];

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
        // Spawn the children hidden and defer the reveal. The parent tile stays visible until the
        // children are revealed (see `reveal_pending_tiles`), so the swap has no gap and no flash.
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
    for merge_req in merge_finished {
        // Spawn the merged parent hidden and defer the reveal. The child tiles stay visible until
        // the parent is revealed (see `reveal_pending_tiles`), so the swap has no gap and no flash.
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

/// Reveals hidden freshly-spawned tiles once their delay elapses, dropping the tiles they replace
/// in the same step so the swap is atomic (no gap, and the new tile's material is already fixed).
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

        // Reveal the new tiles.
        for tile_ent in &pending.new_tiles {
            if let Ok(mut vis) = q_vis.get_mut(*tile_ent) {
                *vis = Visibility::Visible;
            }
        }

        // Drop the tiles being replaced, atomically with the reveal.
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
    tooltip_state: Option<ResMut<crate::plugins::geojson::TooltipNotificationState>>,
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
        if let Some(mut tooltip_state) = tooltip_state {
            tooltip_state.map_loaded_timer = 3.0;
        }
        info!(
            "Initial map load complete: {} / {} tiles loaded.",
            loaded_count, target
        );
        next_state.set(InitialMapLoadFinished::Finished);
        osm_state.set(OsmDatabaseLoadFinished::MapFinished);
    }
}
