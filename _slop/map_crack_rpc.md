# Plan: Move Map Tile Fetching + Collider Computation to game_logic Worker

## Context & Goal

Currently, the Bevy client fetches map tile `.glb` files directly through its `AssetServer` (HTTP URL → Bevy's GLTF loader pipeline), then relies on `ColliderConstructorHierarchy::new(TrimeshFromMesh)` to generate trimesh colliders from the loaded meshes on the main thread. This is expensive:

1. **Each tile fetch** goes through Bevy's asset pipeline which must download the GLB, parse it, load textures, instantiate the scene, and only then does Avian's `ColliderConstructorHierarchy` scan all child meshes to build the trimesh collider on the main thread.
2. **No caching** — the same tile fetched due to LOD split/merge cycles re-downloads and re-parses everything.
3. **Main thread stall** — GLB parsing and collider construction happen on the Bevy main thread (or its limited task pools), competing with rendering.

**Goal**: Move tile fetching and heavy processing into the game_logic worker (web worker on WASM, in-process thread pool on native). The worker will:
- Fetch the raw `.glb` bytes via async HTTP
- Parse the GLB and extract mesh vertex/index data
- Compute the Avian trimesh collider data
- Cache results in a 1000-entry LRU
- Return serialized mesh + collider data to the Bevy client so it can create entities on the main thread cheaply

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│  Bevy Main Thread (client)                              │
│                                                         │
│  LOD system decides tile needed ─────────┐              │
│                                          │              │
│  spawn async task: client.call::<        │              │
│    FetchMapTile>(tile_id, base_url) ─────┼──── RPC ───> │
│                                          │              │
│  poll task, on Ok:                       │              │
│    • Insert raw GLB bytes into           │              │
│      Bevy AssetServer (from_bytes)       │              │
│    • Insert pre-computed Collider        │              │
│      directly (skip Hierarchy scan)      │              │
│    • Spawn entity with mesh + collider   │              │
└──────────────────────────────────────────┘              │
                                                          │
┌─────────────────────────────────────────────────────────┐
│  game_logic Worker                                      │
│                                                         │
│  FetchMapTile handler:                                  │
│    1. Check LRU cache (1000 entries)                    │
│    2. If miss: HTTP GET the .glb bytes                  │
│    3. Parse GLB with a lightweight parser               │
│       to extract mesh vertices + indices                │
│    4. Compute trimesh collider vertices/indices          │
│    5. Store in LRU cache                                │
│    6. Return TileResponse { glb_bytes, collider_data }  │
└─────────────────────────────────────────────────────────┘
```

---

## Critical Design Decisions & Challenges

### Challenge 1: GLB Parsing in the Worker (No Bevy)

The worker crate **cannot depend on Bevy** (it must compile for wasm32-unknown-unknown as a standalone web worker). Bevy's GLTF loader is deeply integrated with the Bevy asset pipeline and renderer.

**Approach**: Use the `gltf` crate (the same one Bevy uses internally) to parse the GLB binary and extract raw mesh data (positions, indices). This crate is pure Rust, works on WASM, and is lightweight.

The worker will:
1. Parse the GLB binary with the `gltf` crate
2. Walk all meshes/primitives and extract `POSITION` attributes (f32x3) and index buffers
3. Collect all (vertices, indices) pairs → this is what Avian needs for `Collider::trimesh()`

### Challenge 2: Avian Collider in the Worker

Avian's `Collider::trimesh(vertices, indices)` is a standalone function that produces a `Collider` from vertex/index data. However, `Collider` contains Parry shape data which is **not trivially serializable with postcard/serde** across the worker boundary.

**Approach**: Instead of serializing the full `Collider` object, serialize the **pre-extracted mesh data** (vertices as `Vec<[f32; 3]>`, indices as `Vec<[u32; 3]>`) that the client needs to construct the collider. This avoids pulling Avian as a dependency into the worker entirely.

The actual plan:
- Worker extracts `(Vec<[f32; 3]>, Vec<[u32; 3]>)` per mesh primitive from the GLB
- This gets serialized via postcard as part of the RPC response
- Client receives this and calls `Collider::trimesh(vertices, indices)` — which is fast compared to the `ColliderConstructorHierarchy` approach that must traverse the entire scene hierarchy

> **Why not avian3d in the worker?** Adding avian3d would pull in bevy as a transitive dependency, which won't compile for the worker target. The collider constructor is really just Parry's trimesh builder, which operates on vertices+indices. We pass those directly.

### Challenge 3: GLB Loading on the Client Side

Currently tiles use `asset_server.load(GltfAssetLabel::Scene(0).from_asset(glb_url))` which returns a `Handle<WorldAsset>`. With worker-fetched bytes, we need an alternative loading path.

**Approach**: Use Bevy's `AssetServer::load_with_reader` or register the GLB bytes as a custom asset source. In Bevy 0.19, the most practical approach is:

1. Worker sends the raw GLB bytes back to the client
2. Client uses `asset_server.add(WorldAsset::from_bytes(...))` or a similar in-memory loading path
3. Alternatively, use a custom `AssetReader` that serves from an in-memory cache

**Recommended path**: The simplest approach that preserves the existing material fix-up pipeline:
- The worker returns `glb_bytes: Vec<u8>` (the raw GLB file)
- The client creates a `Handle<WorldAsset>` from bytes using Bevy's `Assets::add()` or a custom asset source backed by an in-memory `HashMap`
- This means the Bevy GLTF loader still runs on the main thread, BUT the network latency is eliminated (bytes are already in memory) and the collider is pre-extracted

**For Phase 2 (future optimization)**: Pre-process the GLTF into Bevy's native scene format in the worker to eliminate main-thread GLTF parsing entirely.

### Challenge 4: Postcard Serialization of Large Binary Payloads

The RPC system uses postcard serialization. GLB files can be 100KB-2MB each. Postcard handles `Vec<u8>` efficiently (length-prefixed byte blob), so this is fine.

---

## Detailed Implementation Plan

### Phase 1: Worker-Side Tile Fetcher with LRU Cache

#### 1.1 New Dependencies in `game_logic/Cargo.toml`

Add under the `worker` feature:
```toml
gltf = { version = "1", default-features = false }  # GLB parsing (no image loading)
```

No avian3d dependency needed — we only extract raw vertex/index data.

#### 1.2 New Module: `game_logic/src/tile.rs` (always compiled)

Define the RPC request/response types:

```rust
use serde::{Deserialize, Serialize};

/// A single mesh primitive's geometry, ready for Collider::trimesh()
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshColliderData {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<[u32; 3]>,
}

/// Request to fetch and process a single map tile
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchTileRequest {
    pub base_url: String,
    /// The glb_path relative to the data directory (e.g. "data_out/tiles/0/0_0.glb")
    pub glb_path: String,
    /// Unique identifier for this tile (used as cache key) — same as the glb_path
    pub tile_id: String,
}

/// Response containing the raw GLB bytes and pre-extracted collider geometry
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchTileResponse {
    pub tile_id: String,
    /// Raw GLB file bytes — the client feeds these to Bevy's asset loader
    pub glb_bytes: Vec<u8>,
    /// Pre-extracted mesh geometry for each primitive in the GLB.
    /// The client calls Collider::trimesh() on each.
    pub collider_meshes: Vec<MeshColliderData>,
    /// Whether this was served from the LRU cache
    pub from_cache: bool,
}
```

#### 1.3 New Module: `game_logic/src/worker/tile_impl.rs` (worker feature only)

```rust
use crate::tile::*;
use std::collections::HashMap;

/// LRU cache entry
struct CachedTile {
    response: FetchTileResponse,
    last_access_ms: i64,
}

/// In-memory LRU cache for processed tiles
static TILE_CACHE: tokio::sync::RwLock<Option<TileCache>> = 
    tokio::sync::RwLock::const_new(None);

struct TileCache {
    entries: HashMap<String, CachedTile>,
    max_entries: usize,
}

impl TileCache {
    fn new(max_entries: usize) -> Self { ... }
    
    fn get(&mut self, key: &str) -> Option<&FetchTileResponse> {
        // Update last_access_ms, return cached response
        let now = _crack_utils::get_timestamp_now_ms();
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_access_ms = now;
            Some(&entry.response)
        } else {
            None
        }
    }
    
    fn insert(&mut self, key: String, response: FetchTileResponse) {
        let now = _crack_utils::get_timestamp_now_ms();
        // Evict oldest if at capacity
        if self.entries.len() >= self.max_entries {
            if let Some(oldest_key) = self.entries.iter()
                .min_by_key(|(_, v)| v.last_access_ms)
                .map(|(k, _)| k.clone()) 
            {
                self.entries.remove(&oldest_key);
            }
        }
        self.entries.insert(key, CachedTile { 
            response, 
            last_access_ms: now 
        });
    }
}
```

GLB parsing logic:
```rust
fn extract_collider_data(glb_bytes: &[u8]) -> anyhow::Result<Vec<MeshColliderData>> {
    let gltf = gltf::Gltf::from_slice(glb_bytes)?;
    let blob = gltf.blob.as_ref()
        .ok_or_else(|| anyhow::anyhow!("GLB has no binary blob"))?;
    
    let mut meshes = Vec::new();
    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| {
                match buffer.source() {
                    gltf::buffer::Source::Bin => Some(&blob[..]),
                    _ => None,
                }
            });
            
            let positions: Vec<[f32; 3]> = reader.read_positions()
                .map(|iter| iter.collect())
                .unwrap_or_default();
            
            let indices: Vec<u32> = reader.read_indices()
                .map(|iter| iter.into_u32().collect())
                .unwrap_or_default();
            
            if positions.is_empty() || indices.is_empty() {
                continue;
            }
            
            // Convert flat index list to triangle triples
            let triangles: Vec<[u32; 3]> = indices.chunks(3)
                .filter_map(|chunk| {
                    if chunk.len() == 3 {
                        Some([chunk[0], chunk[1], chunk[2]])
                    } else {
                        None
                    }
                })
                .collect();
            
            meshes.push(MeshColliderData {
                vertices: positions,
                indices: triangles,
            });
        }
    }
    Ok(meshes)
}
```

Main handler:
```rust
pub async fn fetch_map_tile(req: FetchTileRequest) -> anyhow::Result<FetchTileResponse> {
    let t0 = _crack_utils::get_timestamp_now_ms();
    
    // Check cache first
    {
        let mut guard = TILE_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| TileCache::new(1000));
        if let Some(cached) = cache.get(&req.tile_id) {
            let mut resp = cached.clone();
            resp.from_cache = true;
            let t1 = _crack_utils::get_timestamp_now_ms();
            tracing::debug!("Tile cache HIT: {} ({}ms)", req.tile_id, t1 - t0);
            return Ok(resp);
        }
    }
    
    // Cache miss: fetch the GLB
    let url = format!("{}/3d_data_v2/{}", req.base_url, req.glb_path);
    let glb_bytes = super::http::http_get_bytes(&url).await?;
    let t_fetch = _crack_utils::get_timestamp_now_ms();
    
    // Extract collider geometry
    let collider_meshes = extract_collider_data(&glb_bytes)?;
    let t_parse = _crack_utils::get_timestamp_now_ms();
    
    let response = FetchTileResponse {
        tile_id: req.tile_id.clone(),
        glb_bytes: glb_bytes.to_vec(),
        collider_meshes,
        from_cache: false,
    };
    
    // Store in cache
    {
        let mut guard = TILE_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| TileCache::new(1000));
        cache.insert(req.tile_id.clone(), response.clone());
    }
    
    let t1 = _crack_utils::get_timestamp_now_ms();
    tracing::info!(
        "Tile fetched: {} (fetch={}ms parse={}ms total={}ms bytes={})",
        req.tile_id, t_fetch - t0, t_parse - t_fetch, t1 - t0, 
        response.glb_bytes.len()
    );
    
    Ok(response)
}
```

#### 1.4 Register the New API Method

**`game_logic/src/api.rs`** — add `FetchMapTile`:
```rust
declare_api_group2! { GameLogicApiGroup, [
    (FetchMapManifest, FetchArgs, crate::map::MapManifestResult),
    (FetchOsmData, FetchArgs, crate::osm::OsmDataResult),
    (ComputeLodChanges, crate::lod::LodComputeRequest, crate::lod::LodComputeResponse),
    (RunGameMigrations, (), ()),
    (FetchMapTile, crate::tile::FetchTileRequest, crate::tile::FetchTileResponse),  // NEW
] }
```

**`game_logic/src/worker/mod.rs`** — register impl:
```rust
implement_api_group2! { GameLogicApiGroup, [
    (FetchMapManifest, manifest_impl::fetch_map_manifest),
    (FetchOsmData, osm_impl::fetch_osm_data),
    (ComputeLodChanges, compute_lod_changes_api),
    (RunGameMigrations, models::run_game_migrations),
    (FetchMapTile, tile_impl::fetch_map_tile),  // NEW
] }
```

**`game_logic/src/lib.rs`** — add `pub mod tile;`

---

### Phase 2: Client-Side Changes (Bevy)

#### 2.1 New Module: `crack_plugin/tile_flow.rs`

This module manages async tile fetch tasks and processes responses.

**Core idea**: Instead of `asset_server.load(glb_url)` → `Handle<WorldAsset>`, the client will:
1. Fire off an RPC call `FetchMapTile` to the worker
2. Receive `FetchTileResponse` containing raw GLB bytes + collider data
3. Register the GLB bytes with a custom in-memory `AssetSource` so Bevy's GLTF loader can load from memory (not HTTP)
4. Create the collider directly from the pre-extracted vertex/index data using `Collider::trimesh()`

**Custom Asset Source** (registered in Bevy's asset system):

```rust
/// An in-memory asset source that serves tile GLB bytes fetched by the worker.
/// Tiles are registered with a unique key (the tile_id) and Bevy loads them
/// from this source using the custom scheme "tile://".
#[derive(Default)]
pub struct TileAssetStore {
    tiles: HashMap<String, Vec<u8>>,
}

impl TileAssetStore {
    pub fn insert(&mut self, tile_id: String, bytes: Vec<u8>) { ... }
}
```

Alternatively (simpler, recommended for Phase 1):
- Use `asset_server.load_with_settings` from bytes, or
- Create assets directly: load the GLB bytes through `Assets<Gltf>::add()` then instantiate the scene manually

**Simplest viable approach**: Use Bevy 0.19's `AssetServer::load_from_bytes` (if available) or write a minimal custom `AssetReader` backed by an `Arc<RwLock<HashMap>>`.

#### 2.2 Modify `map_lod.rs` — Tile Spawning

**Current flow** (`spawn_node_tiles`):
```rust
fn spawn_node_tiles(commands, assets: &[(MapTileAssetId, Handle<WorldAsset>)], ...) -> Vec<Entity>
```

**New flow**: Two-phase spawning:

```rust
/// Phase 1: Create entity with placeholder, kick off worker fetch
fn request_tile_fetch(
    commands: &mut Commands,
    client: &CrackClient,
    asset_id: &MapTileAssetId,
    glb_path: &str,
    node_path: &MapTreeNodePath,
) -> Entity {
    // Spawn an entity with TileFetching marker component
    // Fire off the async task
}

/// Phase 2: When the worker responds, create the mesh + collider
fn finalize_tile(
    commands: &mut Commands,
    entity: Entity,
    response: &FetchTileResponse,
) {
    // 1. Register GLB bytes with Bevy asset system
    // 2. Create collider from pre-extracted data:
    //    for mesh_data in &response.collider_meshes {
    //        let verts: Vec<Vec3> = mesh_data.vertices.iter()
    //            .map(|v| Vec3::new(v[0], v[1], v[2]))
    //            .collect();
    //        let collider = Collider::trimesh(verts, mesh_data.indices.clone());
    //    }
    // 3. Insert WorldAssetRoot + Collider + RigidBody::Static
}
```

#### 2.3 Modify Split/Merge Flow in `map_lod.rs`

**`start_tile_swap_requests`** currently:
```rust
let glb_url = format!("{}/3d_data_v2/{}", DATA_BASE_URL, asset.glb_path);
let asset_path = GltfAssetLabel::Scene(0).from_asset(glb_url);
(asset.name.clone(), asset_server.load(asset_path))
```

**New version**: Instead of calling `asset_server.load()` immediately, it creates `TileFetchTask` entities:

```rust
/// Marker component for a tile that is being fetched by the worker
#[derive(Component)]
pub struct TileFetchTask {
    pub task: Task<anyhow::Result<FetchTileResponse>>,
    pub asset_id: MapTileAssetId,
    pub node_path: MapTreeNodePath,
}
```

For split requests: spawn a `TileFetchTask` entity per tile asset. When all tasks in a split group complete, proceed to spawn the actual tile entities with mesh + collider.

For merge requests: same pattern — fetch the parent's tiles via worker, then swap.

#### 2.4 New System: `poll_tile_fetch_tasks`

```rust
pub fn poll_tile_fetch_tasks(
    mut commands: Commands,
    mut q_tasks: Query<(Entity, &mut TileFetchTask)>,
    // ... asset registration resources
) {
    for (entity, mut task) in q_tasks.iter_mut() {
        if let Some(Ok(response)) = block_on(poll_once(&mut task.task)) {
            // 1. Register GLB bytes with asset system
            // 2. Build collider from pre-extracted mesh data  
            // 3. Replace TileFetchTask component with the final tile components
            // 4. Or: store result and let the split/merge system pick it up
        }
    }
}
```

#### 2.5 Changes to `TileShouldSplit` / `TileShouldMerge`

Instead of storing `Handle<WorldAsset>`, these will store the worker-fetched results:

```rust
#[derive(Component, Debug)]
pub struct TileShouldSplit {
    pub load_children: Vec<(MapTreeNodePath, Vec<TileFetchResult>)>,
    pub drop_parent: MapTreeNodePath,
}

pub struct TileFetchResult {
    pub asset_id: MapTileAssetId,
    pub glb_handle: Handle<WorldAsset>,  // created from worker bytes
    pub collider: Collider,              // pre-built from worker data
}
```

#### 2.6 Changes to `spawn_node_tiles`

The spawned entity will use the pre-built collider instead of `ColliderConstructorHierarchy`:

```rust
fn spawn_node_tiles(
    commands: &mut Commands,
    assets: &[(MapTileAssetId, Handle<WorldAsset>, Collider)],  // NEW: Collider included
    node_path: &MapTreeNodePath,
    hidden: bool,
) -> Vec<Entity> {
    for (asset_id, handle, collider) in assets {
        commands.spawn((
            WorldAssetRoot(handle.clone()),
            visibility,
            Transform::from_xyz(0.0, 0.0, 0.0),
            TreeMapTile { node_path, asset_id },
            RigidBody::Static,
            collider.clone(),           // Direct collider, no hierarchy scan!
            CollisionMargin(0.2),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
            Friction::new(0.9),
            CollisionLayers::new(
                [GamePhysicsLayer::Map],
                [GamePhysicsLayer::Car, GamePhysicsLayer::Wheel],
            ),
        ))
    }
}
```

Note: `ColliderConstructorHierarchy` is **removed** — the collider is directly inserted.

#### 2.7 `CrackTasks` Extension

Add tile-fetch tracking:
```rust
#[derive(Resource, Default)]
pub struct CrackTasks {
    pub manifest: Option<Task<...>>,
    pub osm: Option<Task<...>>,
    pub lod: Option<Task<...>>,
    // No per-tile tasks here — those live as components on TileFetchTask entities
}
```

---

### Phase 3: Root Tile Bootstrap

`spawn_root_map_tiles` currently fires on `MapTree` change. With the new system:

```rust
pub fn spawn_root_map_tiles(
    mut commands: Commands,
    data_res: Res<MapTree>,
    client: Res<CrackClient>,
    // NO asset_server needed
) {
    if !data_res.is_changed() || !data_res.parsed { return; }
    
    for root in &data_res.roots {
        for asset in &root.assets {
            let api_client = client.0.clone();
            let base_url = DATA_BASE_URL.to_string();
            let glb_path = asset.glb_path.clone();
            let tile_id = asset.name.0.clone();
            let node_path = root.path.clone();
            let asset_id = asset.name.clone();
            
            let task = AsyncComputeTaskPool::get().spawn(async move {
                api_client.call::<FetchMapTile>(FetchTileRequest {
                    base_url,
                    glb_path,
                    tile_id,
                }).await
            });
            
            commands.spawn(TileFetchTask {
                task,
                asset_id,
                node_path,
                purpose: TileFetchPurpose::RootTile,
            });
        }
    }
}
```

---

## Files Modified Summary

### game_logic crate
| Action | File | Description |
|--------|------|-------------|
| NEW | `src/tile.rs` | `FetchTileRequest`, `FetchTileResponse`, `MeshColliderData` types |
| MODIFY | `src/lib.rs` | Add `pub mod tile;` |
| MODIFY | `src/api.rs` | Add `FetchMapTile` API method |
| NEW | `src/worker/tile_impl.rs` | GLB fetch, parse, LRU cache, handler |
| MODIFY | `src/worker/mod.rs` | Register `FetchMapTile` impl, add `pub mod tile_impl;` |
| MODIFY | `Cargo.toml` | Add `gltf` to worker feature deps |

### Bevy client crate
| Action | File | Description |
|--------|------|-------------|
| NEW | `src/plugins/crack_plugin/tile_flow.rs` | In-memory asset source, tile fetch polling |
| MODIFY | `src/plugins/crack_plugin/mod.rs` | Register tile_flow systems |
| MODIFY | `src/plugins/map_plugin/map_lod.rs` | Replace `asset_server.load()` with worker RPC, replace `ColliderConstructorHierarchy` with direct `Collider`, new `TileFetchTask` component, rewrite split/merge/root flows |
| MODIFY | `src/plugins/map_plugin/mod.rs` | Register new tile polling system |

### Worker registration (unchanged structure)
Both `web_worker` and `thread_worker` already register `GameLogicApiGroup` — the new `FetchMapTile` method is automatically included.

---

## LRU Cache Design Details

- **Size**: 1000 entries (configurable constant)
- **Key**: `tile_id` string (= the glb_path)
- **Value**: Full `FetchTileResponse` (GLB bytes + collider data)
- **Eviction**: On insert when at capacity, evict the entry with the smallest `last_access_ms`
- **Timestamp**: `_crack_utils::get_timestamp_now_ms()` (works on both WASM and native)
- **Thread safety**: `tokio::sync::RwLock` (matching existing patterns in `manifest_impl.rs`)
- **Memory estimate**: At ~500KB average per GLB × 1000 entries ≈ 500MB max. In practice, active LOD keeps ~300-400 tiles, so cache rarely fills completely.

---

## Risks & Mitigations

### Risk 1: Large postcard payloads over worker boundary
- **Impact**: Serializing 500KB+ GLB bytes through postcard on each tile fetch
- **Mitigation**: Postcard encodes `Vec<u8>` as length-prefixed bytes — no base64 inflation. On the web worker boundary, `serde_wasm_bindgen` will transfer this as a `Uint8Array`. The worker RPC already handles ~1MB OSM payloads fine.
- **Measurement**: Log serialization time (already done by the API macro) and monitor for >50ms hitches

### Risk 2: `gltf` crate on WASM
- **Impact**: The `gltf` crate needs to parse GLB in the web worker
- **Mitigation**: The `gltf` crate is pure Rust with no native dependencies when default features are off. It's the same parser Bevy uses internally. Has been proven in WASM contexts.

### Risk 3: Collider accuracy
- **Impact**: Pre-extracted trimesh data might differ from what `ColliderConstructorHierarchy` produces
- **Mitigation**: `ColliderConstructorHierarchy::new(TrimeshFromMesh)` essentially does exactly what we do: walk all mesh children, extract positions + indices, call `Collider::trimesh()`. We replicate this logic from the raw GLB data. The vertex positions are identical; the only difference is we skip Bevy's intermediate mesh representation (which doesn't alter geometry).

### Risk 4: In-memory asset loading path in Bevy 0.19
- **Impact**: Need to load GLB from `Vec<u8>` instead of a URL
- **Mitigation**: Investigate Bevy 0.19's `AssetServer` API for `load_from_bytes` or custom `AssetSource`. Fallback: write a custom `AssetReader` backed by `Arc<RwLock<HashMap<String, Vec<u8>>>>` that the tile flow inserts bytes into, then load via a custom `tile://` scheme.
- _slop/memory_asset_reader.md -- see how to insert the bytes into the reader

### Risk 5: Material fix-up pipeline
- **Impact**: The `auto_apply_new_materials` system in `map_material_edit.rs` reacts to `AssetEvent<StandardMaterial>::Added` events. If we change how tiles are loaded, this must still fire.
- **Mitigation**: As long as we load GLBs through Bevy's asset system (even from in-memory bytes), the GLTF loader will create `StandardMaterial` assets and emit `Added` events. No change needed.

### Risk 6: Cache memory pressure on WASM
- **Impact**: 1000 cached tiles × ~500KB = ~500MB, which may exceed WASM memory limits
- **Mitigation**: Monitor actual cache utilization. Can reduce to 200-300 entries for WASM builds via cfg. Alternatively, only cache the collider data (small) and re-fetch GLB bytes on cache hit from browser's HTTP cache.

---

## Verification Plan

### Build verification
1. `cargo check -p game_logic` — no worker feature, API types only
2. `cargo check -p game_logic --features worker` — with gltf dep
3. `cargo check -p game_logic --features worker --target wasm32-unknown-unknown` — WASM worker
4. `cargo check -p web_worker --target wasm32-unknown-unknown` — web worker binary
5. `cargo check -p thread_worker` — native worker
6. `cargo check -p demo_resolution_selector_web_bevy` — Bevy client
7. `cargo check --workspace` — full workspace

### Runtime verification
1. **Native**: `./start_game_native.sh` — verify tiles load, colliders work (cars drive on map, pedestrians walk), LOD splits/merges function
2. **Web**: `./start_game_web.sh` — same checks in browser, verify worker logs show cache hits/misses
3. **Cache verification**: Fly camera in a circle, return to start position — second load should show "cache HIT" logs with <1ms response times
4. **Physics verification**: Drive a car across tile boundaries — no falling through the ground (collider coverage matches visual mesh)

---

## Open Questions

1. **Bevy 0.19 in-memory GLB loading**: What is the exact API for loading a GLB from `Vec<u8>` without going through HTTP? Need to check `AssetServer` docs for Bevy 0.19. If no built-in path exists, a custom `AssetSource` implementation is needed (~50 lines of boilerplate).

2. **Should we cache only collider data vs full GLB?**: Caching full GLB bytes uses more memory but avoids re-downloading. The browser already has an HTTP cache. Consider: worker cache stores only collider data (small), always re-fetches GLB bytes (fast from browser HTTP cache on cache hit). This would dramatically reduce worker memory usage.

3. **Parallel tile fetch limit**: Currently `PARALLEL_SPLIT_FETCH = 3` and `PARALLEL_MERGE_FETCH = 3`. With worker-side fetching, should we increase these since the worker can handle concurrent requests better than the main thread? Or keep the same limits to avoid overwhelming the HTTP connection pool?

4. **Should `gltf` crate use `features = ["KHR_mesh_quantization"]`?**: If the map tiles use quantized mesh attributes, we need this feature. Check the actual GLB files.
