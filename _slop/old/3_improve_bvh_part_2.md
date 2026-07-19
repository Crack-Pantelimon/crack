# Improve BVH occlusion — Part 2: real tile trimeshes instead of heightfields

Part 2 of 4. Requires part 1 to be merged. Parts 3 and 4 build on the
structures introduced here — follow the naming exactly.

## Build & check (read first)

All commands run from the repo root `/home/p/VIDOEGAME/crack`. `cargo` may be
provided by an AppImage that misbehaves through its "proxy" wrapper — always
`unset ARGV0` first. Only check the two affected crates:

```sh
unset ARGV0
cargo check -p game_logic --features worker
cargo check -p demo_resolution_selector_web_bevy
```

Also run the unit tests for the visibility module:

```sh
unset ARGV0
cargo test -p game_logic --features worker visibility
```

## Problem

`crack_demo/game_logic/src/visibility.rs` currently rasterizes each tile's
collider mesh into a 64×64 **max-height** heightfield
(`build_heightfield_from_mesh`) and ray-casts against those. Max-height
rasterization plus the 1-pixel dilation pass inflates building/terrain
silhouettes, so distant tiles that are actually visible get reported as
occluded ("too much occlusion; some tiles in the distance stay in their raw
format even though they are visible").

Fix: ray-cast against the **actual tile collider trimesh** (parry3d
`TriMesh`). The collider mesh already exists — `get_tile_collider` in
`crack_demo/game_logic/src/worker/tile_impl.rs` extracts it from the GLB — and
its vertices are in **world space** (the heightfield code used them directly
against world bboxes), so no pose/transform is needed at all.

Additionally, per the maintainer's decision: when a tile's collider is not in
the tile cache yet, the worker must **fetch it (await `fetch_map_tile`)**
rather than skip it. First rebuilds are slower; later ones hit the tile LRU
and the HTTP cache. Fetches must run **concurrently in batches of 8**.

## Constraints (wasm worker — read carefully)

This code runs inside a single-threaded cooperative wasm web worker:

- **Never hold a `tokio::sync::RwLock` guard across an `.await`** of another
  lock user — with `join_all` running sibling futures on the same thread this
  deadlocks or serializes everything. Pattern: lock → read/copy → drop guard →
  await → lock → write → drop guard.
- A Rust panic becomes an `unreachable` trap that silently kills the whole
  worker. Guard against empty/degenerate mesh input before calling parry
  constructors; never `unwrap()` on fetch results.
- `tokio` here has only the `sync` feature — **no** `tokio::spawn`,
  **no** `JoinSet`. Use `futures::future::join_all` (dependency added below).

## Changes

### 1. `crack_demo/game_logic/Cargo.toml` — add `futures`

Add to `[dependencies]`:

```toml
futures = { version = "0.3", default-features = false, features = ["alloc"], optional = true }
```

and add `"dep:futures"` to the `worker = [...]` feature list.

### 2. `crack_demo/game_logic/src/worker/mod.rs` — expose the LRU

The `lru` module (declared in `worker/mod.rs`) must be visible from
`visibility.rs`. Change its declaration to `pub(crate) mod lru;` (or `pub mod
lru;` if `pub(crate)` conflicts with existing visibility).

### 3. `crack_demo/game_logic/src/lod.rs` — plumb `base_url`

`LodComputeRequest` gains one field:

```rust
pub struct LodComputeRequest {
    pub spawned_nodes: BTreeSet<MapTreeNodePath>,
    pub reference_points: Vec<Vec3>,
    pub cameras: Vec<CameraReference>,
    pub lod_budget: u32,
    pub max_lod: i32,
    pub tiles_per_diagonal: f32,
    pub enable_visibility_cull: bool,
    pub base_url: String,          // NEW: origin for worker-side tile fetches
}
```

In `compute_lod_changes`, pass it through to the rebuild call:

```rust
crate::visibility::OccluderWorld::rebuild_bvh(
    &req.spawned_nodes,
    &data_res.coarse_assets,
    &req.base_url,
)
.await
```

Client side, in
`crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs`,
fill it when building the request (same pattern as `map_lod.rs:91`):

```rust
base_url: crate::config::DATA_BASE_URL.to_string(),
```

Then `grep -rn "LodComputeRequest" --include="*.rs"` across the repo and fix
any other constructor (tests, native worker) the same way.

### 4. `crack_demo/game_logic/src/visibility.rs` — the main rewrite

#### 4a. Delete

- `HEIGHTMAP_CACHE` static.
- `build_heightfield_from_mesh` (whole function).
- `invert_pose` (whole function; it has no other callers — verify with grep).
- `OccluderWorld.heightfields` and `OccluderWorld.transforms` fields.
- The `hit_point.y >= cand_bbox.min.y + 1e-3` hack in `is_ray_occluded` (it
  compensated for heightfield sentinel values; trimeshes don't need it).
- Imports that become unused: `HeightField`, `Array2`, `Pose`.

#### 4b. New trimesh cache

```rust
use parry3d::shape::TriMesh;
use std::sync::Arc;

/// Worker-global LRU of per-node occluder trimeshes, keyed by the Debug
/// formatting of the node path (`format!("{:?}", path)`). Bounded so wasm
/// worker memory does not grow with every tile ever seen.
pub static TRIMESH_CACHE: RwLock<Option<crate::worker::lru::LruCache<Arc<TriMesh>>>> =
    RwLock::const_new(None);

const TRIMESH_CACHE_ENTRIES: usize = 256;
```

#### 4c. New builder

```rust
/// Builds a parry TriMesh from world-space collider geometry.
/// Returns None (with a warn! log) on empty or degenerate input instead of
/// panicking — a panic would trap the whole wasm worker.
pub fn build_trimesh_from_mesh(
    vertices: &[[f32; 3]],
    indices: &[[u32; 3]],
) -> Option<TriMesh> {
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
```

(`parry3d::math::Vector` is already imported in this file; `TriMesh::new`
returns `Result<TriMesh, TriMeshBuilderError>` in parry3d 0.27.)

#### 4d. New fetch-or-build helper (this is the piece parts 3 reuses)

```rust
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
        let cache = guard.get_or_insert_with(|| crate::worker::lru::LruCache::new(TRIMESH_CACHE_ENTRIES));
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

    let tm = Arc::new(build_trimesh_from_mesh(&combined_vertices, &combined_indices)?);

    // 3. cache insert (short-lived lock)
    {
        let mut guard = TRIMESH_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| crate::worker::lru::LruCache::new(TRIMESH_CACHE_ENTRIES));
        cache.insert(key, tm.clone());
    }
    Some(tm)
}
```

Check the exact field names of `FetchTileRequest` in
`crack_demo/game_logic/src/tile.rs` before writing this (it has `base_url`,
`tile_id`, `glb_path`; adjust if they differ).

#### 4e. `OccluderWorld` struct

```rust
pub struct OccluderWorld {
    pub bvh: Bvh,
    pub trimeshes: HashMap<u32, Arc<TriMesh>>,
    pub id_to_path: HashMap<u32, MapTreeNodePath>,
    pub path_to_id: HashMap<MapTreeNodePath, u32>, // used by part 3's remove_node
    pub aabbs: HashMap<u32, BBox>,
    pub next_id: u32,
}
```

#### 4f. `rebuild_bvh`

New signature:

```rust
pub async fn rebuild_bvh(
    spawned_nodes: &BTreeSet<MapTreeNodePath>,
    coarse_assets: &[MapTreeAssetInfo],
    base_url: &str,
) -> Self
```

Keep the existing candidate gathering (spawned nodes resolved through the
manifest cache, coarse assets used directly) and the degenerate-bbox skip.
Changes:

- For each candidate, resolve its asset list to `Vec<(String, String)>`
  (asset id, glb_path). For manifest nodes: iterate `node.assets`, look each
  id up in `manifest.assets` and take its `glb_path` (skip assets whose
  `glb_path` is `None`). For coarse assets: `(asset.name.0.clone(),
  asset.glb_path.clone())` — check the actual field/Option shape on
  `MapTreeAssetInfo` in `crack_demo/game_logic/src/map.rs` and skip if the
  path is absent.
- Resolve trimeshes **concurrently in chunks of 8**:

```rust
// candidates: Vec<(MapTreeNodePath, BBox, Vec<(String, String)>)>
let mut resolved: Vec<(MapTreeNodePath, BBox, Option<Arc<TriMesh>>)> = Vec::new();
for chunk in candidates.chunks(8) {
    let futs = chunk
        .iter()
        .map(|(path, _bbox, assets)| get_or_build_trimesh(path, assets, base_url));
    let metas = futures::future::join_all(futs).await;
    for ((path, bbox, _), tm) in chunk.iter().zip(metas) {
        resolved.push((path.clone(), *bbox, tm));
    }
}
```

- Then a plain sync loop over `resolved`: for each `Some(tm)`, assign
  `leaf_id = next_id; next_id += 1;`, push `Aabb::new(bbox.min, bbox.max)`
  into `leaves`, fill `trimeshes`, `id_to_path`, `path_to_id`, `aabbs`.
- Finish with `Bvh::from_leaves(BvhBuildStrategy::Binned, &leaves)` as today.
- Update the "Occluder BVH rebuilt" log in `lod.rs` to report
  `world.trimeshes.len()` instead of `heightfields.len()`.

#### 4g. `is_ray_occluded`

Keep the signature, the non-finite guard, the BVH traversal, the
same-lineage exclusion, and the `bbox_contains` exclusion. Replace the
heightfield leaf test with:

```rust
if let Some(tm) = self.trimeshes.get(&leaf_id) {
    // Trimesh vertices are world-space: local space == world space, no pose.
    if let Some(toi) = tm.cast_local_ray(&ray, dist, true) {
        if toi < dist {
            occluded = true;
            return TraversalAction::EarlyExit;
        }
    }
}
```

(`cast_local_ray(&ray, max_toi, solid) -> Option<f32>` from the
`parry3d::query::RayCast` trait, already imported.)

#### 4h. `is_node_visible`

Only one edit: the early-out `if self.heightfields.is_empty()` becomes
`if self.trimeshes.is_empty()`. The sampling logic is untouched (part 4
rewrites it).

#### 4i. Unit test

Replace `test_heightfield_vertical_raycast` with:

```rust
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
```

Also add a degenerate-input test: `build_trimesh_from_mesh(&[], &[])` must
return `None` (not panic).

## What NOT to change

- `is_node_visible` sampling (Fibonacci block, `CAMERA_SAMPLE_RADIUS`) — part 4.
- The `OCCLUDER_WORLD` static cache keyed by `hash_spawned_nodes` in `lod.rs`
  stays for now — part 3 removes it.
- `tile_impl.rs` — `fetch_map_tile` / `get_tile_collider` are used as-is.

## Verification

1. `cargo check` both crates + `cargo test -p game_logic --features worker visibility`.
2. Manual: with the occluder enabled, distant tiles that are plainly visible
   from a rooftop/freecam vantage must **stop** being culled (previously they
   flipped to raw/coarse even in view). Culling behind buildings must still
   work. The first recompute after startup will be noticeably slower (cold
   tile fetches) — subsequent ones fast; watch for the "Occluder BVH rebuilt
   in N ms" log trending down.
