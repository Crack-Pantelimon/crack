

## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## src

### src/api.rs
```
pub struct FetchArgs
```

### src/geo.rs
```
pub struct GeoBBox
pub struct ProjectionRef
impl GeoBBox
  pub fn contains(&self, lat: f64, lon: f64) → bool
pub fn octant_path_to_geobbox(path: &str) → Option<GeoBBox>
pub fn find_tile_for_lat_lon(lat: f64, lon: f64, map_tree: &'a MapTreeData,) → Option<&'a MapTreeNodeInfo>
pub fn get_enu_rotation_matrix(ref_point: Vec3) → [Vec3
pub fn lat_lon_to_ecef(lat_deg: f32, lon_deg: f32) → Vec3
pub fn lat_lon_to_bevy(lat_deg: f32, lon_deg: f32, ref_point: Vec3, rot_matrix: &[Vec3; 3],) → Vec3
pub fn parse_geo_bbox_from_txt(text: &str) → Option<GeoBBox>
pub fn parse_bbox_from_txt(text: &str) → Option<(f32, f32)>
pub fn apply_geo_extent_bbox(tree: &mut MapTreeData, geo_bbox: &GeoBBox)
pub fn project_point(lat: f64, lon: f64, map_tree: &MapTreeData, coord_res: &ProjectionRef,) → Vec3
```

### src/glb.rs
```
pub struct FetchGlbRequest
pub struct FetchGlbResponse
```

### src/lod.rs
```
pub struct Score
pub struct CameraReference
pub struct LodComputeRequest
pub struct SplitRequestSummary
pub struct MergeRequestSummary
pub struct CulledNodeSummary
pub struct LodComputeResponse
impl Score
impl Score
impl Score
pub fn compute_distance_to_aabb(bbox: &BBox, p: Vec3) → f32
pub async fn compute_lod_changes(data_res: &MapTreeData, req: &LodComputeRequest,) → LodComputeResponse
```

### src/map.rs
```
pub struct BBox
pub struct MapTileAssetId
pub struct MapTreeNodePath
pub struct MapTreeAssetInfo
pub struct MapTreeNodeInfo
pub struct MapTreeData
pub struct FakeMapTile
pub struct MapTileAssetInfoSummary
pub struct MapRootNodeSummary
pub struct MapManifestResult
impl MapTileAssetId
  pub fn get_octant_path(&self) → MapTreeNodePath
impl MapTreeNodePath
  pub fn get_parent(&self) → Option<MapTreeNodePath>
```

### src/network.rs
```
pub struct GameplaySyncRoomType
pub struct GameplayPresence
pub enum GameplayChatMessageContent
impl GameplaySyncRoomType
pub fn network_manager_config() → NetworkManagerConfig
pub fn bootstrap_topics() → Vec<String>
```

### src/osm.rs
```
pub struct RawGeoJsonFeature
pub struct GeoJsonFeature
pub struct OsmDataResult
pub enum RawFeatureGeometry
pub enum FeatureGeometry
```

### src/pedestrian.rs
```
pub struct AnimationMeta
pub struct PedestrianManifestResult
```

### src/tile.rs
```
pub struct MeshColliderData
pub struct FetchTileRequest
pub struct FetchTileResponse
```

### src/visibility.rs
```
pub struct OccluderWorld
impl OccluderWorld
  pub fn new_empty() → Self
  pub fn insert_occluder(&mut self, path: &MapTreeNodePath, bbox: &BBox, trimesh: Arc<TriMesh>)
  pub fn remove_node(&mut self, path: &MapTreeNodePath)
  pub fn retain_paths(&mut self, keep: &BTreeSet<MapTreeNodePath>)
  pub fn is_ray_occluded(&self, origin: Vector, target: Vector, exclude_path: &MapTreeNodePath, exclude_bbox: &BBox,) → bool
  pub fn is_node_visible(&self, node_bbox: &BBox, node_path: &MapTreeNodePath, cameras: &[CameraReference],) → bool
pub fn verdict_should_refresh(path_key: &str, now_ms: i64) → bool
pub fn build_trimesh_from_mesh(vertices: &[[f32; 3]], indices: &[[u32; 3]]) → Option<TriMesh>
pub async fn get_or_build_trimesh(path: &MapTreeNodePath, assets: &[(String, String) → Option<Arc<TriMesh>>
pub async fn get_cached_trimesh(path: &MapTreeNodePath) → Option<Arc<TriMesh>>
```

### src/weapon.rs
```
pub struct WeaponEntry
pub struct WeaponManifestResult
```

### src/worker/http.rs
```
pub async fn http_get_bytes(url: &str) → anyhow::Result<bytes::Bytes>
pub async fn http_get_text(url: &str) → anyhow::Result<String>
pub async fn http_get_bytes(url: &str) → anyhow::Result<bytes::Bytes>
pub async fn http_get_text(url: &str) → anyhow::Result<String>
```

### src/worker/lru.rs
```
pub struct LruCache
impl LruCache
  pub fn new(max_entries: usize) → Self
  pub fn get(&mut self, key: &str) → Option<T>
  pub fn insert(&mut self, key: String, val: T)
```

### src/worker/manifest_impl.rs
```
pub async fn get_manifest_cache() → anyhow::Result<Arc<MapTreeD...
pub async fn fetch_map_manifest(args: FetchArgs) → anyhow::Result<MapManifestR...
pub async fn fetch_fake_map_tiles(_args: FetchArgs) → anyhow::Result<Vec<FakeMapT...
```

### src/worker/models.rs
```
pub async fn run_game_migrations(_: () → anyhow::Result<()>
```

### src/worker/osm_impl.rs
```
pub async fn fetch_osm_data(args: FetchArgs) → anyhow::Result<OsmDataResult>
```

### src/worker/pedestrian_impl.rs
```
pub async fn fetch_pedestrian_manifest(args: FetchArgs,) → anyhow::Result<PedestrianMa...
pub async fn fetch_pedestrian_model(req: FetchGlbRequest) → anyhow::Result<FetchGlbResp...
```

### src/worker/tile_impl.rs
```
pub async fn fetch_map_tile(req: FetchTileRequest) → anyhow::Result<FetchTileRes...
pub async fn get_tile_collider(tile_id: &str) → Option<crate::tile::MeshCol...
```

### src/worker/weapon_impl.rs
```
pub async fn fetch_weapon_manifest(args: FetchArgs) → anyhow::Result<WeaponManife...
pub async fn fetch_weapon_model(req: FetchGlbRequest) → anyhow::Result<FetchGlbResp...
```
