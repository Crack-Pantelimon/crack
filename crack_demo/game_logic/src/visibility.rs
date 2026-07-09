use crate::lod::CameraReference;
use crate::map::{BBox, MapTreeAssetInfo, MapTreeNodePath};
use parry3d::bounding_volume::Aabb;
use parry3d::math::{Pose, Vector};
use parry3d::partitioning::{Bvh, BvhBuildStrategy, TraversalAction};
use parry3d::query::{Ray, RayCast};
use parry3d::shape::HeightField;
use parry3d::utils::Array2;
use std::collections::{BTreeSet, HashMap};
use tokio::sync::RwLock;

/// A worker-global cache storing computed height maps to avoid redundant rasterization.
pub static HEIGHTMAP_CACHE: RwLock<Option<HashMap<MapTreeNodePath, HeightField>>> =
    RwLock::const_new(None);

/// Helper function to invert a `Pose` (Isometry) without relying on trait methods.
#[inline]
pub fn invert_pose(pose: &Pose) -> Pose {
    let inv_rot = pose.rotation.inverse();
    let inv_trans = inv_rot * -pose.translation;
    Pose::from_parts(inv_trans, inv_rot)
}

/// Simplifies a mesh collider data into a 64x64 heightfield.
/// Triangles are projected onto the XZ plane and barycentric coordinates are used
/// to sample heights at grid cell centers. Holes are filled via a 1-pixel dilation pass.
pub fn build_heightfield_from_mesh(
    bbox: &BBox,
    vertices: &[[f32; 3]],
    indices: &[[u32; 3]],
) -> HeightField {
    let nrows = 64;
    let ncols = 64;
    let dx = (bbox.max.x - bbox.min.x) / 63.0;
    let dz = (bbox.max.z - bbox.min.z) / 63.0;

    let sentinel = bbox.min.y - 10.0;
    let mut data = vec![sentinel; nrows * ncols];

    for &tri_idx in indices {
        let a = Vector::from(vertices[tri_idx[0] as usize]);
        let b = Vector::from(vertices[tri_idx[1] as usize]);
        let c = Vector::from(vertices[tri_idx[2] as usize]);

        let min_x = a.x.min(b.x).min(c.x);
        let max_x = a.x.max(b.x).max(c.x);
        let min_z = a.z.min(b.z).min(c.z);
        let max_z = a.z.max(b.z).max(c.z);

        let j_min = (((min_x - bbox.min.x) / dx).floor() as isize).clamp(0, 63) as usize;
        let j_max = (((max_x - bbox.min.x) / dx).ceil() as isize).clamp(0, 63) as usize;
        let i_min = (((min_z - bbox.min.z) / dz).floor() as isize).clamp(0, 63) as usize;
        let i_max = (((max_z - bbox.min.z) / dz).ceil() as isize).clamp(0, 63) as usize;

        for i in i_min..=i_max {
            for j in j_min..=j_max {
                let px = bbox.min.x + (j as f32) * dx;
                let pz = bbox.min.z + (i as f32) * dz;

                let v0 = b - a;
                let v1 = c - a;
                let v2 = Vector::new(px, 0.0, pz) - a;

                let d00 = v0.x * v0.x + v0.z * v0.z;
                let d01 = v0.x * v1.x + v0.z * v1.z;
                let d11 = v1.x * v1.x + v1.z * v1.z;
                let d20 = v2.x * v0.x + v2.z * v0.z;
                let d21 = v2.x * v1.x + v2.z * v1.z;

                let denom = d00 * d11 - d01 * d01;
                if denom.abs() > 1e-6 {
                    let v = (d11 * d20 - d01 * d21) / denom;
                    let w = (d00 * d21 - d01 * d20) / denom;
                    let u = 1.0 - v - w;

                    if u >= -1e-4 && v >= -1e-4 && w >= -1e-4 {
                        let h = u * a.y + v * b.y + w * c.y;
                        let idx = i + j * nrows;
                        if h > data[idx] {
                            data[idx] = h;
                        }
                    }
                }
            }
        }
    }

    // Hole-filling / dilation pass
    let mut filled_data = data.clone();
    for i in 0..nrows {
        for j in 0..ncols {
            let idx = i + j * nrows;
            if data[idx] < bbox.min.y - 1e-3 {
                let mut sum = 0.0;
                let mut count = 0;
                for di in -1..=1 {
                    for dj in -1..=1 {
                        let ni = i as isize + di;
                        let nj = j as isize + dj;
                        if ni >= 0 && ni < nrows as isize && nj >= 0 && nj < ncols as isize {
                            let nidx = ni as usize + nj as usize * nrows;
                            if data[nidx] >= bbox.min.y - 1e-3 {
                                sum += data[nidx];
                                count += 1;
                            }
                        }
                    }
                }
                if count > 0 {
                    filled_data[idx] = sum / count as f32;
                }
            }
        }
    }

    let heights_zx = Array2::new(nrows, ncols, filled_data);
    let scale = Vector::new(bbox.max.x - bbox.min.x, 1.0, bbox.max.z - bbox.min.z);
    HeightField::new(heights_zx, scale)
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
    pub bvh: Bvh,
    pub heightfields: HashMap<u32, HeightField>,
    pub transforms: HashMap<u32, Pose>,
    pub id_to_path: HashMap<u32, MapTreeNodePath>,
    pub aabbs: HashMap<u32, BBox>,
}

impl OccluderWorld {
    /// Builds a dynamic `OccluderWorld` using a set of spawned paths and static coarse assets.
    pub async fn rebuild_bvh(
        spawned_nodes: &BTreeSet<MapTreeNodePath>,
        coarse_assets: &[MapTreeAssetInfo],
    ) -> Self {
        let mut leaves = Vec::new();
        let mut heightfields = HashMap::new();
        let mut transforms = HashMap::new();
        let mut id_to_path = HashMap::new();
        let mut aabbs = HashMap::new();
        let mut next_id = 0;
        let manifest = crate::worker::manifest_impl::get_manifest_cache()
            .await
            .ok();

        let mut hm_cache_guard = HEIGHTMAP_CACHE.write().await;
        let hm_cache = hm_cache_guard.get_or_insert_with(HashMap::new);

        // Gather all relevant assets to treat as potential occluders:
        // 1. All currently active spawned nodes.
        // 2. Coarse tiles representing the background map.
        let mut candidates = Vec::new();
        for path in spawned_nodes {
            candidates.push((path.clone(), None));
        }
        for asset in coarse_assets {
            candidates.push((asset._octant_path.clone(), Some(asset)));
        }

        for (path, coarse_asset_opt) in candidates {
            // Find bounding box from manifest cache
            let (bbox, assets_to_fetch) = if let Some(asset) = coarse_asset_opt {
                (asset.bbox, vec![asset.name.0.clone()])
            } else {
                if let Some(ref manifest) = manifest {
                    if let Some(node) = manifest.all_nodes.get(&path) {
                        (
                            node.bbox,
                            node.assets.iter().map(|a| a.0.clone()).collect::<Vec<_>>(),
                        )
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            };

            // Retrieve or build heightfield
            let hf_opt = if let Some(hf) = hm_cache.get(&path) {
                Some(hf.clone())
            } else {
                let mut combined_vertices = Vec::new();
                let mut combined_indices = Vec::new();

                for asset_id in &assets_to_fetch {
                    if let Some(mesh) = crate::worker::tile_impl::get_tile_collider(asset_id).await
                    {
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

                if !combined_vertices.is_empty() {
                    let hf =
                        build_heightfield_from_mesh(&bbox, &combined_vertices, &combined_indices);
                    hm_cache.insert(path.clone(), hf.clone());
                    Some(hf)
                } else {
                    None
                }
            };

            if let Some(hf) = hf_opt {
                let leaf_id = next_id;
                next_id += 1;

                let parry_aabb = Aabb::new(bbox.min, bbox.max);
                leaves.push(parry_aabb);

                let center_x = (bbox.min.x + bbox.max.x) / 2.0;
                let center_z = (bbox.min.z + bbox.max.z) / 2.0;
                let pose =
                    Pose::from_parts(Vector::new(center_x, 0.0, center_z), glam::Quat::IDENTITY);

                heightfields.insert(leaf_id, hf);
                transforms.insert(leaf_id, pose);
                id_to_path.insert(leaf_id, path);
                aabbs.insert(leaf_id, bbox);
            }
        }

        let bvh = Bvh::from_leaves(BvhBuildStrategy::Binned, &leaves);

        OccluderWorld {
            bvh,
            heightfields,
            transforms,
            id_to_path,
            aabbs,
        }
    }

    /// Casts a ray from `origin` to `target`. Returns true if it is occluded by any heightfield.
    pub fn is_ray_occluded(
        &self,
        origin: Vector,
        target: Vector,
        exclude_path: &MapTreeNodePath,
        exclude_bbox: &BBox,
    ) -> bool {
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
                    let same_lineage = path.0.starts_with(&exclude_path.0)   // target or its descendant
                        || exclude_path.0.starts_with(&path.0); // an ancestor of the target
                    if same_lineage {
                        return TraversalAction::Continue;
                    }
                }

                if let Some(cand_bbox) = self.aabbs.get(&leaf_id) {
                    if bbox_contains(cand_bbox, exclude_bbox, 1e-3) {
                        return TraversalAction::Continue;
                    }

                    if let Some(hf) = self.heightfields.get(&leaf_id) {
                        if let Some(pose) = self.transforms.get(&leaf_id) {
                            let inv_pose = invert_pose(pose);
                            let local_ray = ray.transform_by(&inv_pose);
                            if let Some(intersection) =
                                hf.cast_local_ray_and_get_normal(&local_ray, dist, true)
                            {
                                if intersection.time_of_impact < dist {
                                    let hit_point = local_ray.point_at(intersection.time_of_impact);
                                    if hit_point.y >= cand_bbox.min.y + 1e-3 {
                                        occluded = true;
                                        return TraversalAction::EarlyExit;
                                    }
                                }
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
    /// Generates samples on the hemisphere facing the target node, plus the look target itself.
    pub fn is_node_visible(
        &self,
        node_bbox: &BBox,
        node_path: &MapTreeNodePath,
        cameras: &[CameraReference],
    ) -> bool {
        if cameras.is_empty() {
            return true; // no camera constraint => cannot cull
        }
        if self.heightfields.is_empty() {
            return true; // no occluders => everything visible
        }

        // Target points on the node's AABB (corners + center)
        let center = (node_bbox.min + node_bbox.max) / 2.0;
        let corners = [
            Vector::new(node_bbox.min.x, node_bbox.min.y, node_bbox.min.z),
            Vector::new(node_bbox.min.x, node_bbox.min.y, node_bbox.max.z),
            Vector::new(node_bbox.min.x, node_bbox.max.y, node_bbox.min.z),
            Vector::new(node_bbox.min.x, node_bbox.max.y, node_bbox.max.z),
            Vector::new(node_bbox.max.x, node_bbox.min.y, node_bbox.min.z),
            Vector::new(node_bbox.max.x, node_bbox.min.y, node_bbox.max.z),
            Vector::new(node_bbox.max.x, node_bbox.max.y, node_bbox.min.z),
            Vector::new(node_bbox.max.x, node_bbox.max.y, node_bbox.max.z),
            center,
        ];

        // The camera sphere samples model *uncertainty* in the camera position
        // between LOD recomputes (the camera can drift a few metres before the next
        // request). `camera.max_range` is a LOD *reach* distance (hundreds–thousands
        // of metres in `lod_flow.rs`); using it as the sample-sphere radius scatters
        // test cameras so far out that some sample always has a clear line of sight,
        // which makes the visibility gate return `true` for everything and disables
        // culling entirely. Bound it to a small drift radius so the samples stay in
        // the camera's actual neighbourhood and share its occlusion.
        const MAX_CAMERA_DRIFT: f32 = 6.0;

        for camera in cameras {
            // Check direct line of sight from the actual camera position.
            for &q in &corners {
                if !self.is_ray_occluded(camera.center, q, node_path, node_bbox) {
                    return true; // Visible from the camera itself.
                }
            }

            let drift = camera.max_range.min(MAX_CAMERA_DRIFT);
            if drift <= 1e-3 {
                continue; // no drift budget => camera-center test above is definitive
            }

            // Fibonacci-sphere samples around the camera position (uniform, small radius).
            let num_samples = 16;
            for s in 0..num_samples {
                let y = 1.0 - (s as f32 / (num_samples - 1) as f32) * 2.0;
                let radius = (1.0 - y * y).sqrt();
                let golden_ratio = (1.0 + 5.0f32.sqrt()) / 2.0;
                let theta = 2.0 * std::f32::consts::PI * (s as f32) / golden_ratio;
                let offset = Vector::new(radius * theta.cos(), y, radius * theta.sin());

                let cam_pos = camera.center + offset * drift;
                for &q in &corners {
                    if !self.is_ray_occluded(cam_pos, q, node_path, node_bbox) {
                        return true; // Visible from a plausible near-camera position.
                    }
                }
            }
        }

        false // Occluded from the camera and its whole drift neighbourhood.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heightfield_vertical_raycast() {
        let bbox = BBox {
            min: glam::Vec3::new(0.0, 0.0, 0.0),
            max: glam::Vec3::new(10.0, 10.0, 10.0),
        };
        let vertices = vec![
            [0.0, 5.0, 0.0],
            [10.0, 5.0, 0.0],
            [10.0, 5.0, 10.0],
            [0.0, 5.0, 10.0],
        ];
        let indices = vec![[0, 1, 2], [0, 2, 3]];

        let hf = build_heightfield_from_mesh(&bbox, &vertices, &indices);

        // Ray starts at (5.0, 10.0, 5.0) and points straight down
        let origin = Vector::new(5.0, 10.0, 5.0);
        let dir = Vector::new(0.0, -1.0, 0.0);
        let dist = 10.0;
        let ray = Ray::new(origin, dir);

        let intersection = hf.cast_local_ray_and_get_normal(&ray, dist, true);
        assert!(intersection.is_some(), "Ray should hit the heightfield");
        let hit = intersection.unwrap();
        assert!(hit.time_of_impact < dist);

        let hit_point = ray.point_at(hit.time_of_impact);
        assert!(
            (hit_point.y - 5.0).abs() < 1e-4,
            "Hit height should be 5.0, got {}",
            hit_point.y
        );
    }
}
