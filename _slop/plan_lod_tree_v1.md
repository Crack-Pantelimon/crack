# Plan: Visibility-Aware LOD Tree Pruning (v1)

## 0. Motivation & summary

Today the worker's LOD decision (`game_logic/src/lod.rs::compute_lod_changes`) scores
every octree node using **distance only**. A node is split when its
`bbox_diagonal / distance_to_reference` exceeds a threshold, subject to a global
asset budget served by a max-heap. Distance to a reference point is the *only*
signal — there is no notion of **occlusion**. As a consequence we happily pay to
split (fetch + spawn + build collider for) tiles that are behind a building, under
the ground, or on the far side of a hill, purely because they are geometrically
close to a reference point.

We want to add a second, orthogonal signal: **is this tile actually visible from
any camera position the player can reach?** The player/look target is a
reference point, but the camera is not *at* the reference point — it orbits/roams
within some radius `r` of it. So the real question is:

> Given the ball `B(ref, r)` of all reachable camera positions around a reference
> point, is tile `T` visible (has unobstructed line of sight) from **at least one**
> point of `B`, considering all other tiles as occluders?

If the answer is *no regardless of how far we would subdivide `T`'s parent*, then
we must **not split that parent** — its children can never be seen, so spending
budget on them is pure waste. This lets us keep the same asset budget but spend it
entirely on tiles the player can actually see, effectively raising achievable LOD
in the visible set.

This plan covers:

1. The geometry & the accelerated visibility algorithm (with alternatives &
   complexity analysis).
2. Loading tile **physics meshes** into a worker-side physics/collision library
   (parry3d) and building an acceleration structure once per manifest.
3. Changes to `LodComputeRequest` / `compute_lod_changes` to consume a per-reference
   **camera max range** and to prune occluded subtrees.
4. Client integration into all three camera controllers (free roam, pedestrian,
   car) — including how each derives its correct radius `r`.
5. Complexity & runtime estimate for ~400k tiles.

---

## 1. Current system (as-built) — reference

Data model (`game_logic/src/map.rs`):

- `MapTreeData` holds `all_nodes: BTreeMap<MapTreeNodePath, MapTreeNodeInfo>`,
  `children`, `parents`, `roots`, and `assets`. Each `MapTreeNodeInfo` has a
  `bbox: BBox { min: Vec3, max: Vec3 }`. Octree paths are strings; a child path is
  the parent path plus one octant char. ~400k leaf assets (`level >= 14`), coarser
  levels kept separately as `coarse_assets`.
- The worker caches the whole tree in `MANIFEST_CACHE` (`manifest_impl.rs`), built
  once from `manifest.parquet`.

LOD request/response (`game_logic/src/lod.rs`):

```rust
pub struct LodComputeRequest {
    pub spawned_nodes: BTreeSet<MapTreeNodePath>,
    pub reference_points: Vec<Vec3>,
    pub lod_budget: u32,
    pub max_lod: i32,
    pub tiles_per_diagonal: f32,
}
```

`compute_lod_changes`:

- `tile_score(node)` = `-(min_dist_to_refs + 50) / bbox_diagonal` — max-heap pops
  the *closest/biggest* node first.
- Greedy: pop node, if splitting keeps `current_budget <= budget` **and**
  `is_valid_split(node)` (LOD cap + `bbox_diagonal/dist` threshold), accept the
  split, push children.
- Produces `split_requests` (for nodes currently spawned client-side) and
  `merge_requests` (spawned descendants of a node that should collapse).

Client flow (`crack_plugin/lod_flow.rs`): each frame gathers spawned
`TreeMapTile` node paths + `MapLODState.reference_points` + the camera translation,
dedupes against last request, and RPCs `ComputeLodChanges`. Result drives
split/merge/fetch. Tile GLBs are fetched in the worker (`tile_impl.rs`,
`extract_collider_data` already produces `MeshColliderData { vertices, indices }`)
and the **client** builds `avian3d::Collider::try_trimesh` colliders
(`map_lod.rs:350`).

**Key observation:** the worker already knows how to turn a tile GLB into a
triangle mesh (`extract_collider_data`). We will reuse exactly that path to feed a
worker-side collision world.

---

## 2. The geometry of "visible from a sphere of camera positions"

### 2.1 Problem statement

For a reference point `ref` with camera reach radius `r`, define the camera ball
`B = B(ref, r)`. A candidate tile `T` (AABB `T.bbox`, and optionally its triangle
mesh) is **potentially visible** iff:

∃ camera point `p ∈ B`, ∃ surface point `q ∈ T` such that the open segment
`(p, q)` does not intersect any *other* occluding geometry closer than `q`.

We deliberately ignore the view frustum / camera orientation: the controllers let
the player rotate freely, so any direction is reachable. Only **line-of-sight
occlusion** matters.

### 2.2 Correctness direction (critical)

The failure we must avoid is **culling something the player can actually see**
(hard popping). Therefore the test must be **conservative toward "visible"**: when
in doubt, keep it. We only cull when we are confident `T` is occluded from the
*entire* ball `B`. Concretely:

- Inflate occluders slightly *inward* only, never outward (do not let an occluder
  grow and hide a visible tile). Actually: to stay conservative we must **under-**
  estimate occluder coverage, i.e. treat occluders as slightly smaller / shrink the
  ball we test so a real gap is never missed. See §3.4.
- Sampling-based tests (rays) can miss a thin sightline. Mitigate with (a) adequate
  sample density on `B`, (b) targeting rays at tile AABB **corners + center + edge
  midpoints** (the extremal silhouette), and (c) a per-node *hysteresis*: only cull
  after N consecutive "occluded" verdicts, un-cull immediately on one "visible".

### 2.3 Why this prunes whole subtrees

Visibility is (approximately) **monotone under subdivision for occlusion by
*other* tiles**: if the parent's AABB is fully occluded from `B`, every child AABB
(⊆ parent AABB) is also fully occluded from `B`. So a single occlusion test on the
parent lets us prune the *entire subtree* — we never even consider its children in
the heap. This is what makes the whole thing cheap: we test coarse nodes first and
cut early. (Self-occlusion within `T` is intentionally *not* used to cull, since a
child could poke out; we only use occlusion by geometry outside `T`'s AABB.)

---

## 3. Accelerated visibility algorithms — survey & choice

We need "is AABB `T` visible from ball `B`, given ~400k triangle-soup occluders".
Options, from most to least suitable for a CPU worker:

### 3.1 Option A — Ray-bundle occlusion query over a BVH (CHOSEN)

Build one **BVH/QBVH over occluder geometry** once per manifest (parry3d's
`Qbvh`). For a candidate node:

1. Generate a small set of **camera sample points** `P ⊂ B`: the center `ref`, plus
   a Fibonacci-sphere of `K` points on the surface of `B` (K ≈ 8–32). Optionally
   only the hemisphere facing `T` (the far side of `B` is strictly worse for seeing
   `T` only if occluders are between — keep full sphere for safety, but we can
   prune back-facing samples whose direction to `T` center is > 90° from... no:
   keep it simple, use the surface points on the `T`-facing hemisphere plus center;
   §3.4 explains why this is conservative if we also test `ref`).
2. Generate **target points** `Q` on `T`: 8 AABB corners + center (+ 12 edge
   midpoints for large tiles). 
3. For each `(p, q)` cast a segment/ray query against the QBVH, but **exclude
   triangles belonging to `T` itself and to `T`'s descendants** (occlusion must be
   by *other* geometry). If any ray reaches its `q` with no earlier hit → **VISIBLE,
   early-out**. If *all* `K·|Q|` rays are blocked → **OCCLUDED**.

Ray-vs-QBVH is `O(log N)` amortized. Cost per node ≈ `K·|Q|` ray casts. With
K=16, |Q|=9 → 144 rays/node, each ~`log2(400k)≈19` node visits. Early-out makes the
*visible* case (the common case near the player) very cheap (often the first ray to
the center hits nothing).

Pros: reuses existing mesh extraction, simple, conservative-tunable, no GPU.
Cons: sampling can miss thin sightlines (mitigated by §2.2); building a 400k-tri
BVH is memory-heavy (see §4 — we use a *coarse occluder set*, not all 400k leaves).

### 3.2 Option B — Shadow-frustum / penumbra (tangent-cone) culling

The exact region occluded from a *ball* light `B` by a convex occluder `O` is the
**umbra**, bounded by internal tangent lines between `B` and `O` (a generalized
shadow volume / anti-penumbra). Test "is `T` fully inside the fused umbra of all
occluders". This is the *exact* analytic version of what rays approximate.

Pros: conservative and exact per-occluder; no sampling gaps.
Cons: **umbra fusion** (combining many occluders' shadows) is the hard, classically
expensive part (this is the "from-region visibility"/PVS problem). Full fusion is
impractical for 400k dynamic-ish occluders per frame. We adopt its *idea* (treat
`B` as an area source, shrink it) as the conservative correction in Option A rather
than implementing full fusion.

### 3.3 Option C — Rasterized occlusion / HZB / software depth buffer

Render occluders into a small software depth buffer from representative viewpoints
and test tile AABBs against it (à la Umbra / hierarchical Z, or Frostbite's
software occlusion). Very fast for many queries, standard in engines.

Pros: extremely fast amortized; handles many occluders naturally.
Cons: it's *from-point* (per camera position), not *from-region*; to cover the ball
`B` we'd rasterize from several viewpoints anyway, re-introducing sampling. Needs a
software rasterizer in the worker (new, non-trivial, and awkward in WASM). Deferred
to a possible v2; overkill for our per-request node counts.

### 3.4 Making Option A conservative w.r.t. the ball (from-region → from-point)

Casting rays only from a finite sample set `P` can wrongly declare OCCLUDED when a
sightline exists from an unsampled `p`. Two cheap safeguards make false-cull
vanishingly unlikely:

- **Shrink the ball, not the tile.** Testing rays only from the surface + center of
  `B` approximates "visible from anywhere in `B`" well *because visibility is
  monotone in `r`* for external occluders: if some interior point sees `T`, then
  moving outward along the ray toward the tile-facing surface keeps line of sight
  unless a new occluder intervenes — which only *reduces* the surface's ability to
  see, so we additionally always test the **center `ref`** (the innermost point) and
  a **tile-facing surface cap**. Union of "center sees it" OR "any cap sample sees
  it" ⇒ keep. This over-keeps (safe).
- **Corner/edge targeting + AABB inflation of the *target* tile.** Inflate `T`'s
  AABB by a small epsilon (e.g. one child-tile diagonal) when generating `Q`, so a
  child poking just outside the parent AABB still counts as visible.

Net: the test is biased toward VISIBLE. Residual risk handled by hysteresis (§2.2).

### 3.5 Which occluders? (the 400k problem)

We do **not** put all ~400k leaf meshes in the BVH — that is too much memory and
build time, and most leaves are unloaded anyway. Instead:

- **Occluder set = the currently-realizable coarse shell.** Use the meshes of the
  nodes at a *bounded depth* (e.g. the `coarse_assets` already kept worker-side,
  plus currently-spawned nodes' meshes) as occluders. Buildings/terrain at coarse
  LOD are excellent occluders and are cheap (a few thousand tiles, not 400k).
- Occluder geometry is loaded lazily and cached (see §5). A city block's coarse tile
  occludes everything behind it regardless of the fine LOD we might have wanted — so
  coarse occluders are sufficient to make the *cull* decision.

This keeps the BVH at ~a few k–tens-of-k triangles-worth of tiles, not 400k.

---

## 4. Worker-side physics/collision: loading tile meshes

### 4.1 Library choice: `parry3d`

- Add `parry3d` (pure-Rust, compiles to WASM; it's what `avian3d` wraps under the
  hood via `parry`). We only need geometry queries, not dynamics, so `parry3d`
  alone (no `rapier`) is the minimal dependency.
- New optional dep behind the existing `worker` feature in
  `game_logic/Cargo.toml`:
  ```toml
  parry3d = { version = "0.17", optional = true }   # track glam/nalgebra compat
  ```
  Note: parry uses `nalgebra` types (`Point3`, `Vector3`); add tiny glam⇄nalgebra
  convert helpers (`Vec3 -> Point3<f32>`). Keep this isolated in a new module.

### 4.2 New module: `game_logic/src/visibility.rs`

Responsibilities:

- `struct OccluderWorld` holding a `parry3d::partitioning::Qbvh<u32>` (leaf id =
  occluder index) plus a `Vec<TriMesh>` (one `parry3d::shape::TriMesh` per occluder
  tile) and a map `leaf_id -> MapTreeNodePath` (so we can exclude self-hits).
- `fn build_occluder_world(meshes: &[(MapTreeNodePath, MeshColliderData)]) ->
  OccluderWorld` — builds each tile's `TriMesh` and inserts its AABB into the QBVH.
- `fn segment_hits_other(&self, p, q, exclude_prefix: &str) -> bool` — traverse the
  QBVH with a ray/segment, for each candidate leaf whose path does **not** start
  with `exclude_prefix` (i.e. not `T` or a descendant), do exact ray-vs-TriMesh; if
  a hit at `toi < |p-q|` exists → blocked. (Use parry's `RayCast`/`cast_local_ray`
  and `Qbvh::traverse`/`intersect_ray` visitor.)
- `fn is_node_visible(&self, node_bbox: &BBox, node_path: &str, cams: &[Camera]) ->
  bool` — the ray-bundle test of §3.1, early-out on first clear ray.

Where `Camera { center: Vec3, radius: f32 }` is one reference sphere.

### 4.3 Reusing the mesh source

`tile_impl::extract_collider_data(glb_bytes) -> MeshColliderData { vertices:
Vec<[f32;3]>, indices: Vec<[u32;3]> }` already exists and runs in the worker. We
feed exactly this into `parry3d::shape::TriMesh::new(points, indices)`.

For occluders we need the tile GLBs. The coarse occluder tiles' GLB paths are in
`MapTreeData` (`assets[*].glb_path`, `coarse_assets`). The worker fetches them via
the same `http_get_bytes` used by `fetch_map_tile`, then `extract_collider_data`.
Cache aggressively (§5) — this is the main cost and must be amortized.

### 4.4 Coordinate space

All tile bboxes and mesh vertices are already in the game's Bevy world space (the
same space `reference_points` live in — see `lod_flow.rs` pushing
`camera.translation`). No transform needed; feed raw. (Verify tile GLB local vs.
world: `extract_collider_data` reads raw positions; if tiles are pre-baked in world
space this is fine — **validate during implementation** by casting one known ray.)

---

## 5. Caching & lifecycle (make it cheap enough to run every request)

- **Occluder BVH cache:** a worker-global `OnceCell`/`RwLock<Option<Arc<OccluderWorld>>>`
  keyed off the manifest, analogous to `MANIFEST_CACHE`. Built lazily from the
  coarse occluder set on first LOD request after manifest load. Because coarse
  occluders are a fixed, bounded set, the BVH is built **once** and reused for every
  subsequent `compute_lod_changes` call.
- **Mesh fetch cache:** reuse/extend `tile_impl`'s `TILE_CACHE` (LRU) so occluder
  GLBs fetched for physics are shared with normal tile fetches.
- **Per-node visibility memoization within a request:** `BTreeMap<MapTreeNodePath,
  bool>` like the existing `score_cache`. Because we test parents before children
  (heap pops coarse-first) and prune subtrees, most nodes are never tested.
- **Cross-request cache with invalidation:** cache `(node_path, quantized_ref_set) ->
  visible` for a few frames. Camera moves smoothly; quantize each reference
  center/radius to a grid (e.g. 4 m / 8 m buckets) so small jitters reuse results.
  Invalidate when the occluder set changes.

---

## 6. Request/response API changes

### 6.1 `LodComputeRequest` (in `game_logic/src/lod.rs`)

Replace the flat `reference_points: Vec<Vec3>` semantics with per-reference camera
reach. Backward-compatible, additive:

```rust
pub struct CameraReference {
    pub center: Vec3,     // the look/anchor point (old reference_point)
    pub max_range: f32,   // radius r of reachable camera positions around center
}

pub struct LodComputeRequest {
    pub spawned_nodes: BTreeSet<MapTreeNodePath>,
    pub reference_points: Vec<Vec3>,      // KEEP for scoring (§6.3) — or derive from cameras
    pub cameras: Vec<CameraReference>,    // NEW: drives visibility pruning
    pub lod_budget: u32,
    pub max_lod: i32,
    pub tiles_per_diagonal: f32,
    pub enable_visibility_cull: bool,     // NEW: feature flag / kill switch
}
```

The user's phrasing "the request will now also receive the max range of the camera
to any of the given reference points" ⇒ each reference gets its `max_range`. Keep a
single `Vec<CameraReference>`. `reference_points` stays for the *distance* score, or
we compute the score from `cameras[i].center` and drop the duplicate — pick during
implementation (dropping the duplicate is cleaner). The `enable_visibility_cull`
flag lets us A/B and ship dark.

### 6.2 `compute_lod_changes` integration

Add one gate inside the heap loop, right where a split is currently accepted
(`lod.rs:157`, the `if new_budget <= budget && is_valid_split(&node_path)` branch):

```rust
if new_budget <= budget as usize
    && is_valid_split(&node_path)
    && (!req.enable_visibility_cull || node_visible(&node_path))   // NEW
{ ... accept split, push children ... }
```

where `node_visible(path)` calls `OccluderWorld::is_node_visible(node.bbox, path,
&req.cameras)` with the request-level memo cache. Because an occluded parent is
never split, its children never enter the heap — the subtree is pruned exactly as
required ("simply avoid splitting their parent at all").

Important ordering detail: the heap already pops coarse (closest-big) nodes first,
so we naturally test high in the tree and prune early. Keep the visibility test
**after** the cheap `is_valid_split` distance test (only pay for rays on nodes that
distance *wants* to split).

### 6.3 Scoring interaction

Leave `tile_score` distance-based (visibility is a hard gate, not a soft score).
Optionally, later, fold a visibility margin into the score to prefer barely-visible
tiles last. Out of scope for v1.

---

## 7. Client integration — per-controller camera radius

All three controllers already push a reference point (currently only the freecam
camera translation is pushed in `lod_flow.rs:51`; pedestrian/car set
`MapLODState.reference_points`). We change the client to publish
`Vec<CameraReference>` with the **correct radius per controller**. The guiding
principle from the request: *"even if we wildly rotate the camera or shift-move, we
only divide the splits we can still see"* — so `r` must cover the worst-case camera
displacement over the LOD update interval, not just the current camera position.

Add a resource-agnostic producer: each controller writes into
`MapLODState.cameras: Vec<CameraReference>` (new field) each frame; `lod_flow.rs`
sends that instead of the ad-hoc `reference_points`.

### 7.1 Free-roam camera (`game_freecam/camera_controls.rs`)

Speed is `speed = clamp(height, 5.0, 500.0) * (shift ? 5.0 : 1.0)`
(lines 140–141). Requirement: radius = **2× max sprint speed at that altitude**.

```rust
let sprint_speed = height.clamp(5.0, 500.0) * 5.0; // worst-case speed this frame
let r = 2.0 * sprint_speed;                          // meters of reach
// center = camera translation (the freecam has no separate anchor)
cameras.push(CameraReference { center: cam.translation, r });
```

Rationale: over an LOD refresh the freecam can sprint `~sprint_speed` m/s; `2×`
gives comfortable headroom so tiles that come into view during the move aren't
mid-flight culled. At altitude 500, `r = 5000` m — essentially "cull only stuff
behind terrain/horizon", which is exactly what we want up high.

### 7.2 Pedestrian controller (`pedestrians/.../camera.rs`)

Camera orbits the character at `rig.current_distance` (lerps `CAM_DISTANCE` ↔
`CAM_AIM_DISTANCE`). The anchor is the character; camera is `current_distance` away
and can orbit anywhere on that sphere.

Requirement: radius = **camera-distance-to-object × 2**.

```rust
let center = character_world_pos;              // the anchor / look target
let r = rig.current_distance * 2.0;            // orbit reach, doubled for rotation headroom
cameras.push(CameraReference { center, r });
```

The `×2` guarantees that if the player whips the orbit around the character (camera
can be on the opposite side, distance `current_distance` away), the whole orbit
sphere is inside `B(center, r)`, so nothing the swung camera could see gets culled.

### 7.3 Car controller (`cars_driving/.../camera_follow.rs`)

Fixed orbit radius `r = 16.0` (line 82), anchored at the car `center`. Same rule:

```rust
let center = car_center;
let r = 16.0 * 2.0; // = 32.0, camera orbit radius doubled
cameras.push(CameraReference { center, r });
```

(If the car later gets variable zoom, read the live orbit radius instead of the
constant.)

### 7.4 Multiple simultaneous references

`reference_points` already supports several manual points (map UI) plus the active
camera. Visibility keep is an **OR across all cameras**: a tile survives if visible
from *any* reference ball. This is automatic since `is_node_visible` early-outs on
the first clear ray across all `cameras`.

### 7.5 Dedup / throttle unchanged

`lod_flow.rs` already dedupes requests against `last` and only re-sends when nodes
/ refs / budget change. Extend the `last` tuple to include the quantized `cameras`
so tiny camera jitter doesn't spam RPCs (quantize center to ~1 m, radius to ~2 m).

---

## 8. Complexity & runtime estimate (~400k tiles)

Let:
- `N_occ` = occluder tiles in BVH (coarse shell + spawned) ≈ **2k–20k tiles**,
  ~10–100 tris each ⇒ ~0.2–2 M triangles. BVH build: `O(N_occ log N_occ)` on
  *tiles* (AABBs), one-time, ~tens of ms; per-tile TriMesh build amortized by cache.
- `N_test` = nodes the heap actually *considers splitting* per request. This is
  **not** 400k. The budget caps spawned nodes (`lod_budget`, a few hundred to low
  thousands), and the heap only pops nodes reachable from roots under budget. In
  practice `N_test` ≈ **hundreds to low thousands** per request.
- Rays per node = `K·|Q|` ≈ `16 · 9 = 144`, early-out ⇒ effective ~1–20 for visible
  nodes (common), full 144 only for genuinely occluded nodes.
- Ray-vs-QBVH ≈ `O(log N_occ)` ≈ 14–20 leaf visits, each maybe 1 exact tri test.

Per request (worst case, all considered nodes occluded, no cache):
`N_test · K·|Q| · log N_occ` ≈ `1000 · 144 · 18` ≈ **2.6 M ray-leaf ops**. At
~10–50 M simple ray-AABB/ray-tri ops per second per core in Rust, that's
**~50–260 ms** worst case single-threaded — too slow if it happened every frame,
but:

- **Early-out** collapses the common (visible) case by ~10–50×.
- **Subtree pruning**: one occluded coarse node removes its *whole* subtree from
  `N_test`, so occluded regions are cheap, not expensive.
- **Memoization + cross-request quantized cache**: steady camera ⇒ near-zero recompute.
- **Parallelism**: node visibility tests are independent ⇒ `rayon`/task pool across
  cores (worker already async). 4–8× headroom.

Realistic steady-state target: **< 5–10 ms** added to `compute_lod_changes`
(which already logs when it exceeds 12 ms). The one-time occluder-BVH build (tens of
ms) happens once after manifest load, off the hot path. The dominant *new* cost is
**fetching occluder GLBs**, fully hidden behind the LRU cache and the fact that the
coarse occluder set is small and static.

The naive alternative (test all 400k leaves against the full 400k-tri mesh every
frame) would be `~10^11` ops — infeasible; the coarse-occluder + subtree-prune +
early-out design is what makes it tractable.

---

## 9. Implementation steps (ordered, each independently testable)

1. **API surface (dark, no behavior change).** Add `CameraReference`,
   `cameras`, `enable_visibility_cull` to `LodComputeRequest`; wire client to
   populate `cameras` (with per-controller radii, §7) but keep
   `enable_visibility_cull=false`. Ship — verifies serialization, no regression.
2. **`visibility.rs` skeleton + parry3d dep.** `OccluderWorld`, glam⇄nalgebra
   helpers, `build_occluder_world`, `segment_hits_other`, unit tests with a hand-made
   box occluder (ray blocked / clear).
3. **Occluder ingestion.** Fetch coarse occluder GLBs via existing http + 
   `extract_collider_data`; build & cache `OccluderWorld` (worker-global `OnceCell`).
   Log tri count & build time.
4. **`is_node_visible` + ray bundle** (Fibonacci sphere sampler, corner/edge target
   generator, early-out). Unit-test: tile behind a big box from a ball on the far
   side ⇒ occluded; move ball to same side ⇒ visible.
5. **Gate the split** in `compute_lod_changes` (§6.2), behind
   `enable_visibility_cull`. Add memo cache + timing logs.
6. **Enable on client** (flip flag) for freecam first; validate visually (no popping
   of visible tiles; occluded-behind-hill tiles no longer split). Then pedestrian &
   car.
7. **Cross-request quantized cache + parallelism** (`rayon`) once correctness is
   confirmed.
8. **Hysteresis** (N-frame confirm before cull) to kill any residual flicker.

## 10. Risks & mitigations

- **False culls (popping):** conservative bias (§2.2/§3.4) + hysteresis + kill
  switch (`enable_visibility_cull`). Start with generous `K`, shrink later.
- **Occluder mesh coordinate mismatch:** validate raw-vertex world-space assumption
  early with a known ray (step 2/4).
- **WASM build of parry3d:** parry is `no_std`-friendly and used by avian on wasm;
  confirm the worker (`web_worker`) builds. Keep it behind `worker` feature so the
  bevy client (which already links avian/parry) is unaffected.
- **Memory of many TriMeshes:** bound occluder set by depth; drop TriMesh vertices
  after BVH build if only AABB-level tests are enabled in a "fast" mode.
- **Cost spikes on camera teleport (respawn, enter/exit car):** cache miss storm.
  Mitigate by keeping the BVH static (occluders don't depend on camera) — only the
  cheap ray tests re-run, and they're parallel + early-out.

## 11. Out of scope for v1 (future)

- Full from-region PVS / umbra fusion (§3.2).
- Software-rasterized HZB occlusion (§3.3).
- Folding a visibility margin into the LOD *score* (§6.3).
- Using fine (leaf) meshes as occluders / self-occlusion-based culling.
