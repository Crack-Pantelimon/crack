

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
pub struct FetchArgs  :5-7
```

### src/geo.rs
```
pub struct GeoBBox  :5-10
pub struct ProjectionRef  :178-181
impl GeoBBox  :12-16
  pub fn contains(&self, lat: f64, lon: f64) → bool  :13-13
pub fn octant_path_to_geobbox(path: &str) → Option<GeoBBox>  :18-101
pub fn find_tile_for_lat_lon(lat: f64, lon: f64, map_tree: &'a MapTreeData,) → Option<&'a MapTreeNodeInfo>  :103-175
pub fn get_enu_rotation_matrix(ref_point: Vec3) → [Vec3  :183-183
pub fn lat_lon_to_ecef(lat_deg: f32, lon_deg: f32) → Vec3  :204-214
pub fn lat_lon_to_bevy(lat_deg: f32, lon_deg: f32, ref_point: Vec3, rot_matrix: &[Vec3; 3],) → Vec3  :216-229
pub fn parse_geo_bbox_from_txt(text: &str) → Option<GeoBBox>  :232-260
pub fn parse_bbox_from_txt(text: &str) → Option<(f32, f32)>  :262-268
pub fn apply_geo_extent_bbox(tree: &mut MapTreeData, geo_bbox: &GeoBBox)  :289-324
pub fn project_point(lat: f64, lon: f64, map_tree: &MapTreeData, coord_res: &ProjectionRef,) → Vec3  :326-356
```

### src/glb.rs
```
pub struct FetchGlbRequest  :4-8
pub struct FetchGlbResponse  :11-15
```

### src/lod.rs
```
pub struct Score  :8-8
pub struct CameraReference  :23-27
pub struct LodComputeRequest  :30-44
pub struct SplitRequestSummary  :47-50
pub struct MergeRequestSummary  :53-58
pub struct CulledNodeSummary  :61-64
pub struct LodComputeResponse  :67-71
impl Score  :10-10
impl Score  :11-15
impl Score  :16-20
pub fn compute_distance_to_aabb(bbox: &BBox, p: Vec3) → f32  :85-99
pub async fn compute_lod_changes(data_res: &MapTreeData, req: &LodComputeRequest,) → LodComputeResponse  :142-302
```

### src/map.rs
```
pub struct BBox  :6-9
pub struct MapTileAssetId  :12-12
pub struct MapTreeNodePath  :20-20
pub struct MapTreeAssetInfo  :33-41
pub struct MapTreeNodeInfo  :44-48
pub struct MapTreeData  :51-60
pub struct FakeMapTile  :63-68
pub struct MapTileAssetInfoSummary  :71-75
pub struct MapRootNodeSummary  :78-82
pub struct MapManifestResult  :85-89
impl MapTileAssetId  :13-17
  pub fn get_octant_path(&self) → MapTreeNodePath  :14-14
impl MapTreeNodePath  :21-30
  pub fn get_parent(&self) → Option<MapTreeNodePath>  :22-22
```

### src/network.rs
```
pub struct GameplaySyncRoomType  :30-30
pub struct GameplayPresence  :46-48
pub enum GameplayChatMessageContent  :41-43
impl GameplaySyncRoomType  :32-38
pub fn network_manager_config() → NetworkManagerConfig  :16-20
pub fn bootstrap_topics() → Vec<String>  :24-26
```

### src/osm.rs
```
pub struct RawGeoJsonFeature  :14-20
pub struct GeoJsonFeature  :31-40
pub struct OsmDataResult  :43-45
pub enum RawFeatureGeometry  :6-11
pub enum FeatureGeometry  :23-28
```

### src/pedestrian.rs
```
pub struct AnimationMeta  :4-8
pub struct PedestrianManifestResult  :11-14
```

### src/tile.rs
```
pub struct MeshColliderData  :4-7
pub struct FetchTileRequest  :10-14
pub struct FetchTileResponse  :17-22
```

### src/visibility.rs
```
pub struct OccluderWorld  :158-165
impl OccluderWorld  :167-338
  pub fn new_empty() → Self  :169-169
  pub fn insert_occluder(&mut self, path: &MapTreeNodePath, bbox: &BBox, trimesh: Arc<TriMesh>)  :182-182
  pub fn remove_node(&mut self, path: &MapTreeNodePath)  :196-196
  pub fn retain_paths(&mut self, keep: &BTreeSet<MapTreeNodePath>)  :208-208
  pub fn is_ray_occluded(&self, origin: Vector, target: Vector, exclude_path: &MapTreeNodePath, exclude_bbox: &BBox,) → bool  :221-227
  pub fn is_node_visible(&self, node_bbox: &BBox, node_path: &MapTreeNodePath, cameras: &[CameraReference],) → bool  :286-291
pub fn verdict_should_refresh(path_key: &str, now_ms: i64) → bool  :43-49
pub fn build_trimesh_from_mesh(vertices: &[[f32; 3]], indices: &[[u32; 3]]) → Option<TriMesh>  :54-66
pub async fn get_or_build_trimesh(path: &MapTreeNodePath, assets: &[(String, String) → Option<Arc<TriMesh>>  :72-138
pub async fn get_cached_trimesh(path: &MapTreeNodePath) → Option<Arc<TriMesh>>  :142-146
```

### src/weapon.rs
```
pub struct WeaponEntry  :4-14
pub struct WeaponManifestResult  :17-19
```

### src/worker/http.rs
```
pub async fn http_get_bytes(url: &str) → anyhow::Result<bytes::Bytes>  :18-21
pub async fn http_get_text(url: &str) → anyhow::Result<String>  :24-27
pub async fn http_get_bytes(url: &str) → anyhow::Result<bytes::Bytes>  :30-43
pub async fn http_get_text(url: &str) → anyhow::Result<String>  :46-59
```

### src/worker/lru.rs
```
pub struct LruCache  :10-13
impl LruCache  :15-54
  pub fn new(max_entries: usize) → Self  :16-16
  pub fn get(&mut self, key: &str) → Option<T>  :23-23
  pub fn insert(&mut self, key: String, val: T)  :33-33
```

### src/worker/manifest_impl.rs
```
pub async fn get_manifest_cache() → anyhow::Result<Arc<MapTreeD...  :16-22
pub async fn fetch_map_manifest(args: FetchArgs) → anyhow::Result<MapManifestR...  :64-118
pub async fn fetch_fake_map_tiles(_args: FetchArgs) → anyhow::Result<Vec<FakeMapT...  :120-136
```

### src/worker/models.rs
```
pub async fn run_game_migrations(_: () → anyhow::Result<()>  :12-30
```

### src/worker/osm_impl.rs
```
pub async fn fetch_osm_data(args: FetchArgs) → anyhow::Result<OsmDataResult>  :14-104
```

### src/worker/pedestrian_impl.rs
```
pub async fn fetch_pedestrian_manifest(args: FetchArgs,) → anyhow::Result<PedestrianMa...  :11-93
pub async fn fetch_pedestrian_model(req: FetchGlbRequest) → anyhow::Result<FetchGlbResp...  :95-144
```

### src/worker/tile_impl.rs
```
pub async fn fetch_map_tile(req: FetchTileRequest) → anyhow::Result<FetchTileRes...  :58-126
pub async fn get_tile_collider(tile_id: &str) → Option<crate::tile::MeshCol...  :128-134
```

### src/worker/weapon_impl.rs
```
pub async fn fetch_weapon_manifest(args: FetchArgs) → anyhow::Result<WeaponManife...  :11-72
pub async fn fetch_weapon_model(req: FetchGlbRequest) → anyhow::Result<FetchGlbResp...  :74-119
```
