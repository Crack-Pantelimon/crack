use crate::lod::CameraReference;
use crate::map::{BBox, MapTreeNodePath};
use parry3d::bounding_volume::Aabb;
use parry3d::math::Vector;
use parry3d::partitioning::{Bvh, BvhBuildStrategy, TraversalAction};
use parry3d::query::{Ray, RayCast};
use parry3d::shape::TriMesh;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Worker-global LRU of per-node occluder trimeshes, keyed by the Debug
/// formatting of the node path (`format!("{:?}", path)`). Bounded so wasm
/// worker memory does not grow with every tile ever seen.
pub static TRIMESH_CACHE: RwLock<Option<crate::worker::lru::LruCache<Arc<TriMesh>>>> =
    RwLock::const_new(None);

const TRIMESH_CACHE_ENTRIES: usize = 256;

/// Worker-global persistent occluder world. `compute_lod_changes` diffs it
/// against the client's spawned tile set each call instead of rebuilding it
/// (and re-fetching every occluder GLB) from scratch — the rebuild is what
/// made each LOD recompute cost hundreds of milliseconds.
pub static OCCLUDER_WORLD_CACHE: RwLock<Option<OccluderWorld>> = RwLock::const_new(None);

/// Worker-global cache of visibility verdicts, keyed by
/// (node path, hash of quantized camera cells + sample radii). The LOD walk
/// re-tests the same nodes every convergence round; verdicts from previous
/// rounds/frames are reused until the camera leaves its 2 m cell, the entry
/// ages past the TTL, or the probabilistic refresh re-rolls it.
pub static VIS_VERDICT_CACHE: RwLock<Option<HashMap<(String, u64), (bool, i64)>>> =
    RwLock::const_new(None);

/// Verdicts older than this are recomputed (occluders may have changed).
pub const VIS_VERDICT_TTL_MS: i64 = 4000;

/// Safety bound on the verdict map; cleared wholesale when exceeded.
pub const VIS_VERDICT_MAX_ENTRIES: usize = 16384;

/// Probabilistic refresh: ~5% of cache hits are re-tested anyway, so a stale
/// verdict (e.g. occluders changed under a stationary camera) heals within a
/// few recomputes instead of persisting for the full TTL.
pub fn verdict_should_refresh(path_key: &str, now_ms: i64) -> bool {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path_key.hash(&mut h);
    (now_ms / 500).hash(&mut h);
    h.finish() % 20 == 0
}

/// Builds a parry TriMesh from world-space collider geometry.
/// Returns None (with a warn! log) on empty or degenerate input instead of
/// panicking — a panic would trap the whole wasm worker.
pub fn build_trimesh_from_mesh(vertices: &[[f32; 3]], indices: &[[u32; 3]]) -> Option<TriMesh> {
    if vertices.is_empty() || indices.is_empty() {
        return None;
    }
    let verts: Vec<Vector> = vertices.iter().map(|v| Vector::from(*v)).collect();
    match TriMesh::new(verts, indices.to_vec()) {
        Ok(tm) => Some(tm),
        Err(e) => {
            tracing::warn!("TriMesh build failed: {:?}", e);
            None
        }
    }
}

/// Returns the occluder trimesh for one node, fetching missing tile GLBs
/// through the worker tile cache (and HTTP cache) if needed.
/// `assets` is (asset_id, glb_path) for every renderable asset of the node.
/// NEVER holds the TRIMESH_CACHE lock across an await.
pub async fn get_or_build_trimesh(
    path: &MapTreeNodePath,
    assets: &[(String, String)],
    base_url: &str,
) -> Option<Arc<TriMesh>> {
    let key = format!("{:?}", path);

    // 1. cache probe (short-lived lock)
    {
        let mut guard = TRIMESH_CACHE.write().await;
        let cache =
            guard.get_or_insert_with(|| crate::worker::lru::LruCache::new(TRIMESH_CACHE_ENTRIES));
        if let Some(tm) = cache.get(&key) {
            return Some(tm);
        }
    } // guard dropped here, before any fetch await

    // 2. gather collider meshes, fetching misses
    let mut combined_vertices: Vec<[f32; 3]> = Vec::new();
    let mut combined_indices: Vec<[u32; 3]> = Vec::new();
    for (asset_id, glb_path) in assets {
        let mesh = match crate::worker::tile_impl::get_tile_collider(asset_id).await {
            Some(m) => Some(m),
            None => {
                // Cache miss: fetch the GLB (fills the tile LRU as a side effect).
                match crate::worker::tile_impl::fetch_map_tile(crate::tile::FetchTileRequest {
                    base_url: base_url.to_string(),
                    tile_id: asset_id.clone(),
                    glb_path: glb_path.clone(),
                })
                .await
                {
                    Ok(resp) => resp.collider_mesh,
                    Err(e) => {
                        tracing::warn!("occluder tile fetch failed for {}: {:?}", asset_id, e);
                        None
                    }
                }
            }
        };
        if let Some(mesh) = mesh {
            let vertex_offset = combined_vertices.len() as u32;
            combined_vertices.extend(mesh.vertices);
            for tri in mesh.indices {
                combined_indices.push([
                    tri[0] + vertex_offset,
                    tri[1] + vertex_offset,
                    tri[2] + vertex_offset,
                ]);
            }
        }
    }

    let tm = Arc::new(build_trimesh_from_mesh(
        &combined_vertices,
        &combined_indices,
    )?);

    // 3. cache insert (short-lived lock)
    {
        let mut guard = TRIMESH_CACHE.write().await;
        let cache =
            guard.get_or_insert_with(|| crate::worker::lru::LruCache::new(TRIMESH_CACHE_ENTRIES));
        cache.insert(key, tm.clone());
    }
    Some(tm)
}

/// Cache-only trimesh lookup — never fetches. Used by the walk's lock-step
/// occluder refinement, which must not await the network mid-walk.
pub async fn get_cached_trimesh(path: &MapTreeNodePath) -> Option<Arc<TriMesh>> {
    let key = format!("{:?}", path);
    let mut guard = TRIMESH_CACHE.write().await;
    guard.as_mut()?.get(&key)
}

fn bbox_contains(candidate: &BBox, target: &BBox, epsilon: f32) -> bool {
    candidate.min.x - epsilon <= target.min.x
        && candidate.max.x + epsilon >= target.max.x
        && candidate.min.y - epsilon <= target.min.y
        && candidate.max.y + epsilon >= target.max.y
        && candidate.min.z - epsilon <= target.min.z
        && candidate.max.z + epsilon >= target.max.z
}

/// The spatial database built dynamically from currently spawned tiles and coarse horizon tiles.
pub struct OccluderWorld {
    /// BVH indexing occluder leaf ids by world-space bounds.
    pub bvh: Bvh,
    /// Trimesh geometry keyed by occluder leaf id.
    pub trimeshes: HashMap<u32, Arc<TriMesh>>,
    /// Leaf id to octant path lookup.
    pub id_to_path: HashMap<u32, MapTreeNodePath>,
    /// Octant path to leaf id lookup.
    pub path_to_id: HashMap<MapTreeNodePath, u32>,
    /// World bounds keyed by occluder leaf id.
    pub aabbs: HashMap<u32, BBox>,
    /// Next leaf id assigned by `insert_occluder`.
    pub next_id: u32,
}

impl OccluderWorld {
    /// An empty world; occluders are added incrementally with insert_occluder.
    pub fn new_empty() -> Self {
        OccluderWorld {
            bvh: Bvh::from_leaves(BvhBuildStrategy::Binned, &[]),
            trimeshes: HashMap::new(),
            id_to_path: HashMap::new(),
            path_to_id: HashMap::new(),
            aabbs: HashMap::new(),
            next_id: 0,
        }
    }

    /// Inserts one occluder whose trimesh has already been resolved.
    /// No-op if the path is already present.
    pub fn insert_occluder(&mut self, path: &MapTreeNodePath, bbox: &BBox, trimesh: Arc<TriMesh>) {
        if self.path_to_id.contains_key(path) {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.bvh.insert(Aabb::new(bbox.min, bbox.max), id);
        self.trimeshes.insert(id, trimesh);
        self.id_to_path.insert(id, path.clone());
        self.path_to_id.insert(path.clone(), id);
        self.aabbs.insert(id, *bbox);
    }

    /// Removes one occluder; no-op if absent.
    pub fn remove_node(&mut self, path: &MapTreeNodePath) {
        if let Some(id) = self.path_to_id.remove(path) {
            self.bvh.remove(id);
            self.trimeshes.remove(&id);
            self.id_to_path.remove(&id);
            self.aabbs.remove(&id);
        }
    }

    /// Drops every occluder whose path is not in `keep`. Together with
    /// insert_occluder this diffs the persistent world against the client's
    /// current spawned set.
    pub fn retain_paths(&mut self, keep: &BTreeSet<MapTreeNodePath>) {
        let stale: Vec<MapTreeNodePath> = self
            .path_to_id
            .keys()
            .filter(|p| !keep.contains(*p))
            .cloned()
            .collect();
        for path in stale {
            self.remove_node(&path);
        }
    }

    /// Casts a ray from `origin` to `target`. Returns true if it is occluded by any trimesh.
    pub fn is_ray_occluded(
        &self,
        origin: Vector,
        target: Vector,
        exclude_path: &MapTreeNodePath,
        exclude_bbox: &BBox,
    ) -> bool {
        // Reject non-finite endpoints before they reach parry. A NaN/inf ray can
        // trigger undefined behaviour / a wasm trap deep inside the trimesh
        // ray cast, which on the browser silently aborts the whole worker.
        if !origin.is_finite() || !target.is_finite() {
            return false;
        }
        let dir = target - origin;
        let dist = dir.length();
        if dist < 1e-3 {
            return false;
        }
        let ray = Ray::new(origin, dir / dist);

        let mut occluded = false;

        self.bvh.traverse(|node| {
            if occluded {
                return TraversalAction::EarlyExit;
            }

            let toi = node.cast_ray(&ray, dist);
            if toi >= dist {
                return TraversalAction::Prune;
            }

            if let Some(leaf_id) = node.leaf_data() {
                if let Some(path) = self.id_to_path.get(&leaf_id) {
                    let same_lineage = path.0.starts_with(&exclude_path.0) // target or its descendant
                        || exclude_path.0.starts_with(&path.0); // an ancestor of the target
                    if same_lineage {
                        return TraversalAction::Continue;
                    }
                }

                if let Some(cand_bbox) = self.aabbs.get(&leaf_id) {
                    if bbox_contains(cand_bbox, exclude_bbox, 1e-3) {
                        return TraversalAction::Continue;
                    }

                    if let Some(tm) = self.trimeshes.get(&leaf_id) {
                        // Trimesh vertices are world-space: local space == world space, no pose.
                        if let Some(toi) = tm.cast_local_ray(&ray, dist, true) {
                            if toi < dist {
                                occluded = true;
                                return TraversalAction::EarlyExit;
                            }
                        }
                    }
                }
            }

            TraversalAction::Continue
        });

        occluded
    }

    /// Checks if a node is visible from any camera position.
    pub fn is_node_visible(
        &self,
        node_bbox: &BBox,
        node_path: &MapTreeNodePath,
        cameras: &[CameraReference],
    ) -> bool {
        if cameras.is_empty() {
            return true; // no camera constraint => cannot cull
        }
        if self.trimeshes.is_empty() {
            return true; // no occluders => everything visible
        }

        // Target points on the node's AABB: center + 8 corners + 6 face
        // centers, all pulled 5% toward the center. Un-shrunk corners and
        // faces lie exactly on the shared boundary with neighboring tiles,
        // so rays to them graze neighbor geometry at the endpoint and report
        // spurious occlusion — distant tiles then never refine.
        let center = (node_bbox.min + node_bbox.max) / 2.0;
        let shrink = |p: Vector| -> Vector { center + (p - center) * 0.95 };
        let (min, max) = (node_bbox.min, node_bbox.max);
        // Top corners before bottom ones: sight lines from a street-level
        // camera clear a tile's top first, and `sees_node` early-exits on the
        // first unoccluded target.
        let mut targets: Vec<Vector> = Vec::with_capacity(16);
        targets.push(center);
        for corner in [
            Vector::new(min.x, max.y, min.z),
            Vector::new(min.x, max.y, max.z),
            Vector::new(max.x, max.y, min.z),
            Vector::new(max.x, max.y, max.z),
            Vector::new(min.x, min.y, min.z),
            Vector::new(min.x, min.y, max.z),
            Vector::new(max.x, min.y, min.z),
            Vector::new(max.x, min.y, max.z),
        ] {
            targets.push(shrink(corner));
        }
        for face in [
            Vector::new(min.x, center.y, center.z),
            Vector::new(max.x, center.y, center.z),
            Vector::new(center.x, min.y, center.z),
            Vector::new(center.x, max.y, center.z),
            Vector::new(center.x, center.y, min.z),
            Vector::new(center.x, center.y, max.z),
        ] {
            targets.push(shrink(face));
        }

        const MIN_SAMPLING_RADIUS: f32 = 0.25;
        // Velocity below this (m/s) is noise; skip lookahead points.
        const MIN_LOOKAHEAD_SPEED: f32 = 0.5;

        for camera in cameras {
            // The node point nearest to the camera. For a tile seen down a
            // street canyon this lies on the near face on the canyon axis —
            // a sight line the corner/face samples all miss laterally.
            let nearest = shrink(Vector::new(
                camera.center.x.clamp(min.x.min(max.x), min.x.max(max.x)),
                camera.center.y.clamp(min.y.min(max.y), min.y.max(max.y)),
                camera.center.z.clamp(min.z.min(max.z), min.z.max(max.z)),
            ));

            // Closure: can any target point be seen from this origin?
            let sees_node = |origin: Vector| -> bool {
                std::iter::once(&nearest)
                    .chain(targets.iter())
                    .any(|&q| !self.is_ray_occluded(origin, q, node_path, node_bbox))
            };

            // 1. The camera point itself is always definitive when clear.
            if sees_node(camera.center) {
                return true;
            }

            let r = camera.sample_radius;
            if r < MIN_SAMPLING_RADIUS {
                continue; // point-based model: nothing more to test for this camera
            }

            // Candidate extra origins, all within radius r of the camera.
            let mut origins: Vec<Vector> = Vec::with_capacity(11);

            // 2. Velocity lookahead: where the camera is about to be.
            let speed = camera.velocity.length();
            if speed > MIN_LOOKAHEAD_SPEED {
                let v_dir = camera.velocity / speed;
                for k in [1.0 / 3.0, 2.0 / 3.0, 1.0] {
                    origins.push(camera.center + v_dir * (r * k));
                }
            }

            // 3. Horizontal ring: turns the velocity doesn't predict.
            for k in 0..8 {
                let a = (k as f32) * std::f32::consts::FRAC_PI_4;
                origins.push(camera.center + Vector::new(a.cos() * r, 0.0, a.sin() * r));
            }

            for origin in origins {
                // Pre-filter: only use origins the real camera can see. An origin
                // behind a wall or under the terrain fails this test and is skipped,
                // so it can never falsely re-grant visibility (the failure mode of
                // the old large-radius sphere model).
                if self.is_ray_occluded(camera.center, origin, node_path, node_bbox) {
                    continue;
                }
                if sees_node(origin) {
                    return true;
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trimesh_vertical_raycast() {
        let vertices = vec![
            [0.0, 5.0, 0.0],
            [10.0, 5.0, 0.0],
            [10.0, 5.0, 10.0],
            [0.0, 5.0, 10.0],
        ];
        let indices = vec![[0, 1, 2], [0, 2, 3]];
        let tm = build_trimesh_from_mesh(&vertices, &indices).expect("trimesh builds");

        let ray = Ray::new(Vector::new(5.0, 10.0, 5.0), Vector::new(0.0, -1.0, 0.0));
        let toi = tm.cast_local_ray(&ray, 10.0, true).expect("ray hits");
        assert!((ray.point_at(toi).y - 5.0).abs() < 1e-4);
    }

    #[test]
    fn test_trimesh_degenerate_input() {
        assert!(build_trimesh_from_mesh(&[], &[]).is_none());
    }
}
