use crate::map::{BBox, MapTreeData, MapTreeNodePath};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
#[cfg(feature = "worker")]
use std::sync::Arc;
#[cfg(feature = "worker")]
use tokio::sync::RwLock;

#[derive(Clone, Copy, PartialEq)]
pub struct Score(pub f32);

impl Eq for Score {}
impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}
impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CameraReference {
    pub center: Vec3,
    pub max_range: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodComputeRequest {
    pub spawned_nodes: BTreeSet<MapTreeNodePath>,
    pub reference_points: Vec<Vec3>,
    pub cameras: Vec<CameraReference>,
    pub lod_budget: u32,
    pub max_lod: i32,
    pub tiles_per_diagonal: f32,
    pub enable_visibility_cull: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SplitRequestSummary {
    pub parent_path: MapTreeNodePath,
    pub children: Vec<crate::map::MapRootNodeSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeRequestSummary {
    pub parent_path: MapTreeNodePath,
    pub parent_assets: Vec<crate::map::MapTileAssetInfoSummary>,
    pub drop_children: BTreeSet<MapTreeNodePath>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodComputeResponse {
    pub split_requests: Vec<SplitRequestSummary>,
    pub merge_requests: Vec<MergeRequestSummary>,
}

#[inline]
pub fn compute_distance_to_aabb(bbox: &BBox, p: Vec3) -> f32 {
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

#[cfg(feature = "worker")]
static OCCLUDER_WORLD: RwLock<Option<(u64, Arc<crate::visibility::OccluderWorld>)>> = RwLock::const_new(None);

#[cfg(feature = "worker")]
fn hash_spawned_nodes(nodes: &BTreeSet<MapTreeNodePath>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    nodes.hash(&mut hasher);
    hasher.finish()
}

pub async fn compute_lod_changes(data_res: &MapTreeData, req: &LodComputeRequest) -> LodComputeResponse {
    let t0 = _crack_utils::get_timestamp_now_ms();

    #[cfg(feature = "worker")]
    let occluder_world = if req.enable_visibility_cull {
        let hash_key = hash_spawned_nodes(&req.spawned_nodes);
        let cached = {
            let guard = OCCLUDER_WORLD.read().await;
            if let Some((key, ref world)) = *guard {
                if key == hash_key {
                    Some(world.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(world) = cached {
            Some(world)
        } else {
            let t_start_bvh = _crack_utils::get_timestamp_now_ms();
            let world = Arc::new(crate::visibility::OccluderWorld::rebuild_bvh(&req.spawned_nodes, &data_res.coarse_assets).await);
            let elapsed = _crack_utils::get_timestamp_now_ms() - t_start_bvh;
            tracing::info!("Occluder BVH rebuilt in {} ms (leaves: {})", elapsed, world.heightfields.len());
            let mut guard = OCCLUDER_WORLD.write().await;
            *guard = Some((hash_key, world.clone()));
            Some(world)
        }
    } else {
        None
    };

    #[cfg(not(feature = "worker"))]
    let _occluder_world: Option<()> = None;

    let nodes = &req.spawned_nodes;
    let budget = req.lod_budget;
    let refs = &req.reference_points;

    let tile_bbox = |node_path: &MapTreeNodePath| {
        if let Some(node) = data_res.all_nodes.get(node_path) {
            node.bbox
        } else {
            tracing::warn!("Cannot find tile {:?}", node_path);
            BBox::default()
        }
    };

    let mut score_cache = BTreeMap::new();
    let mut visibility_cache = BTreeMap::new();
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
        let score = -distance / bbox_diagonal;
        score_cache.insert(node_path.clone(), score);
        score
    };

    let is_valid_split = |node_path: &MapTreeNodePath| -> bool {
        if node_path.0.len() as i32 > req.max_lod {
            return false;
        }
        let bbox = tile_bbox(node_path);
        let bbox_diagonal = bbox.min.distance(bbox.max).clamp(0.00001, 100000.0);
        let mut distance = f32::INFINITY;
        for point in refs.iter() {
            distance = distance.min(compute_distance_to_aabb(&bbox, *point));
        }
        distance += 0.01;

        let tile_value = bbox_diagonal / distance;
        if tile_value < 1.0 / (0.01 + req.tiles_per_diagonal) {
            return false;
        }
        true
    };

    let parents = data_res.roots.clone();
    let mut heap = BinaryHeap::new();
    let mut current_budget = 0;

    for p in parents.iter() {
        if let Some(node) = data_res.all_nodes.get(p) {
            current_budget += node.assets.len();
        }
    }

    let mut proposed_nodes = parents.clone();
    for p in parents.iter() {
        heap.push((Score(tile_score(p)), p.clone()));
    }

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

            let is_visible = if new_budget <= budget as usize && is_valid_split(&node_path) {
                if let Some(&vis) = visibility_cache.get(&node_path) {
                    vis
                } else {
                    #[cfg(feature = "worker")]
                    let vis = if let Some(ref world) = occluder_world {
                        let bbox = tile_bbox(&node_path);
                        world.is_node_visible(&bbox, &node_path, &req.cameras)
                    } else {
                        true
                    };

                    #[cfg(not(feature = "worker"))]
                    let vis = true;

                    visibility_cache.insert(node_path.clone(), vis);
                    vis
                }
            } else {
                false
            };

            if new_budget <= budget as usize && is_valid_split(&node_path) && is_visible {
                proposed_nodes.remove(&node_path);
                proposed_splits.insert(node_path.clone());
                current_budget = new_budget;
                for c in children {
                    heap.push((Score(tile_score(&c)), c.clone()));
                    proposed_nodes.insert(c.clone());
                }
            }
        }
    }

    let mut split_count = 0;
    let mut resolved_splits = Vec::new();
    for item in &proposed_splits {
        if nodes.contains(item) {
            split_count += 1;
            let mut children_summary = Vec::new();
            if let Some(child_paths) = data_res.children.get(item) {
                for cp in child_paths {
                    let mut assets_summary = Vec::new();
                    if let Some(node_info) = data_res.all_nodes.get(cp) {
                        for asset_id in &node_info.assets {
                            if let Some(asset_info) = data_res.assets.get(asset_id) {
                                if let Some(glb_path) = &asset_info.glb_path {
                                    assets_summary.push(crate::map::MapTileAssetInfoSummary {
                                        name: asset_id.clone(),
                                        glb_path: glb_path.clone(),
                                    });
                                }
                            }
                        }
                    }
                    children_summary.push(crate::map::MapRootNodeSummary {
                        path: cp.clone(),
                        assets: assets_summary,
                    });
                }
            }
            resolved_splits.push(SplitRequestSummary {
                parent_path: item.clone(),
                children: children_summary,
            });
        }
    }

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

    let mut rem = vec![];
    for a in &merge_requests {
        for b in &merge_requests {
            if Some(a.clone()) == b.get_parent() {
                rem.push(b.clone());
            }
        }
    }
    for b in rem {
        merge_requests.remove(&b);
    }

    let mut resolved_merges = Vec::new();
    for proposed in &merge_requests {
        let mut parent_assets = Vec::new();
        if let Some(node_info) = data_res.all_nodes.get(proposed) {
            for asset_id in &node_info.assets {
                if let Some(asset_info) = data_res.assets.get(asset_id) {
                    if let Some(glb_path) = &asset_info.glb_path {
                        parent_assets.push(crate::map::MapTileAssetInfoSummary {
                            name: asset_id.clone(),
                            glb_path: glb_path.clone(),
                        });
                    }
                }
            }
        }
        let drop_children = data_res.children.get(proposed).cloned().unwrap_or_default();
        resolved_merges.push(MergeRequestSummary {
            parent_path: proposed.clone(),
            parent_assets,
            drop_children,
        });
    }

    let t1 = _crack_utils::get_timestamp_now_ms();
    let dt = t1 - t0;
    if dt > 12 {
        tracing::info!(
            "{} split requests / {} merge requests. compute_lod_changes took {} ms",
            split_count,
            resolved_merges.len(),
            dt
        );
    }

    LodComputeResponse {
        split_requests: resolved_splits,
        merge_requests: resolved_merges,
    }
}
