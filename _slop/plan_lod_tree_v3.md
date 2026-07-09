# LOD Visibility-Cull — Bug Review & Fix Specification (v3)

## 0. What this document is

The v1 plan (`_slop/plan_lod_tree_v1.md`) was implemented by another model. The result
is in:

- `crack_demo/game_logic/src/visibility.rs` (new)
- `crack_demo/game_logic/src/lod.rs` (`compute_lod_changes`)
- `crack_demo/game_logic/src/worker/tile_impl.rs` (`get_tile_collider`)
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/lod_flow.rs`

Observed symptom: with `enable_visibility_cull = true` (hard-coded on in
`lod_flow.rs:130`), the whole map is stuck at a very low level of detail — tiles near
the camera never subdivide.

This document (a) states the root cause, (b) lists every defect with file/line and the
mechanism by which it breaks, and (c) gives specific, ordered fixes. It is written so it
can be handed to an implementer directly.

---

## 1. Root cause (summary)

`compute_lod_changes` now gates every split on `OccluderWorld::is_node_visible(...)`
(`lod.rs:180-190`). A split is accepted only if the node is judged "visible". The
visibility test is **biased toward OCCLUDED** in several independent ways, so it returns
`false` for tiles that are plainly in view. An occluded parent is never split, and
because children only enter the heap when their parent splits, an entire subtree is
pruned. The net effect is that the tree never descends past the coarse root tiles — "very
low level of detail, stuck."

The plan (§2.2) was explicit that the test **must be conservative toward VISIBLE** ("the
failure we must avoid is culling something the player can actually see"). The
implementation does the opposite. The specific mechanisms are D1–D5 below; D1 and D2 are
each independently sufficient to cause the stuck-LOD symptom.

---

## 2. Defects

### D1 — A tile is occluded by its own coarser self (self-lineage not excluded) — PRIMARY

`visibility.rs:239`:

```rust
if path.0.starts_with(&exclude_path.0) {
    return TraversalAction::Continue; // treat as "not an occluder"
}
```

This excludes the target node and its **descendants** (paths that extend
`exclude_path`). It does **not** exclude the target's **ancestors**, nor the coarse-shell
tile that covers the same ground column.

- `exclude_path` = the node under test `T`.
- An ancestor `A` of `T` has a path that is a *prefix* of `T`. `A.starts_with(T)` is
  `false`, so `A` is **kept as an occluder**.
- The occluder set (`rebuild_bvh`, `lod.rs:81`) is built from `spawned_nodes` **plus
  `data_res.coarse_assets`**. `coarse_assets` are, by definition
  (`map.rs:58`, "Coarse horizon tiles… kept worker-side"), coarser representations of the
  *same world surface*. Their heightfields sit at essentially the same height as `T`'s
  surface over the same XZ column.

Consequence: when we test whether to split terrain tile `T`, we cast a ray from the
camera down to a point *on* `T`'s surface, and that ray is blocked by the coarser
heightfield of the *same terrain* (an ancestor node and/or an overlapping coarse asset).
`T` is declared occluded → never split. At initial load the only spawned tiles are the
coarse roots, and each root is occluded by the coarse shell of its own column, so the
tree never leaves the root level. This is the dominant cause of the stuck-LOD symptom.

**Fix:** exclude any occluder on the same root-to-leaf lineage as the target, and any
occluder whose AABB spatially contains the target (a container cannot validly occlude its
own contents). See F1.

### D2 — `is_node_visible` fails **closed** (returns "occluded") on empty input

`visibility.rs:266-319`. The function loops over `cameras`; if `cameras` is empty it falls
through to `false` (occluded). Likewise, if the occluder BVH happens to contain the
target's own geometry only (D1) every ray is blocked and it returns `false`.

A visibility test used as a hard gate must **fail open** (default VISIBLE) whenever it
cannot make a confident *occluded* determination — no cameras, no occluders, degenerate
AABB, ray-length underflow, etc. As written, any such case freezes LOD.

**Fix:** F2 — early-return `true` when `cameras.is_empty()` or the occluder world is
empty.

### D3 — The occluder BVH is rebuilt from scratch on **every** request (hot path) + O(n²)

`lod.rs:79-86` calls `OccluderWorld::rebuild_bvh(...)` inside `compute_lod_changes`, i.e.
on every LOD request (every time the camera moves past the quantization grid — many times
per second). `rebuild_bvh` (`visibility.rs:129-214`):

- iterates `spawned_nodes + coarse_assets` and, for each **coarse** candidate, does a
  linear `coarse_assets.iter().find(...)` (`visibility.rs:156`) → **O(n²)** over the coarse
  set (hundreds–thousands of tiles).
- calls `get_manifest_cache().await` **inside the per-candidate loop** (`visibility.rs:162`)
  instead of once.
- takes the `HEIGHTMAP_CACHE` write lock (`visibility.rs:139`) for the entire build and
  rebuilds/clones heightfields.

The plan (§5) required the occluder structure to be built **once** and reused. Even after
the correctness fixes, doing this per request will blow the 12 ms budget
(`lod.rs:285`) and starve LOD updates.

**Fix:** F3 — cache the `OccluderWorld` in a worker-global keyed on the spawned-node set;
rebuild only when that set changes. Query the manifest once; precompute a
`path → &MapTreeAssetInfo` map for coarse assets.

### D4 — Occluder meshes are looked up by the wrong key, so most occluders are silently missing

Occluder geometry is fetched via `get_tile_collider(&path.0)` (`visibility.rs:178`), which
reads `TILE_CACHE` (`tile_impl.rs:128-132`). But `TILE_CACHE` is **keyed by
`asset.name.0`** (the asset id), inserted by `fetch_map_tile`
(`tile_impl.rs:112`, `map_lod.rs:89` `tile_id = asset.name.0`), **not** by node path.

- For a node with multiple assets, or any node whose asset id string ≠ its octant path
  string, the lookup misses and the tile contributes **no** occluder.
- `coarse_assets` are requested through `FetchFakeMapTiles`
  (`manifest_impl.rs:116`), which returns metadata only; their GLBs are not guaranteed to
  be in `TILE_CACHE` at all, so `get_tile_collider(&asset._octant_path.0)` misses.

So the *intended* coarse-shell occluder set is largely empty, while the occluders that do
get through are precisely the currently-spawned tiles — which are the ones being tested
(feeding D1). The occluder set is both wrong *and* self-defeating.

**Fix:** F4 — resolve a node's asset ids from the manifest and look colliders up by
**asset id** (matching how they were inserted), combining a node's assets into one
occluder. Skip occluders whose mesh is genuinely unavailable rather than letting the miss
distort the set.

### D5 — Heightfields cannot represent the occluders the plan called for

The plan (§3.1, §4.2) specified `parry3d::shape::TriMesh` occluders with exact
ray-vs-triangle casts. The implementation instead rasterizes each tile into a 64×64
`HeightField` (`visibility.rs:26-117`, `build_heightfield_from_mesh`). A heightfield is a
single-valued top-down surface (one max height per XZ cell). It **cannot** represent:

- vertical building faces / walls (the main urban occluders),
- anything occluding geometry *behind* it at a similar height,
- overhangs.

It also introduces its own hazards:
- unfilled cells default to `bbox.min.y` (`visibility.rs:36`), creating a flat "floor"
  occluder across the whole tile footprint at the AABB bottom;
- the empty-cell sentinel is `== bbox.min.y` (`visibility.rs:91`), so a legitimately
  low sample is mistaken for a hole;
- the row/col/scale axis mapping (`idx = i + j*nrows`, `Array2::new(nrows, ncols, …)`,
  `HeightField::new(heights_zx, scale)`) is unverified and can transpose the surface.

For terrain-behind-a-hill culling (the easy win) a heightfield is adequate; for buildings
it is not. This is a design decision to make explicitly (F5), not silently.

### D6 — Visibility is computed before the cheap distance gate (wasted ray casts)

`lod.rs:179-190`: `is_visible` is evaluated **before** `is_valid_split`. Ray casting is
far more expensive than the distance/LOD-cap check. Nodes that distance does not even want
to split still pay for a full ray bundle.

**Fix:** F6 — reorder so visibility is the **last** term:
`new_budget <= budget && is_valid_split(&node_path) && is_visible(...)`, and rely on
short-circuit evaluation. Add per-request memoization of the visibility verdict.

---

## 3. Fixes (specific)

### F1 — Exclude the target's whole lineage and any containing occluder (fixes D1)

In `OccluderWorld::is_ray_occluded` (`visibility.rs:217-262`), replace the exclusion test
at line 239:

```rust
// OLD
if path.0.starts_with(&exclude_path.0) {
    return TraversalAction::Continue;
}
```

with an exclusion that drops occluders on the same lineage (ancestor **or** descendant
**or** self):

```rust
// NEW
let same_lineage = path.0.starts_with(&exclude_path.0)   // target or its descendant
    || exclude_path.0.starts_with(&path.0);              // an ancestor of the target
if same_lineage {
    return TraversalAction::Continue;
}
```

Additionally, exclude occluders that spatially contain the target. Pass the target AABB
into `is_ray_occluded` and, for each candidate leaf, skip it when its stored AABB fully
contains the target AABB (inflated by a small epsilon). This catches overlapping
`coarse_assets` whose path strings are unrelated to the target's path. A candidate whose
AABB strictly contains the node cannot be a legitimate *external* occluder of that node.

Rationale: matches plan §2.3/§3.1 ("occlusion must be by geometry *outside* `T`'s AABB").

### F2 — Fail open (fixes D2)

At the top of `is_node_visible` (`visibility.rs:266`):

```rust
if cameras.is_empty() {
    return true;               // no camera constraint => cannot cull
}
if self.bvh.leaf_count() == 0 || self.heightfields.is_empty() {
    return true;               // no occluders => everything visible
}
```

(Use whatever "is the BVH empty" accessor parry exposes; if none, track a
`self.leaf_count: usize` populated in `rebuild_bvh`.)

Also keep the existing behavior that any single unobstructed ray returns `true`
immediately — that is correct and conservative; do not change it.

### F3 — Build the occluder world once, cache it, keep it off the hot path (fixes D3)

1. Add a worker-global cache:

```rust
static OCCLUDER_WORLD: RwLock<Option<(u64, Arc<OccluderWorld>)>> = RwLock::const_new(None);
```

where the `u64` is a hash of the spawned-node set (e.g. hash the sorted `spawned_nodes`).
`coarse_assets` are static per manifest, so they do not need to enter the key.

2. In `compute_lod_changes` (`lod.rs:79-86`), replace the unconditional `rebuild_bvh` with
   "get-or-build": compute the key, read-lock the cache, reuse if the key matches,
   otherwise build once, store, and use. Wrap the world in `Arc` so the heap loop borrows a
   snapshot without holding the lock.

3. Inside `rebuild_bvh`:
   - call `get_manifest_cache().await` **once** before the loop (`visibility.rs:162`);
   - build a `HashMap<&MapTreeNodePath, &MapTreeAssetInfo>` from `coarse_assets` up front
     and use it instead of the O(n) `.find()` (`visibility.rs:156`).

4. Log build time and leaf/triangle count once per rebuild (not per request), as the plan
   asked (§9 step 3).

### F4 — Look occluder meshes up by asset id, not node path (fixes D4)

In `rebuild_bvh`, do not call `get_tile_collider(&path.0)`. Instead:

- For a **spawned node** `path`: read `manifest.all_nodes[path].assets` and call
  `get_tile_collider(&asset_id.0)` for each asset id, merging the returned
  `MeshColliderData` into one occluder mesh for that node (concatenate vertices, offset
  indices — same pattern as `extract_collider_data` in `tile_impl.rs:39-48`).
- For a **coarse asset**: call `get_tile_collider(&asset.name.0)` (its `name`, i.e. the key
  used at insertion), **not** `_octant_path`.

If a node has *no* asset collider available in cache, skip it as an occluder (it simply
does not contribute) — do **not** substitute a bbox-only box, which would over-occlude.

Note: this makes occluders depend on what the client has fetched. That is acceptable for a
first correct version, but document it: a tile that is not yet fetched cannot occlude. If
we later want coarse tiles as reliable occluders regardless of client fetch state, the
worker must fetch their GLBs itself (plan §4.3 / §5) — out of scope for the unstick fix.

### F5 — Occluder representation decision (addresses D5)

Choose one and record it in code comments:

- **Option A (recommended for the immediate fix):** keep heightfields, but treat this as a
  **terrain-occlusion-only** feature. Fix the three heightfield hazards: (i) initialize
  empty cells to `f32::MIN` (or a NaN sentinel) rather than `bbox.min.y`, and treat unset
  cells as *no surface* (do not occlude) instead of a floor; (ii) detect holes by that
  sentinel, not by `== bbox.min.y`; (iii) add a unit test that casts a known vertical ray
  through a flat heightfield to confirm the row/col/scale axis mapping and pose are
  correct before trusting any cull.
- **Option B (matches plan, larger change):** replace `HeightField` with
  `parry3d::shape::TriMesh` per occluder and use `TriMesh::cast_ray`. This is required if
  buildings must occlude. Heavier memory/build cost; do only after A proves the pipeline.

Given the goal is to un-stick LOD, ship Option A first; revisit Option B if building
occlusion is needed.

### F6 — Gate order + memoization (fixes D6)

In `compute_lod_changes` (`lod.rs:179-190`):

- Reorder the accept condition to
  `if new_budget <= budget as usize && is_valid_split(&node_path) && is_visible { … }`,
  computing `is_visible` lazily (only when the first two pass) so short-circuit avoids ray
  casts on nodes distance rejects.
- Add a request-scoped `BTreeMap<MapTreeNodePath, bool>` visibility memo, analogous to
  `score_cache` (`lod.rs:104`), keyed by node path.

---

## 4. Ordered implementation checklist

1. **F2 + F6** (smallest, immediately un-sticks LOD in the common case): fail-open on empty
   cameras/occluders, reorder the gate. Verify the map subdivides again.
2. **F1**: exclude self-lineage + containing occluders. Verify terrain still subdivides
   when occluders are present.
3. **F4**: fix the collider cache key so occluders are actually loaded. Verify occluders
   appear (log leaf count > 0).
4. **F3**: move the build off the hot path + cache + drop the O(n²)/per-candidate manifest
   fetch. Verify `compute_lod_changes` stays under 12 ms in steady state.
5. **F5 Option A**: harden the heightfield (sentinel, hole detection, axis unit test).
6. Only then, if building occlusion is required, **F5 Option B** (TriMesh).

Each step is independently testable; after step 1 the stuck-LOD symptom should be gone.

---

## 5. Verification

- **Unit (visibility.rs):**
  - Empty `cameras` ⇒ `is_node_visible == true`.
  - Empty occluder world ⇒ `is_node_visible == true`.
  - A target with only its own coarser ancestor in the occluder set ⇒ `true` (F1).
  - A box occluder squarely between a single camera and the target's every corner ⇒
    `false`; move the camera to the same side ⇒ `true`.
  - Vertical ray through a flat heightfield hits at the expected height (axis-mapping
    guard, F5A).
- **Integration:** run the app (see `/run`) with `enable_visibility_cull = true`; confirm
  tiles near the camera reach full LOD and only tiles behind terrain stay coarse. Toggle
  the flag off in `lod_flow.rs:130` and confirm identical near-camera LOD (no false culls).
- **Perf:** confirm `compute_lod_changes` logs no longer trip the 12 ms warning
  (`lod.rs:285`) during continuous camera motion; the BVH build logs at most once per
  spawned-set change.

---

## 6. Notes / decisions to confirm with the author

- **Kill switch:** `enable_visibility_cull` is hard-coded `true` (`lod_flow.rs:130`).
  Consider surfacing it as a runtime toggle (a field on `MapLODState`) so it can be
  disabled without a rebuild while validating.
- **Coordinate space** is fine as-is: tiles spawn at `Transform::from_xyz(0,0,0)`
  (`map_lod.rs:38`) with world-space GLB vertices, and the heightfield uses the world-space
  `bbox`, so occluders and `reference_points`/`cameras` share one space. No transform
  needed; keep the F5A axis unit test to prove the heightfield local mapping.
- **Worker-fetched coarse occluders** (plan §4.3/§5) are deliberately out of scope here;
  F4 only uses colliders the client already fetched. Track as a follow-up if building/coarse
  occlusion must be independent of client fetch state.
</content>
</invoke>
