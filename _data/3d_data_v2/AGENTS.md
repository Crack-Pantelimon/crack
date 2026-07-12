

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

## deps
```
_blend_build_map.py ← mathutils, bmesh, bpy, numpy
_blend_render_postprocess.py ← __future__, mathutils, bpy
_blend_render_topdown.py ← __future__, mathutils, bpy
_check_blend.py ← bpy, numpy
build_blend.py ← bpy, numpy
earth_client.py ← config, google, requests, rocktree_pb2
glb_stats.py ← bpy
main.py ← octree, earth_client, mesh_decoder, numpy
mesh_decoder.py ← numpy, rocktree_pb2
osm_download.py ← octree, pyarrow, requests
osm_postprocess_batch.py ← octree, cv2, pyarrow, yolo_v8_obb_sat
rebuild_manifest.py ← pygltflib, octree, pyarrow
render_tile.py ← bpy, numpy
yolo_v7_sat.py ← __future__, cv2, numpy
yolo_v8_obb_sat.py ← __future__, cv2, numpy
```

## versions (installed direct deps)
```
huggingface-hub@1.23.0
numpy@2.5.0
pandas@3.0.3
pyarrow@24.0.0
pygltflib@1.16.5
requests@2.34.2
```

## .

### _blend_build_map.py
```
def clear_scene()  :28-30  # Wipe the current scene so the next tile imports into a clean
def weld_terrain_mesh(obj: bpy.types.Object, dist: float) → None  :33-48
def measure_terrain_bbox() → dict  :51-81  # Compute axis-aligned bounds of all mesh objects in Blender c
def latlon_to_xy(lon: float, lat: float, latlon_bbox: dict, terrain_bbox: dict) → tuple[float, float]  :84-99  # Map lat/lon to Blender (east, north) via bilinear extrapolat
def build_terrain_bvh(terrain_objs: list[bpy.types.Object] | None) → BVHTree | None  :102-131  # Build a world-space BVH over all terrain meshes once, up fro
def raycast_hit(x: float, y: float, top: float, bvh: BVHTree | None) → tuple[float, Vector] | None  :134-135  # Cast downward from above the terrain bbox; return (hit z, hi
def raycast_height(x: float, y: float, top: float, bvh: BVHTree | None) → float | None  :148-151  # Cast downward from above the terrain bbox; return hit z or N
def resolve_heights(heights: list[float | None]) → list[float] | None  :154-178  # Fill ray-cast misses from nearest chain neighbor with a hit
def get_or_create_collection(name: str) → bpy.types.Collection  :181-186
def create_road_object(feature_id, coords_xy: list[tuple[float, float]], heights: list[float]) → bpy.types.Object  :189-192  # Create a mesh polyline named road_<feature_id> in the roads 
def resolve_corner_heights(raw_zs: list[float | None], top: float) → list[float]  :208-212  # Fill corner ray misses with the average z of the corners tha
def build_collider_mesh(corners_latlon: list[list[float]], center_latlon: list[float], latlon_bbox: dict, terrain_bbox: dict, top: float, bvh: BVHTree | None) → dict  :215-221  # Build a closed box collider molded onto the terrain, enlarge
def create_car_object(car_index: int, verts: list[tuple[float, float, float]], faces: list[list[int]]) → bpy.types.Object  :299-302  # Link a pre-built collider mesh into the cars collection
def build_fill_material(mesh: bpy.types.Object, index: int) → tuple[int, int] | None  :371-372
def cut_car_from_terrain(mesh_obj: bpy.types.Object, car_obj: bpy.types.Object, z_range: tuple[float, float], mark_mat: bpy.types.Material, n_colors: int, fill_slot: int) → tuple[int, int, list]  :589-595
def log(msg: str) → None  :774-775
def process_item(item: dict) → None  :778-904
def main()  :907-936
```

### _blend_render_postprocess.py
```
def pick_render_engine() → str  :30-38
def convert_materials_to_emission() → None  :41-66  # Flatten every textured material to an unlit emission of its 
def make_cage_material() → bpy.types.Material  :69-98  # A translucent red-orange tint for the car wrappers: emission
def show_car_wrappers_as_cage() → None  :101-116  # Tint every object in the 'cars' collection translucent red s
def compute_mesh_bbox(objects) → dict | None  :119-144
def setup_world_black(scene: bpy.types.Scene) → None  :147-153
def render_blend(blend_path: str) → bool  :156-211
def main() → None  :214-230
```

### _blend_render_topdown.py
```
def enable_gpu_rendering() → list[str]  :25-55  # Enable GPU compute devices for Blender rendering
def pick_render_engine() → str  :58-66
def ensure_gpu_rendering() → None  :69-74
def clear_scene() → None  :77-96
def convert_materials_to_emission() → None  :99-123
def compute_mesh_bbox() → dict | None  :126-154
def resolve_resolution(tile: dict) → tuple[int, int]  :157-161
def setup_render_settings(scene: bpy.types.Scene, *, width: int, height: int) → None  :164-187
def render_tile(tile: dict) → bool  :190-253
def main() → None  :256-287
```

### _check_blend.py
```
def check_blend(blend_path: str) → None  :17-115
```

### build_blend.py
```
def unpack_vertices(packed: bytes) → np.ndarray  :21-35
def unpack_tex_coords(packed: bytes, vertex_count: int) → tuple[np.ndarray, float, fl...  :37-55
def unpack_indices_to_strip(packed: bytes) → np.ndarray  :57-74
def triangulate_strip(strip: np.ndarray, w_mask, masked_octants) → np.ndarray  :76-95
def unpack_octant_mask_and_layer_bounds(packed: bytes, indices: np.ndarray, vertex_count: int) → tuple[np.ndarray, list[int]]  :97-100
def unpack_for_normals(for_normals_bytes: bytes) → np.ndarray | None  :137-205
def unpack_normals(mesh_normals: bytes, for_normals: np.ndarray | None, vertex_count: int) → np.ndarray  :207-210
def apply_matrix(vertices_u8: np.ndarray, matrix: list[float]) → np.ndarray  :233-246
def transform_normals(normals: np.ndarray, matrix: list[float]) → np.ndarray  :248-258
def get_enu_rotation_matrix(ref_point: np.ndarray) → np.ndarray  :260-280  # Computes rotation matrix from ECEF space to local ENU tangen
def clear_scene()  :282-284  # Wipe the current scene so the next node imports into a clean
def build_one(json_path: str, out_glb_path: str, ref_point: np.ndarray)  :287-435  # Build a single node's
def main()  :438-471  # Process a batch of nodes described by a single JSON file in 
```

### debug_positions.py
```
def unpack_vertices(packed: bytes)  :4-12
def apply_matrix(vertices, matrix)  :14-22
def get_enu_rotation_matrix(ref_point)  :24-37
```

### earth_client.py (used by: main.py)
```
class BulkIndex  :171-265
  def __init__(bulk: pb.BulkMetadata, parent_path: str)
  def get_node(rel_path: str) → tuple[pb.NodeMetadata, int] | 
  def has_data(rel_path: str) → bool
  def is_leaf(rel_path: str) → bool
  def has_bulk_children(rel_path: str) → bool
  def get_node_epoch(rel_path: str) → int
  def get_bulk_epoch(rel_path: str) → int
  def get_texture_format(rel_path: str) → int
class NodeInfo  :268-275
  def __init__(path: str, epoch: int, texture_format: int, imagery_epoch: int | None)
def fetch_planetoid_metadata() → pb.PlanetoidMetadata  :105-114  # Fetch PlanetoidMetadata from the server
def fetch_bulk_metadata(path: str, epoch: int) → pb.BulkMetadata  :117-127  # Fetch BulkMetadata for a given octant path and epoch
def fetch_node_data(path: str, epoch: int, texture_format: int, imagery_epoch: int | None) → pb.NodeData  :130-134  # Fetch NodeData for a given octant path, epoch, and texture f
def unpack_path_and_flags(path_and_flags: int) → tuple[str, int]  :152-168  # Unpack the path_and_flags field from NodeMetadata
def resolve_node(octant_path: str, root_epoch: int) → NodeInfo | None  :311-380  # Walk the bulk metadata tree from the root to resolve a speci
def download_node(node_info: NodeInfo) → pb.NodeData  :383-390  # Download and parse NodeData for a resolved node
def find_tiles_in_bbox(bbox, target_level: int, root_epoch: int) → list[str]  :393-456  # Dynamically traverse the bulk metadata tree to find all non-
def find_tiles_in_bbox_levels(bbox, min_level: int, max_level: int, root_epoch: int, bulk_fetch_workers: int) → dict[int, list[str]]  :463-468
```

### glb_stats.py
```
def glb_stats(glb_path: str) → dict  :21-57
```

### main.py
```
def run_blender_batch(script: str, batch_json_path: str) → str  :43-60  # Run a Blender -P script over a whole batch (single process) 
def compute_reference_point(bbox) → np.ndarray  :74-91  # Compute ECEF reference point from the bounding box center
def main()  :94-364  # Main pipeline: parse bbox → compute level → download → expor
```

### mesh_decoder.py (used by: main.py)
```
class DecodedMesh  :350-361
  def __init__()
def unpack_vertices(packed: bytes) → np.ndarray  :32-54  # Unpack vertices from Mesh
def unpack_tex_coords(packed: bytes, vertex_count: int) → tuple[np.ndarray, float, fl...  :57-84  # Unpack texture coordinates from Mesh
def unpack_indices_to_strip(packed: bytes) → np.ndarray  :87-108  # Unpack triangle indices from Mesh
def triangulate_strip(strip: np.ndarray, w_mask: np.ndarray, masked_octants: set[int]) → np.ndarray  :111-139  # Convert a triangle strip to a list of triangle indices
def unpack_octant_mask_and_layer_bounds(packed: bytes, indices: np.ndarray, vertex_count: int) → tuple[np.ndarray, list[int]]  :142-145  # Unpack octant mask (W value per vertex) and layer bounds
def unpack_for_normals(for_normals_bytes: bytes) → np.ndarray | None  :188-262  # Unpack the for_normals field from NodeData
def unpack_normals(mesh_normals: bytes, for_normals: np.ndarray | None, vertex_count: int) → np.ndarray  :265-268  # Unpack normals for a mesh using mesh_normals indices and for
def apply_matrix(vertices_u8: np.ndarray, matrix: list[float]) → np.ndarray  :298-300  # Transform local 8-bit vertex coordinates to world (ECEF) coo
def transform_normals(normals: np.ndarray, matrix: list[float]) → np.ndarray  :328-330  # Transform normals using the 3x3 rotation part of the matrix
def get_enu_rotation_matrix(ref_point: np.ndarray) → np.ndarray  :364-385  # Computes rotation matrix from ECEF space to local ENU tangen
def decode_node(node_data: pb.NodeData, masked_octants: set[int] | None) → list[DecodedMesh]  :387-481  # Fully decode all meshes from a NodeData protobuf message
```

### migrate_glb_paths.py
```
def main()  :8-32
```

### octree.py (used by: main.py, osm_download.py, osm_postprocess_batch.py, rebuild_manifest.py)
```
@dataclass BBox(north, south, west, east)  :15-36
def parse_bbox(filepath: str) → BBox  :39-57  # Parse bounding box from file
def get_first_octant(lat: float, lon: float) → tuple[str, BBox]  :60-80  # Get the first 2-character octant path and bounding box for a
def get_next_octant(box: BBox, lat: float, lon: float) → tuple[int, BBox]  :83-117  # Given a bounding box, determines which sub-octant (0-3) a la
def lat_lon_to_octant(lat: float, lon: float, level: int) → str  :120-129  # Convert a lat/lon to an octant path at the given level
def octant_path_to_bbox(path: str) → BBox  :132-177  # Given an octant path, compute its lat/lon bounding box
def child_bbox(box: BBox, digit: int) → BBox  :180-206  # Compute the bounding box of a single child octant (digit 0-7
def compute_best_level(bbox: BBox, target_grid: int, max_level: int) → int  :209-240  # Compute the octree level that gives approximately target_gri
def enumerate_octants_in_bbox(bbox: BBox, level: int) → list[str]  :243-274  # Enumerate all octant paths at the given level that overlap w
def tile_grid_dimensions(bbox: BBox, level: int) → tuple[int, int]  :277-283  # Return approximate (rows, cols) of the tile grid at the give
```

### osm_download.py
```
def format_eta(seconds: float) → str  :90-100  # Format seconds into a human-readable ETA string
def download_category_with_retry(query_part: str, bbox_str: str, proxies: dict, headers: dict) → dict | None  :103-105  # Download OSM data for a query part from public Overpass API 
def download_all()  :171-285  # Stage 1: download OSM layers into data_osm/original/
def load_octree_index()  :288-321  # Load octree tile bboxes and parent/child relationships from 
def iter_lonlat(geometry)  :335-343  # Yield (lon, lat) from any GeoJSON geometry
def collect_octant_paths(points, path, bboxes, children)  :346-356  # Recursively assign points to octant paths using half-open bb
def assign_feature_paths(geometry, bboxes, children, roots)  :359-368  # Return the set of octant paths that contain any point of the
def build_feature_manifests()  :371-440  # Stage 2: build features
def main()  :443-445
```

### osm_postprocess_batch.py
```
def run_blender_batch(script: str, batch_json_path: str) → str  :52-80  # Run a Blender -P script over a batch JSON file, streaming it
def glb_path_for_tile(tile: str) → Path  :83-87  # Return the on-disk GLB path for an octant path (matches main
def tile_sidecar_paths(tile: str) → dict[str, Path]  :90-97
def pixel_to_latlon(px: float, py: float, meta: dict) → tuple[float, float]  :100-120  # Map render pixel to lat/lon using ortho camera + mesh-bbox a
def obb_pixel_to_latlon_corners(corners_pixel: list[list[float]], meta: dict) → tuple[list[list[float]], li...  :123-124
def node_inside_bbox(lon: float, lat: float, bbox) → bool  :133-135  # Half-open containment: south <= lat < north, west <= lon < e
def trim_road_feature(feature: dict, bbox) → dict | None  :138-175  # Keep coordinate indices inside bbox or adjacent to an inside
def has_lanes(feature: dict) → bool  :178-180
def lookup_manifest_row(manifest_dataset, tile: str) → dict | None  :183-189
def query_road_feature_ids(octtree_dataset, candidate: str) → list  :192-203
def load_road_features(features_dataset, feature_ids: list[int]) → list[dict]  :206-223
def find_roads_for_tile(tile: str, octtree_dataset, features_dataset) → tuple[str | None, list[dict]]  :226-229  # Walk up the parent chain from tile until qualifying roads ar
def build_work_item(tile: str, manifest_row: dict, road_source_path: str | None, roads: list[dict], sidecars: dict[str, Path]) → dict  :254-259
def load_sample_tiles() → list[str]  :288-290
def run_render_stage(tile_specs: list[dict]) → None  :293-307
def run_detect_stage(tile_records: list[dict], net) → None  :310-358
def run_blend_stage(items: list[dict]) → None  :361-375
def main()  :378-469
```

### pyproject.toml
```
table [project]
key name
key version
key description
key readme
key requires-python
key dependencies
```

### README.md
```
h1 3D Data V2
h2 Setup and Running
h3 Running the Main Script
h1 Option 1: Run directly with uv
h1 Option 2: Run arbitrary scripts using uv's python
h3 Rebuilding the Manifest
h3 Running from another context
h2 Known Issues Addressed
code-fence bash
code-fence plain
```

### rebuild_manifest.py
```
def get_glb_stats(glb_path: str) → dict  :42-82  # Extract stats from a single
def build_row(glb_path: str) → dict  :85-118  # Assemble a single manifest row for one
def main()  :121-195
```

### render_tile.py
```
def render_blend(blend_path, out_jpg_path, ref_point)  :8-113
def main()  :115-146  # Render a batch of nodes described by a single JSON file in o
```

### rocktree.proto
```
syntax = "proto2"
package geo_globetrotter_proto_rocktree
message BulkMetadataRequest
message NodeDataRequest
message NodeKey
message CopyrightRequest
message TextureDataRequest
message BulkMetadata
message NodeMetadata
message NodeData
message Mesh
message Texture
message TextureData
message Copyrights
message Copyright
message PlanetoidMetadata
enum Flags
enum Layer
enum LayerMask
enum Format
enum ViewDirection
```

### yolo_v7_sat.py
```
def load_net(onnx_path: Path | str) → cv2.dnn.Net  :14-18
def detect_cars(net: cv2.dnn.Net, image_bgr: np.ndarray, *, conf: float, nms: float) → list[dict]  :21-26  # Return car detections as pixel bboxes in the source image
```

### yolo_v8_obb_sat.py (used by: osm_postprocess_batch.py)
```
def load_net(onnx_path: Path | str) → cv2.dnn.Net  :29-33
def detect_cars(net: cv2.dnn.Net, image_bgr: np.ndarray, *, conf: float, nms: float) → list[dict]  :43-48  # Return vehicle detections as rotated pixel quads in the sour
```
