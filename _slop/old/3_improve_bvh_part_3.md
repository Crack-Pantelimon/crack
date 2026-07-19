# Improve BVH occlusion — Part 3: lock-step LOD walk against the proposed render set

Part 3 of 4. Requires parts 1 and 2 merged (uses `get_or_build_trimesh`,
`Arc<TriMesh>`, `path_to_id`, `base_url` plumbing from part 2).

## Build & check (read first)

All commands run from the repo root `/home/p/VIDOEGAME/crack`. `cargo` may be
provided by an AppImage that misbehaves through its "proxy" wrapper — always
`unset ARGV0` first. Only check the two affected crates:

```sh
unset ARGV0
cargo check -p game_logic --features worker
cargo check -p demo_resolution_selector_web_bevy
```

## Problem

Today `compute_lod_changes` (`crack_demo/game_logic/src/lod.rs`) tests each
split candidate's visibility against **one static `OccluderWorld` built from
the currently spawned tiles + coarse assets** (cached in the `OCCLUDER_WORLD`
static keyed by `hash_spawned_nodes`). But the greedy walk itself starts from
the **root tiles** and simulates its own render set — so a candidate can be
declared occluded (or visible) by tiles the same walk has already decided to
replace. Decisions and occluders are out of sync.

Fix (maintainer's decision — "evolving proposed set"): the occluder set must
be exactly **what the walk has rendered so far**:

- Initialize the `OccluderWorld` with the **root tiles + coarse assets** (not
  the spawned set).
- When the walk **accepts a split** of node N: remove N from the occluder set
  and insert N's children.
- The visibility test for a candidate N runs against the occluder set as it
  stands at that moment (N itself is already excluded by the same-lineage
  rules inside `is_ray_occluded`, so it never occludes itself).

Children just accepted may not have cached trimeshes yet — fetch them
(awaiting `fetch_map_tile` through `get_or_build_trimesh`), concurrently in
chunks of 8, exactly like part 2's rebuild did. These are tiles the client is
about to fetch for rendering anyway, so this pre-warms the tile cache.

## Constraints (same wasm-worker rules as part 2)

- Never hold a lock guard across an `.await`.
- No `tokio::spawn` / `JoinSet`; use `futures::future::join_all`.
- `&mut self` methods cannot be driven by `join_all` — so mesh *acquisition*
  (async, shared) is separated from *insertion* (sync, `&mut self`), see 1b/1c.

## Changes

### 1. `crack_demo/game_logic/src/visibility.rs` — incremental `OccluderWorld`

parry3d 0.27's `Bvh` supports incremental mutation: `Bvh::insert(aabb,
leaf_index)` and `Bvh::remove(leaf_index)` (verified in
`parry3d-0.27.0/src/partitioning/bvh/bvh_insert.rs:126` and
`bvh_tree.rs:2357`). Use them; do NOT rebuild via `from_leaves` per split.

#### 1a. Constructor

```rust
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
}
```

(If `from_leaves` with an empty slice does not compile/behave, use
`Bvh::default()` — check the parry source; either is acceptable as long as
`traverse` on the empty tree is a no-op.)

#### 1b. Sync insertion (mesh already acquired)

```rust
/// Inserts one occluder whose trimesh has already been resolved.
/// No-op if the path is already present.
pub fn insert_occluder(
    &mut self,
    path: &MapTreeNodePath,
    bbox: &BBox,
    trimesh: Arc<TriMesh>,
) {
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
```

#### 1c. Removal

```rust
/// Removes one occluder; no-op if absent.
pub fn remove_node(&mut self, path: &MapTreeNodePath) {
    if let Some(id) = self.path_to_id.remove(path) {
        self.bvh.remove(id);
        self.trimeshes.remove(&id);
        self.id_to_path.remove(&id);
        self.aabbs.remove(&id);
    }
}
```

#### 1d. Delete `rebuild_bvh`

The whole `rebuild_bvh` function is deleted — its candidate-gathering logic
moves to `compute_lod_changes` (step 2c), and its chunked
`get_or_build_trimesh` + insert pattern is reused there. Keep
`get_or_build_trimesh`, `build_trimesh_from_mesh`, `TRIMESH_CACHE`, the
degenerate-bbox guard logic (move it to where candidates are gathered),
`is_ray_occluded`, and `is_node_visible` unchanged.

### 2. `crack_demo/game_logic/src/lod.rs` — the lock-step walk

#### 2a. Delete the static cache

Remove the `OCCLUDER_WORLD` static and `hash_spawned_nodes` (both
`#[cfg(feature = "worker")]`). The world is now rebuilt logically every
compute, but cheaply: all trimeshes come out of `TRIMESH_CACHE`, and BVH
inserts on a few hundred leaves are microseconds.

#### 2b. Asset-list helper

Add a small local helper (closure or fn) used everywhere a node's occluder
assets are needed:

```rust
// (asset_id, glb_path) pairs for one node, skipping assets without a glb.
fn occluder_assets_of(data_res: &MapTreeData, path: &MapTreeNodePath) -> Vec<(String, String)>
```

Implementation: `data_res.all_nodes.get(path)` → for each asset id in
`node.assets`, look up `data_res.assets.get(asset_id)` and keep
`(asset_id.clone(), glb_path.clone())` when `glb_path` is `Some`.

Note: `compute_lod_changes` already receives `data_res: &MapTreeData` — use
it directly instead of `get_manifest_cache()` (they are the same data; this
removes the extra await that `rebuild_bvh` used to do).

#### 2c. Build the initial world (roots + coarse), gated on the flag

Replace the current `occluder_world` construction block with:

```rust
#[cfg(feature = "worker")]
let mut occluder_world = if req.enable_visibility_cull {
    let t_start = _crack_utils::get_timestamp_now_ms();
    let mut world = crate::visibility::OccluderWorld::new_empty();

    // initial render set = root tiles + coarse horizon assets
    let mut initial: Vec<(MapTreeNodePath, BBox, Vec<(String, String)>)> = Vec::new();
    for root in &data_res.roots {
        if let Some(node) = data_res.all_nodes.get(root) {
            initial.push((root.clone(), node.bbox, occluder_assets_of(data_res, root)));
        }
    }
    for asset in &data_res.coarse_assets {
        if let Some(glb) = /* asset glb_path, see part 2 note on MapTreeAssetInfo */ {
            initial.push((
                asset._octant_path.clone(),
                asset.bbox,
                vec![(asset.name.0.clone(), glb)],
            ));
        }
    }

    // same degenerate-bbox skip as before (extent_x/extent_z < 1e-3 → drop)

    for chunk in initial.chunks(8) {
        let futs = chunk.iter().map(|(path, _bbox, assets)| {
            crate::visibility::get_or_build_trimesh(path, assets, &req.base_url)
        });
        let metas = futures::future::join_all(futs).await;
        for ((path, bbox, _), tm) in chunk.iter().zip(metas) {
            if let Some(tm) = tm {
                world.insert_occluder(path, bbox, tm);
            }
        }
    }
    let dt = _crack_utils::get_timestamp_now_ms() - t_start;
    tracing::info!("Occluder world (lock-step) built in {} ms ({} leaves)", dt, world.trimeshes.len());
    Some(world)
} else {
    None
};
```

When `enable_visibility_cull` is **false**, no occluder work and no fetches
may happen at all (exactly as today).

#### 2d. The walk: test against, then mutate, the evolving set

Inside the `while let Some((_score, node_path)) = heap.pop()` loop:

- **Remove the `visibility_cache`.** Each node is pushed to the heap exactly
  once (roots once; children once when their parent splits), and with an
  evolving occluder set a cached answer would be stale anyway. Delete the
  `BTreeMap` and its lookups.
- The visibility test itself is unchanged in shape:

```rust
#[cfg(feature = "worker")]
let vis = if let Some(ref world) = occluder_world {
    let bbox = tile_bbox(&node_path);
    world.is_node_visible(&bbox, &node_path, &req.cameras)
} else {
    true
};
```

- **On an accepted split** (the branch that currently does
  `proposed_nodes.remove/insert`, `proposed_splits.insert`, budget update,
  push children on heap) additionally update the occluder set:

```rust
#[cfg(feature = "worker")]
if let Some(ref mut world) = occluder_world {
    world.remove_node(&node_path);
    // resolve children trimeshes concurrently, then insert
    let child_meta: Vec<(MapTreeNodePath, BBox, Vec<(String, String)>)> = children
        .iter()
        .map(|c| (c.clone(), tile_bbox(c), occluder_assets_of(data_res, c)))
        .collect();
    for chunk in child_meta.chunks(8) {
        let futs = chunk.iter().map(|(p, _b, a)| {
            crate::visibility::get_or_build_trimesh(p, a, &req.base_url)
        });
        let metas = futures::future::join_all(futs).await;
        for ((p, b, _), tm) in chunk.iter().zip(metas) {
            if let Some(tm) = tm {
                world.insert_occluder(p, b, tm);
            }
        }
    }
}
```

  Factor this children-insertion block into a small `async fn` if the borrow
  checker allows it cleanly; otherwise inline is fine. Note `children` is
  cloned earlier in the loop — reuse that.

- **On a rejected-for-visibility split** (the `culled_nodes.push` branch):
  do **not** mutate the occluder set — the parent stays rendered, which is
  exactly the lock-step semantics.

- The order note: currently `is_visible` is computed *before* the
  budget/validity `if`. Keep the existing short-circuit structure
  (`new_budget <= budget && is_valid_split(...)` first, visibility second) so
  we never fetch/raycast for nodes that fail the cheap checks. Since the
  visibility computation is now behind `.await`s (child fetches happen only
  on acceptance), the loop body becomes async — `compute_lod_changes` is
  already `async fn`, so this is just code motion, no signature change.

- Everything after the walk (resolved splits, merges, dedup, response) is
  untouched.

#### 2e. `#[cfg]` hygiene

The non-worker build (`cargo check -p game_logic` without the feature is NOT
required, but the demo crate compiles `game_logic` without `worker` — check
how it's declared) must still compile: keep every occluder-related statement
behind `#[cfg(feature = "worker")]` exactly as the current code does, and
keep the `#[cfg(not(feature = "worker"))] let vis = true;` fallback.

## What NOT to change

- `is_node_visible` / `is_ray_occluded` internals (part 4 touches sampling).
- The client (`lod_flow.rs`) — no protocol change in this part.
- Merge-request logic, `culled_nodes` reporting, budget accounting.

## Verification

1. Both `cargo check`s + `cargo test -p game_logic --features worker`.
2. Manual: enable the occluder and watch the 3D BVH Minimap while walking
   around dense buildings. Expected differences from before:
   - Splits behind a wall stop happening even when the *old spawned set*
     wouldn't have occluded them — culled (blue) boxes should appear at
     coarser levels (parents refuse to split) rather than only at leaves.
   - No flip-flopping between consecutive recomputes at a stationary camera
     (the walk is deterministic given the same camera and manifest).
   - First recompute after a fresh page load is slower (root+coarse trimesh
     fetches), subsequent ones fast.
