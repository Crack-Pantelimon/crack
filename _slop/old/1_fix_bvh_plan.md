# BVH occluder — fix plan

Two problems reported against the `bvh-occluder` work (commit `2009cb0`):

1. **Browser build broken** — map never loads past the 65 root tiles. No visible
   browser/worker errors. "Some operation is failing on the browser."
2. **Under-culling** — putting the character in a hole culls nothing; culling
   only kicks in when zooming a gun. We want to drop the sphere-sampling model
   and use the reference points *as-is* (camera only for now), with a tiny
   `0.01` radius kept as a hatch.

---

## 0. Architecture recap (why this is subtle)

`compute_lod_changes` (`game_logic/src/lod.rs`) and the whole occluder path
(`game_logic/src/visibility.rs`) are gated behind `#[cfg(feature = "worker")]`.
On the browser this feature **is compiled** and runs inside a dedicated **wasm
web worker** (`packages/web_serviceworker_crackslave/src/lib.rs` →
`_js_compute_payload_reply` → `compute_response_message`).

Consequences that dominate both bugs:

- **Worker faults are near-silent.** A Rust `panic!` in wasm becomes an
  `unreachable` trap that *aborts the whole wasm instance*. The JS glue in
  `_js_compute_payload_reply` only forwards `Result` errors; a trap does not go
  through that path, so the console may show nothing useful (or a cryptic
  `RuntimeError`). After a trap the worker's linear memory is poisoned and **no
  further RPCs succeed** → the map freezes at whatever tiles already loaded (the
  65 roots).
- **The worker is a single cooperative thread.** The CPU-bound parts of
  `rebuild_bvh` (building 64×64 heightfields, hundreds of ray casts) do **not**
  `.await`, so while they run they block *every other* worker RPC, including
  `FetchMapTile`. A heavy occluder pass can starve the fetch pipeline.
- `enable_visibility_cull` is now defaulted **on** at load
  (`crack_plugin/manifest_flow.rs:62`), so this path runs from the first frame.

Ruled out by reading `parry3d-0.27.0` source:
- `Bvh::from_leaves` handles 0/1/2 leaves specially — **empty leaves do not
  panic** (`partitioning/bvh/bvh_tree.rs` `from_iter`).
- `HeightField::with_flags` only asserts `nrows > 1 && ncols > 1`; we always
  build 64×64, so that assert can't fire.
- Collider cache key matches: client fetches with `tile_id = asset.name.0`
  (`map_lod.rs:98`) and `get_tile_collider` looks up by the same string, so
  colliders are found once their tile has been fetched.

So the crash surface is narrower than "any parry call." The remaining realistic
silent-fault candidates are **degenerate geometry feeding parry ray casts**
(zero-scale heightfield → NaN/inf), and **cost/stall** from the sphere sampling.

---

## 1. Bug #2 — drop the sphere model, use points as-is (definitive)

**Root cause of under-culling.** `OccluderWorld::is_node_visible`
(`visibility.rs`) already tests the true camera point first:

```rust
for &q in &corners {
    if !self.is_ray_occluded(camera.center, q, node_path, node_bbox) {
        return true;
    }
}
```

…but then it scatters **16 Fibonacci-sphere samples at radius `drift =
camera.max_range.min(6.0)` = 6 m** around the camera and returns `true` if *any*
of those has a clear line of sight. When the character stands in a hole, the
upper-hemisphere samples poke **above the rim / above the ground**, instantly
re-granting visibility to everything — nothing is ever culled. The large radius
also drops samples **under the terrain**, producing rays that originate inside a
heightfield.

**Fix (implemented).** Collapse the sample sphere to the point itself, keeping a
tiny radius as a documented hatch:

- Replace `const MAX_CAMERA_DRIFT: f32 = 6.0` with
  `const CAMERA_SAMPLE_RADIUS: f32 = 0.01` and use that *fixed* radius
  (decoupled from `camera.max_range`, which is a LOD *reach* distance, not a
  positional uncertainty).
- Add `const MIN_SAMPLING_RADIUS` so that when the radius is tiny (as now), the
  scatter loop is **skipped entirely** — the direct center-point test is
  definitive. This is the point-based model the user asked for, and it also
  removes ~16× of the ray-cast cost per candidate node (helps Bug #1's stall).
- To re-enable a real sphere model later, bump `CAMERA_SAMPLE_RADIUS` above
  `MIN_SAMPLING_RADIUS`; the sampling code is preserved.

Net effect: visibility is now decided purely from the actual camera/reference
positions, so a camera in a hole is genuinely occluded and its far tiles cull.

---

## 2. Bug #1 — browser build stuck at 65 roots

The point-model change above already removes the two most likely wasm fault
triggers on this path (under-terrain sample origins; 16× ray-cast load). The
plan additionally hardens the occluder so a single bad tile can't trap the
worker, and makes the failure **observable** so it can be pinned if it persists.

### 2a. Fixes implemented now

1. **Skip degenerate occluder tiles.** In `rebuild_bvh`, skip any candidate
   whose bbox has (near-)zero horizontal extent before building a heightfield.
   A zero `scale.x`/`scale.z` heightfield makes parry ray casts divide by zero
   → NaN/inf → potential wasm trap. Cheap guard, removes a whole class of
   silent aborts.
2. **Reject non-finite rays.** In `is_ray_occluded`, bail out (return
   not-occluded) if `origin`/`target` are non-finite, so a stray NaN can never
   reach `bvh.traverse` / `cast_local_ray_and_get_normal`.
3. **Observability.** Log leaf/heightfield counts and per-pass timing at
   `info`, and log a one-line summary of how many candidate nodes were tested /
   culled. In the wasm worker these surface via the existing `tracing`
   subscriber and turn "silent" into "diagnosable."

### 2b. How to debug if it still stalls after 2a (recommended order)

1. **Bisect with the toggle.** Uncheck **"BVH occluder (visibility cull)"** in
   the minimap panel (`bvh_minimap.rs:245`). The flag is part of the LOD change
   key (`lod_flow.rs:81`), so unchecking forces an immediate recompute with the
   occluder disabled.
   - Map loads past 65 roots with it **off** ⇒ the fault is inside the occluder
     path (rebuild or `is_node_visible`), confirming this file is the culprit.
   - Still stuck with it **off** ⇒ the regression is *not* the occluder logic;
     look at the LOD request wiring / worker plumbing instead.
2. **Watch the worker console for a trap.** Open the dedicated worker's console
   (Chrome DevTools → Sources → the worker thread, or the "worker" console
   scope). A wasm trap shows as `RuntimeError: unreachable` or a
   `wasm-bindgen` panic string. If present, the next RPC after it will fail with
   a channel-closed error on the client side (`lod_flow.rs:178`
   `LOD RPC error`).
3. **Confirm the RPC is even returning.** Temporarily log at the top and bottom
   of `compute_lod_changes_api` (`worker/mod.rs`) and in `poll_lod_task`. Three
   outcomes:
   - request logged, no response ⇒ trap/stall inside the compute (go to step 4).
   - response logged as `Err` ⇒ read the error string (missing manifest, etc.).
   - response `Ok` but empty `split_requests` ⇒ not a crash; visibility is
     culling everything (Bug #2 territory — verify §1 landed).
4. **Time the occluder build.** The `"Occluder BVH rebuilt in {} ms"` log
   (`lod.rs:134`) already exists. If it never prints, the trap/stall is inside
   `rebuild_bvh`; if it prints huge numbers, it's a cost/starvation problem and
   we should throttle rebuilds (see 2c).

### 2c. Follow-ups (only if 2b shows cost/stall, not a crash)

- `rebuild_bvh` runs on **every** change of the spawned-node set (keyed by
  `hash_spawned_nodes`), rebuilding *all* heightfields each time. If timing is
  the problem, make it incremental (cache is already per-path in
  `HEIGHTMAP_CACHE`; the BVH rebuild is the expensive part) or rate-limit LOD
  recomputes while a rebuild is in flight.
- As a safety valve, keep a debug switch to default `enable_visibility_cull`
  **off** so a future occluder regression can't wedge the browser build again.

---

## 3. Files touched

- `crack_demo/game_logic/src/visibility.rs`
  - `is_node_visible`: point-based sampling (`CAMERA_SAMPLE_RADIUS = 0.01`,
    `MIN_SAMPLING_RADIUS` gate), radius decoupled from `max_range`. (Bug #2)
  - `rebuild_bvh`: skip degenerate-bbox occluders. (Bug #1 hardening)
  - `is_ray_occluded`: reject non-finite ray endpoints. (Bug #1 hardening)
  - added `tracing` diagnostics. (Bug #1 observability)

No behavior outside the occluder path changes. When
`enable_visibility_cull` is off, this code is not exercised.
