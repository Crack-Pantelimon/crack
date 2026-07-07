use crate::map::{BBox, MapTreeData, MapTreeNodePath};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Score(pub f32);

impl Eq for Score {}
impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodComputeRequest {
    pub spawned_nodes: BTreeSet<MapTreeNodePath>,
    pub reference_points: Vec<Vec3>,
    pub lod_budget: u32,
    pub max_lod: i32,
    pub tiles_per_diagonal: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodComputeResponse {
    pub split_requests: BTreeSet<MapTreeNodePath>,
    pub merge_requests: BTreeSet<MapTreeNodePath>,
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

pub fn compute_lod_changes(data_res: &MapTreeData, req: &LodComputeRequest) -> LodComputeResponse {
    let t0 = _crack_utils::get_timestamp_now_ms();

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
            if new_budget <= budget as usize && is_valid_split(&node_path) {
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

    let mut split_requests = BTreeSet::new();
    for item in &proposed_splits {
        if nodes.contains(item) {
            split_requests.insert(item.clone());
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
    for a in merge_requests.iter() {
        for b in merge_requests.iter() {
            if Some(a.clone()) == b.get_parent() {
                rem.push(b.clone());
            }
        }
    }
    for b in rem {
        merge_requests.remove(&b);
    }

    let t1 = _crack_utils::get_timestamp_now_ms();
    let dt = t1 - t0;
    if dt > 12 {
        tracing::info!(
            "{} split requests / {} merge requests. compute_lod_changes took {} ms",
            split_requests.len(),
            merge_requests.len(),
            dt
        );
    }

    LodComputeResponse {
        split_requests,
        merge_requests,
    }
}
