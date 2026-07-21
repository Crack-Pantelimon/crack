use crate::map::{BBox, MapTreeData, MapTreeNodePath};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

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
    pub velocity: Vec3,
    pub sample_radius: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodComputeRequest {
    pub spawned_nodes: BTreeSet<MapTreeNodePath>,
    pub reference_points: Vec<Vec3>,
    pub cameras: Vec<CameraReference>,
    pub lod_budget: u32,
    pub max_lod: i32,
    /// Detail floor: nodes coarser than this level split unconditionally,
    /// without any visibility rays. Establishes the base map distribution.
    pub min_tiles_per_diagonal: f32,
    /// Detail ceiling: between min and max a node splits only if the BVH
    /// occluder test says it is visible from a camera.
    pub max_tiles_per_diagonal: f32,
    pub enable_visibility_cull: bool,
    pub base_url: String,
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
    pub bbox: BBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CulledNodeSummary {
    pub path: MapTreeNodePath,
    pub bbox: BBox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodComputeResponse {
    pub split_requests: Vec<SplitRequestSummary>,
    pub merge_requests: Vec<MergeRequestSummary>,
    pub culled_nodes: Vec<CulledNodeSummary>,
}

/// Where a node sits relative to the min/max tiles-per-diagonal detail band.
#[derive(Clone, Copy, PartialEq)]
enum SplitClass {
    /// Fine enough already — never split.
    No,
    /// Between min and max detail — split only when visible.
    IfVisible,
    /// Coarser than the detail floor — always split.
    Mandatory,
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
fn occluder_assets_of(data_res: &MapTreeData, path: &MapTreeNodePath) -> Vec<(String, String)> {
    let mut assets = Vec::new();
    if let Some(node) = data_res.all_nodes.get(path) {
        for asset_id in &node.assets {
            if let Some(asset_info) = data_res.assets.get(asset_id) {
                if let Some(glb_path) = &asset_info.glb_path {
                    assets.push((asset_id.0.clone(), glb_path.clone()));
                }
            }
        }
    }
    assets
}

#[cfg(feature = "worker")]
fn bbox_occluder_ok(bbox: &BBox) -> bool {
    let extent_x = (bbox.max.x - bbox.min.x).abs();
    let extent_z = (bbox.max.z - bbox.min.z).abs();
    extent_x >= 1e-3 && extent_z >= 1e-3
}

#[cfg(feature = "worker")]
async fn insert_occluders_chunked(
    world: &mut crate::visibility::OccluderWorld,
    entries: &[(MapTreeNodePath, BBox, Vec<(String, String)>)],
    base_url: &str,
) {
    for chunk in entries.chunks(8) {
        let futs = chunk.iter().map(|(path, _bbox, assets)| {
            crate::visibility::get_or_build_trimesh(path, assets, base_url)
        });
        let metas = futures::future::join_all(futs).await;
        for ((path, bbox, _), tm) in chunk.iter().zip(metas) {
            if let Some(tm) = tm {
                world.insert_occluder(path, bbox, tm);
            }
        }
    }
}

pub async fn compute_lod_changes(
    data_res: &MapTreeData,
    req: &LodComputeRequest,
) -> LodComputeResponse {
    let t0 = _crack_utils::get_timestamp_now_ms();
    let mut culled_nodes = Vec::new();

    // Persistent occluder world, locked for the whole call and *diffed*
    // against the client's spawned set plus the coarse horizon assets.
    // Rebuilding it from scratch every recompute (with all the GLB fetches
    // and trimesh builds that implies) is what blew the call up to hundreds
    // of milliseconds; the steady-state diff here is a handful of tiles.
    #[cfg(feature = "worker")]
    let mut occluder_guard = if req.enable_visibility_cull {
        let mut guard = crate::visibility::OCCLUDER_WORLD_CACHE.write().await;
        let world = guard.get_or_insert_with(crate::visibility::OccluderWorld::new_empty);

        let mut keep: BTreeSet<MapTreeNodePath> = BTreeSet::new();
        let mut missing: Vec<(MapTreeNodePath, BBox, Vec<(String, String)>)> = Vec::new();
        for path in &req.spawned_nodes {
            let Some(node) = data_res.all_nodes.get(path) else {
                continue;
            };
            if !bbox_occluder_ok(&node.bbox) {
                continue;
            }
            keep.insert(path.clone());
            if !world.path_to_id.contains_key(path) {
                missing.push((path.clone(), node.bbox, occluder_assets_of(data_res, path)));
            }
        }
        for asset in &data_res.coarse_assets {
            let Some(glb_path) = &asset.glb_path else {
                continue;
            };
            if !bbox_occluder_ok(&asset.bbox) {
                continue;
            }
            keep.insert(asset._octant_path.clone());
            if !world.path_to_id.contains_key(&asset._octant_path) {
                missing.push((
                    asset._octant_path.clone(),
                    asset.bbox,
                    vec![(asset.name.0.clone(), glb_path.clone())],
                ));
            }
        }

        world.retain_paths(&keep);
        if !missing.is_empty() {
            let added = missing.len();
            insert_occluders_chunked(world, &missing, &req.base_url).await;
            let dt = _crack_utils::get_timestamp_now_ms() - t0;
            if dt > 4 {
                tracing::info!(
                    "Occluder world sync: +{} occluders in {} ms ({} total)",
                    added,
                    dt,
                    world.trimeshes.len()
                );
            }
        }
        Some(guard)
    } else {
        None
    };

    // Cross-call visibility verdicts (see VIS_VERDICT_CACHE docs).
    #[cfg(feature = "worker")]
    let mut verdict_guard = if req.enable_visibility_cull {
        Some(crate::visibility::VIS_VERDICT_CACHE.write().await)
    } else {
        None
    };

    // Verdicts are keyed per camera configuration: quantized 2 m cell (the
    // client already recomputes on ~2 m movement) plus the sample radius.
    #[cfg(feature = "worker")]
    let cam_key: u64 = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for cam in &req.cameras {
            ((cam.center.x / 2.0).round() as i64).hash(&mut h);
            ((cam.center.y / 2.0).round() as i64).hash(&mut h);
            ((cam.center.z / 2.0).round() as i64).hash(&mut h);
            cam.sample_radius.to_bits().hash(&mut h);
        }
        h.finish()
    };

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

    // Detail band. `min` is the coarsest distribution the map may rest at:
    // any node coarser than it must split, no visibility rays asked. Between
    // min and max, splitting is gated on the BVH visibility test, so occluded
    // regions hold at the min level instead of consuming budget.
    let min_tpd = req.min_tiles_per_diagonal.min(req.max_tiles_per_diagonal);
    let max_tpd = req.max_tiles_per_diagonal;

    let split_class = |node_path: &MapTreeNodePath| -> SplitClass {
        if node_path.0.len() as i32 > req.max_lod {
            return SplitClass::No;
        }
        let bbox = tile_bbox(node_path);
        let bbox_diagonal = bbox.min.distance(bbox.max).clamp(0.00001, 100000.0);
        let mut distance = f32::INFINITY;
        for point in refs.iter() {
            distance = distance.min(compute_distance_to_aabb(&bbox, *point));
        }
        distance += 0.01;

        let tile_value = bbox_diagonal / distance;
        if tile_value >= 1.0 / (0.01 + min_tpd) {
            SplitClass::Mandatory
        } else if tile_value >= 1.0 / (0.01 + max_tpd) {
            SplitClass::IfVisible
        } else {
            SplitClass::No
        }
    };

    // Wall-clock budget for visibility rays in this call. When exhausted,
    // remaining candidates are assumed visible (fail-open) and left uncached,
    // so the next recompute re-tests them with a fresh budget: refinement
    // stays progressive instead of the call price growing with map size.
    #[cfg(feature = "worker")]
    const VIS_RAY_BUDGET_MS: i64 = 8;
    #[cfg(feature = "worker")]
    let t_walk = _crack_utils::get_timestamp_now_ms();
    #[cfg(feature = "worker")]
    let mut vis_budget_truncated = 0usize;

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

            let class = split_class(&node_path);
            if new_budget <= budget as usize && class != SplitClass::No {
                // Mandatory splits (below the detail floor) skip the ray
                // casting entirely: the base distribution is never culled.
                #[cfg(feature = "worker")]
                let vis = if class == SplitClass::Mandatory {
                    true
                } else if let (Some(occ), Some(ver)) =
                    (occluder_guard.as_mut(), verdict_guard.as_mut())
                {
                    let world = occ.as_mut().expect("occluder world initialized above");
                    let verdicts = ver.get_or_insert_with(std::collections::HashMap::new);
                    let now = _crack_utils::get_timestamp_now_ms();
                    let path_key = format!("{:?}", node_path);
                    let cached = verdicts
                        .get(&(path_key.clone(), cam_key))
                        .and_then(|(v, ts)| {
                            let fresh = now - ts < crate::visibility::VIS_VERDICT_TTL_MS;
                            (fresh && !crate::visibility::verdict_should_refresh(&path_key, now))
                                .then_some(*v)
                        });
                    match cached {
                        Some(v) => v,
                        None if now - t_walk > VIS_RAY_BUDGET_MS => {
                            vis_budget_truncated += 1;
                            true
                        }
                        None => {
                            let bbox = tile_bbox(&node_path);
                            let v = world.is_node_visible(&bbox, &node_path, &req.cameras);
                            if verdicts.len() >= crate::visibility::VIS_VERDICT_MAX_ENTRIES {
                                verdicts.clear();
                            }
                            verdicts.insert((path_key, cam_key), (v, now));
                            v
                        }
                    }
                } else {
                    true
                };

                #[cfg(not(feature = "worker"))]
                let vis = true;

                if vis {
                    proposed_nodes.remove(&node_path);
                    proposed_splits.insert(node_path.clone());
                    current_budget = new_budget;

                    // Lock-step occluder refinement, cache-only: swap the
                    // split parent's occluder for whichever children already
                    // have a cached trimesh. Never fetches mid-walk — a child
                    // seen for the first time becomes an occluder on a later
                    // call, once the client spawned it and the sync step at
                    // the top of this function picked it up.
                    #[cfg(feature = "worker")]
                    if let Some(occ) = occluder_guard.as_mut() {
                        let world = occ.as_mut().expect("occluder world initialized above");
                        world.remove_node(&node_path);
                        for c in &children {
                            let bbox = tile_bbox(c);
                            if !bbox_occluder_ok(&bbox) {
                                continue;
                            }
                            if let Some(tm) = crate::visibility::get_cached_trimesh(c).await {
                                world.insert_occluder(c, &bbox, tm);
                            }
                        }
                    }

                    for c in children {
                        heap.push((Score(tile_score(&c)), c.clone()));
                        proposed_nodes.insert(c.clone());
                    }
                } else {
                    culled_nodes.push(CulledNodeSummary {
                        path: node_path.clone(),
                        bbox: tile_bbox(&node_path),
                    });
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
                    let mut node_bbox = BBox::default();
                    if let Some(node_info) = data_res.all_nodes.get(cp) {
                        node_bbox = node_info.bbox;
                        for asset_id in &node_info.assets {
                            if let Some(asset_info) = data_res.assets.get(asset_id) {
                                if let Some(glb_path) = &asset_info.glb_path {
                                    assets_summary.push(crate::map::MapTileAssetInfoSummary {
                                        name: asset_id.clone(),
                                        glb_path: glb_path.clone(),
                                        bbox: asset_info.bbox,
                                    });
                                }
                            }
                        }
                    }
                    children_summary.push(crate::map::MapRootNodeSummary {
                        path: cp.clone(),
                        assets: assets_summary,
                        bbox: node_bbox,
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
        let mut node_bbox = BBox::default();
        if let Some(node_info) = data_res.all_nodes.get(proposed) {
            node_bbox = node_info.bbox;
            for asset_id in &node_info.assets {
                if let Some(asset_info) = data_res.assets.get(asset_id) {
                    if let Some(glb_path) = &asset_info.glb_path {
                        parent_assets.push(crate::map::MapTileAssetInfoSummary {
                            name: asset_id.clone(),
                            glb_path: glb_path.clone(),
                            bbox: asset_info.bbox,
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
            bbox: node_bbox,
        });
    }

    #[cfg(feature = "worker")]
    if vis_budget_truncated > 0 {
        tracing::info!(
            "visibility ray budget ({} ms) exhausted: {} candidates assumed visible this call",
            VIS_RAY_BUDGET_MS,
            vis_budget_truncated
        );
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
        culled_nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_compute_distance_to_aabb() {
        let bbox = BBox {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(2.0, 2.0, 2.0),
        };
        // Center of the box: zero distance to surface and to middle.
        assert_eq!(
            compute_distance_to_aabb(&bbox, Vec3::new(1.0, 1.0, 1.0)),
            0.0
        );
        // Outside point: average of surface distance and middle distance.
        // p = (4,1,1): surface distance 2, middle distance 3 -> (2 + 3) / 2.
        let d = compute_distance_to_aabb(&bbox, Vec3::new(4.0, 1.0, 1.0));
        assert!((d - 2.5).abs() < 1e-6);
    }
}
