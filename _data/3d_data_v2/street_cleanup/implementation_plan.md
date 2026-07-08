# Street Obstacle Detection & Removal Pipeline

Remove cars, trees, and other satellite-captured obstacles from the GTA Pantelimon 3D map so that streets are flat and driveable. The physics engine (Avian3D) uses `TrimeshFromMesh` colliders built directly from the GLB geometry, so bumps from cars/trees on the satellite imagery translate into physical obstacles in-game.

## Context & Architecture Summary

### How the map works today

1. **Data origin**: Google Earth satellite photogrammetry tiles are downloaded as protobuf `NodeData`, decoded, and exported to `.glb` files by [main.py](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/main.py) + [build_blend.py](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/build_blend.py).
2. **Tile organization**: Tiles are stored under `_data/3d_data_v2/data_out/{depth}/{last3}/{octant_path}.glb`. There are **6,114 tiles** across depths 10–20. The highest-detail tiles are at depths 19 (3,009 tiles) and 20 (2,052 tiles).
3. **Manifest**: [rebuild_manifest.py](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/rebuild_manifest.py) scans all `.glb` files and produces `data_out/manifest.parquet` with schema: `octant_path, depth, glb_path, file_size_bytes, vertex_count, triangle_count, mesh_count, lat_north, lat_south, lon_west, lon_east, x_min, y_min, z_min, x_max, y_max, z_max`.
4. **Game loading**: The Bevy client ([map_metadata_parquet.rs](file:///home/vasile/.gemini/antigravity/scratch/crack/crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_metadata_parquet.rs)) fetches `manifest.parquet` over HTTP, parses it, and streams `.glb` tiles. Each tile spawns with `RigidBody::Static` + `ColliderConstructorHierarchy(TrimeshFromMesh)` — meaning the **exact mesh geometry becomes the collision surface**.
5. **Coordinate system**: The pipeline converts ECEF → local ENU (East, North, Up) relative to reference point Lat: 44.445522, Lon: 26.142436 (Cora Pantelimon). In Blender (Z-up): X=East, Y=North, Z=Up. In Bevy (Y-up, from GLTF export): X=East, Y=Up, Z=-North.

### Bounding box
- NW corner: `44.4597799, 26.119593`  
- SE corner: `44.4312648, 26.1652789`

### OSM data
Already downloaded to `_data/3d_data_v2/data_osm/`. The [roads.geojson](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/data_osm/roads.geojson) file contains **5,088 features** with highway types including: `residential` (892), `primary` (199), `secondary` (145), `tertiary` (108), `service` (1,007), `living_street` (22), etc.

### Existing Blender rendering script
[render_tile.py](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/render_tile.py) already renders `.blend` files to `.jpg` from a top-down orthographic-like view (camera placed directly above looking down). This is our starting point.

### Python environment
The `_data/3d_data_v2/` directory has its own `uv` virtual environment with [pyproject.toml](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/pyproject.toml). Currently depends on: `numpy`, `pillow`, `protobuf`, `pyarrow`, `pygltflib`, `requests[socks]`. New dependencies (e.g. `ultralytics`, `shapely`, `trimesh`) will be added here.

---

## High-Level Pipeline Overview

```
┌──────────────────┐     ┌──────────────────────┐     ┌────────────────────┐     ┌──────────────────────┐
│  Phase 1          │     │  Phase 2              │     │  Phase 3            │     │  Phase 4              │
│  OSM Street       │────▶│  Render Tiles to      │────▶│  YOLO Object        │────▶│  Map Detections Back  │
│  Extraction       │     │  Top-Down Images      │     │  Detection          │     │  to 3D & Flatten Mesh │
└──────────────────┘     └──────────────────────┘     └────────────────────┘     └──────────────────────┘
```

---

## Phase 1: OSM Street Extraction & Tile Identification

**Goal**: Parse the OSM roads GeoJSON, extract street geometries with their widths, convert to the same coordinate system as our tiles, and identify which tiles (from the manifest) intersect with streets.

---

### Task 1.1: Create the pipeline directory structure

- [ ] Create directory `_data/3d_data_v2/street_cleanup/` — this is the working directory for all new scripts
- [ ] Create directory `_data/3d_data_v2/street_cleanup/renders/` — output directory for top-down rendered JPGs
- [ ] Create directory `_data/3d_data_v2/street_cleanup/detections/` — output directory for YOLO detection results
- [ ] Create directory `_data/3d_data_v2/street_cleanup/patches/` — output directory for flattened/patched GLBs

---

### Task 1.2: Add Python dependencies

- [ ] Add `shapely>=2.0` to `pyproject.toml` (for geometric operations: buffering roads into polygons, intersection tests)
- [ ] Add `ultralytics>=8.0` to `pyproject.toml` (for YOLO object detection)
- [ ] Add `trimesh>=4.0` to `pyproject.toml` (for programmatic GLB/mesh manipulation without Blender)
- [ ] Add `opencv-python-headless>=4.0` to `pyproject.toml` (required by ultralytics, use headless variant for server)
- [ ] Run `uv sync` from `_data/3d_data_v2/` to install all new dependencies

---

### Task 1.3: Write `street_cleanup/extract_streets.py` — Parse OSM roads

This script reads the OSM roads GeoJSON and outputs a cleaned, filtered set of street geometries.

- [ ] Read `data_osm/roads.geojson` using `json.load()`
- [ ] Filter features to only include **driveable** road types. Include only features where `properties.tags.highway` is one of: `primary`, `secondary`, `tertiary`, `residential`, `service`, `living_street`, `trunk`, `motorway`, `motorway_link`, `trunk_link`, `primary_link`, `secondary_link`, `tertiary_link`, `unclassified`, `construction`
- [ ] Exclude non-driveable types: `footway`, `crossing`, `steps`, `pedestrian`, `path`, `cycleway`, `track`, `bus_stop`, `traffic_signals`, `street_lamp`, `give_way`, `elevator`
- [ ] For each accepted road feature:
  - Extract the geometry coordinates (note: GeoJSON uses `[lon, lat]` order)
  - Handle both `LineString` and `MultiLineString` geometry types
  - Skip `Point` features (traffic signals, bus stops, etc.)
  - Store the highway type (used later for road width estimation)
- [ ] Define road width lookup table (in meters) based on highway type:
  - `motorway` / `trunk`: 12m (3 lanes each direction)
  - `primary`: 10m
  - `secondary`: 8m
  - `tertiary`: 7m
  - `residential` / `living_street`: 6m
  - `service`: 4m
  - `unclassified`: 5m
  - `*_link` types: 4m
  - Default: 6m
- [ ] If the OSM feature has a `lanes` tag, override the width: `width = lanes * 3.5m`
- [ ] Use `shapely.geometry.LineString` to represent each road segment
- [ ] Buffer each road LineString by `road_width / 2` to create a `Polygon` representing the road surface area
- [ ] Merge all road polygons using `shapely.ops.unary_union` to create a single `MultiPolygon` of all driveable road surfaces
- [ ] Save the merged road polygon to `street_cleanup/road_polygons.geojson` (as a GeoJSON FeatureCollection with a single MultiPolygon feature) for debugging/visualization
- [ ] Print summary stats: total road features processed, total road length (km), total road area (m²)

> [!IMPORTANT]
> The GeoJSON coordinates are in WGS84 (lat/lon degrees). All geometric operations (buffering, area calculations) should be done in a **projected coordinate system** (meters). Use a local UTM projection (Zone 35N for Bucharest/Pantelimon, EPSG:32635) or a simple Mercator approximation. Shapely's `buffer()` on lat/lon would produce incorrect results.
> 
> Recommended approach: Convert lat/lon to local ENU meters using the same reference point as the 3D pipeline (Lat: 44.445522, Lon: 26.142436) with the formula from the existing codebase. This way the road polygons will be in the **same coordinate frame** as the mesh data.

---

### Task 1.4: Write `street_cleanup/coord_utils.py` — Coordinate conversion utilities

- [ ] Implement `latlon_to_enu(lat, lon, ref_lat=44.445522, ref_lon=26.142436)` → `(east_m, north_m)`
  - Use the standard geodetic to ENU conversion:
    - `east = (lon - ref_lon) * cos(ref_lat) * 111319.9`
    - `north = (lat - ref_lat) * 111319.9`
  - (These are approximate but sufficient for a ~3km area)
- [ ] Implement `enu_to_latlon(east_m, north_m, ref_lat=44.445522, ref_lon=26.142436)` → `(lat, lon)` (inverse)
- [ ] Implement `enu_to_bevy(east, north, up)` → `(bevy_x, bevy_y, bevy_z)`:
  - `bevy_x = east`
  - `bevy_y = up`
  - `bevy_z = -north`
- [ ] Implement `bevy_to_enu(bevy_x, bevy_y, bevy_z)` → `(east, north, up)`:
  - `east = bevy_x`
  - `north = -bevy_z`
  - `up = bevy_y`
- [ ] Implement `glb_xyz_to_enu(x_min, y_min, z_min, x_max, y_max, z_max)` → `BBox` in ENU coordinates
  - The manifest's `x_min/x_max` = East range
  - The manifest's `y_min/y_max` = Height range (ENU Up)
  - The manifest's `z_min/z_max` = North range (but **negative** because the GLTF export flips Y→Z with negation)
  - So: `east_range = [x_min, x_max]`, `north_range = [-z_max, -z_min]`, `up_range = [y_min, y_max]`

> [!NOTE]
> The coordinates in the manifest's `x_min/y_min/z_min/x_max/y_max/z_max` fields are in the GLTF/Bevy coordinate space (Y-up), NOT Blender's Z-up space. The mapping is:
> - Manifest `x` = East (same in both)
> - Manifest `y` = Up (Bevy Y = Blender Z)
> - Manifest `z` = -North (Bevy Z = -Blender Y)

---

### Task 1.5: Write `street_cleanup/find_street_tiles.py` — Identify which tiles intersect streets

- [ ] Load `data_out/manifest.parquet` using `pyarrow`
- [ ] Load road polygons from Task 1.3 (the merged `MultiPolygon` in ENU coordinates)
- [ ] For **each row** in the manifest:
  - Convert the tile's xyz bbox (from manifest columns `x_min, y_min, z_min, x_max, y_max, z_max`) to ENU coordinates using `coord_utils.glb_xyz_to_enu()`
  - Create a 2D `shapely.geometry.box(east_min, north_min, east_max, north_max)` from the ENU bbox (ignoring height)
  - Test if this 2D box **intersects** the road polygon MultiPolygon
  - If yes, add this tile to the "street tiles" list
- [ ] Filter to only include **high-detail tiles** (depth >= 18). Lower LODs cover huge areas and are too coarse for meaningful obstacle detection. The game primarily uses depth 19-20 tiles for close-up views anyway.
- [ ] Save the list of street-intersecting tiles to `street_cleanup/street_tiles.parquet` with columns:
  - `octant_path` (string)
  - `depth` (int)
  - `glb_path` (string — relative path to the GLB file)
  - `east_min, east_max, north_min, north_max` (float — ENU bbox of the tile)
  - `road_coverage_ratio` (float — what fraction of the tile's 2D area is covered by roads)
- [ ] Print summary: total tiles in manifest, tiles intersecting streets, breakdown by depth
- [ ] Also compute and store the intersection polygon between each tile and the road surface (for use in Phase 4)

---

## Phase 2: Render Tiles to Top-Down Images

**Goal**: For each street-intersecting tile, render a top-down orthographic image that can be fed to YOLO for object detection.

---

### Task 2.1: Write `street_cleanup/render_street_tiles.py` — Blender top-down rendering script

This is a Blender Python script (run via `blender -b -P`). It must be based on the existing [render_tile.py](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/render_tile.py) but modified for our use case.

- [ ] Accept a batch JSON file as input (same pattern as `render_tile.py`): `blender -b -P render_street_tiles.py -- <batch_json_path>`
- [ ] The batch JSON format:
  ```json
  {
    "nodes": [
      {
        "glb_path": "data_out/19/xxx/30436272xxxx.glb",
        "jpg_path": "street_cleanup/renders/30436272xxxx.jpg",
        "octant_path": "30436272xxxx"
      }
    ]
  }
  ```
- [ ] For each node in the batch:
  1. Clear the scene (`bpy.ops.wm.read_factory_settings(use_empty=True)`)
  2. Import the GLB file: `bpy.ops.import_scene.gltf(filepath=glb_path)`
  3. Compute the bounding box of all mesh objects (iterate all vertices in world space)
  4. Set up an **orthographic** camera (NOT perspective, as in the current `render_tile.py`):
     - `cam_data = bpy.data.cameras.new(name="Camera")`
     - `cam_data.type = 'ORTHO'`
     - `cam_data.ortho_scale = max(size_x, size_y) * 1.05` (slightly larger than the tile to avoid clipping)
  5. Position the camera **directly above** the center of the bounding box, looking straight down:
     - `cam_object.location = (center_x, center_y, center_z + distance)` (in Blender Z-up space)
     - Point camera down using a Track-To constraint targeting the center
  6. Add a sun light (directional, pointing down) for even illumination
  7. Use **EEVEE** render engine (NOT Cycles) for speed: `bpy.context.scene.render.engine = 'BLENDER_EEVEE_NEXT'` (Blender 4.x). Fall back to `'BLENDER_EEVEE'` for older versions
  8. Set render resolution to **1024×1024** pixels (high enough for YOLO to detect small objects)
  9. Set background color to a neutral gray
  10. Render to JPEG: `bpy.ops.render.render(write_still=True)`
  11. Print `RENDER_OK <octant_path>` on success or `RENDER_FAIL <octant_path>: <error>` on failure
- [ ] Use `try/except` around each node to allow the batch to continue if one node fails

> [!IMPORTANT]
> **Why orthographic camera?** We need the rendered image to have a consistent, measurable mapping between pixels and world-space meters. With an orthographic camera, 1 pixel = a fixed number of meters, making it trivial to convert YOLO bounding box coordinates back to 3D world coordinates. With a perspective camera, the mapping varies across the image.

> [!TIP]
> For each rendered image, also save a sidecar JSON with the render metadata:
> ```json
> {
>   "octant_path": "30436272xxxx",
>   "center_enu": [east, north, up],
>   "ortho_scale": 45.6,
>   "resolution": [1024, 1024],
>   "meters_per_pixel": 0.0445,
>   "bbox_min_enu": [east_min, north_min],
>   "bbox_max_enu": [east_max, north_max]
> }
> ```
> This metadata is critical for Phase 4 (mapping pixel coordinates back to world coordinates).

---

### Task 2.2: Write `street_cleanup/run_renders.py` — Orchestrator for rendering

This is a regular Python script (NOT a Blender script) that orchestrates the rendering process.

- [ ] Load the street tiles list from `street_cleanup/street_tiles.parquet`
- [ ] For each tile, check if `street_cleanup/renders/{octant_path}.jpg` already exists (skip if so, for re-entrancy)
- [ ] Group tiles into batches of 32 (matching the existing `BLENDER_BATCH_SIZE` pattern from [main.py](file:///home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2/main.py))
- [ ] For each batch:
  1. Write a temporary batch JSON file
  2. Run `blender -b -P street_cleanup/render_street_tiles.py -- <batch_json>` via `subprocess.run()`
  3. Check which renders were actually produced on disk
  4. Log progress: `[batch N/M] Rendered X/Y tiles successfully`
- [ ] Use `ThreadPoolExecutor` with `max_workers=4` to run multiple Blender processes in parallel (Blender instances for EEVEE rendering are CPU-bound and can run concurrently)
- [ ] After all batches complete, print summary: total rendered, total skipped (already existed), total failed
- [ ] Run with: `uv run python street_cleanup/run_renders.py`

---

### Task 2.3: Store render metadata alongside images

- [ ] For each successfully rendered tile, the Blender script (Task 2.1) must also write a metadata JSON file to `street_cleanup/renders/{octant_path}.json` containing:
  - `octant_path`: tile identifier
  - `glb_path`: path to source GLB
  - `render_resolution`: `[width, height]` in pixels
  - `ortho_scale`: the orthographic camera scale (world units covered)
  - `center_blender`: `[x, y, z]` center of the bounding box in Blender coords
  - `bbox_min_blender`: `[x, y, z]` minimum bounds in Blender coords
  - `bbox_max_blender`: `[x, y, z]` maximum bounds in Blender coords
  - `meters_per_pixel_x`: `ortho_scale / resolution_x` (horizontal)
  - `meters_per_pixel_y`: `ortho_scale / resolution_y` (vertical)
- [ ] This metadata is essential for mapping YOLO bounding boxes (in pixels) back to 3D coordinates

---

## Phase 3: YOLO Object Detection

**Goal**: Run YOLO on all rendered top-down tile images to detect cars, trucks, trees, and other obstacles.

---

### Task 3.1: Write `street_cleanup/run_yolo.py` — YOLO detection script

- [ ] Import `ultralytics` YOLO: `from ultralytics import YOLO`
- [ ] Load a pre-trained YOLO model. Use `yolo11n.pt` (YOLOv11 nano) or `yolov8n.pt` (YOLOv8 nano) for speed, or `yolo11m.pt` / `yolov8m.pt` (medium) for better accuracy. The model will auto-download on first use.
  ```python
  model = YOLO("yolo11m.pt")  # or "yolov8m.pt"
  ```
- [ ] Define the COCO class IDs we care about (obstacles to detect):
  ```python
  TARGET_CLASSES = {
      2: "car",
      3: "motorcycle", 
      5: "bus",
      7: "truck",
      # Trees are not a standard COCO class, so we may need to handle them differently
      # (see Task 3.2 for tree detection strategy)
  }
  ```
- [ ] Iterate over all rendered JPGs in `street_cleanup/renders/`:
  - Load the image
  - Run inference: `results = model.predict(source=image_path, conf=0.25, iou=0.45, classes=list(TARGET_CLASSES.keys()))`
  - For each detection:
    - Extract: class_id, class_name, confidence, bbox (`x1, y1, x2, y2` in pixels)
    - Load the corresponding render metadata JSON (`street_cleanup/renders/{octant_path}.json`)
    - Convert pixel bbox to Blender/ENU world coordinates using the metadata:
      ```python
      world_x_min = bbox_min_blender_x + (x1 * meters_per_pixel_x)
      world_y_min = bbox_min_blender_y + (y1 * meters_per_pixel_y)  # Note: image Y is inverted
      world_x_max = bbox_min_blender_x + (x2 * meters_per_pixel_x)
      world_y_max = bbox_min_blender_y + (y2 * meters_per_pixel_y)
      ```
    - Store detection with both pixel and world coordinates
- [ ] Save all detections to `street_cleanup/detections/all_detections.parquet` with columns:
  - `octant_path` (string)
  - `class_id` (int)
  - `class_name` (string)
  - `confidence` (float)
  - `pixel_x1, pixel_y1, pixel_x2, pixel_y2` (int — bbox in image pixels)
  - `world_east_min, world_east_max, world_north_min, world_north_max` (float — bbox in ENU meters)
  - `glb_path` (string)
- [ ] Also save per-tile detection results as individual JSONs: `street_cleanup/detections/{octant_path}.json`
- [ ] Generate annotated debug images: copy each render and draw bounding boxes + labels on it, save to `street_cleanup/detections/annotated/{octant_path}.jpg` for visual inspection
- [ ] Print summary: total images processed, total detections, breakdown by class
- [ ] Run with: `uv run python street_cleanup/run_yolo.py`

> [!IMPORTANT]
> **Coordinate mapping from pixels to world**: The image is rendered with an orthographic camera looking straight down. The image top corresponds to Blender's +Y (North) and image left corresponds to Blender's -X (West). Be careful with axis orientations:
> - Image pixel `(0, 0)` = top-left of image = `(bbox_min_x, bbox_max_y)` in Blender coords (North-West corner)
> - Image pixel `(W, H)` = bottom-right = `(bbox_max_x, bbox_min_y)` in Blender coords (South-East corner)
> - `world_x = bbox_min_x + (pixel_x / width) * (bbox_max_x - bbox_min_x)` (East)
> - `world_y = bbox_max_y - (pixel_y / height) * (bbox_max_y - bbox_min_y)` (North, inverted because image Y grows down)

---

### Task 3.2: Tree detection strategy

YOLO's standard COCO model doesn't include a "tree" class. Options to detect trees:

- [ ] **Option A (Recommended): Use COCO's "potted plant" class (ID 58) as a rough proxy.** Trees from above may trigger this class. Test this first.
- [ ] **Option B: Green-area segmentation.** After YOLO detection, additionally run a simple color-based segmentation on the rendered images to find green patches (trees/bushes). Use OpenCV HSV color filtering:
  ```python
  hsv = cv2.cvtColor(image, cv2.COLOR_BGR2HSV)
  # Green range in HSV
  lower_green = np.array([25, 40, 40])
  upper_green = np.array([90, 255, 255])
  mask = cv2.inRange(hsv, lower_green, upper_green)
  ```
  Then find contours of green regions and create bounding boxes from them.
- [ ] **Option C: Use a custom/fine-tuned model.** Fine-tune YOLO on aerial/satellite imagery datasets that include trees. This is the most accurate but most time-consuming approach. **Only pursue this if Options A+B are insufficient.**
- [ ] Whichever option is chosen, append tree detections to the same `all_detections.parquet` with `class_name = "tree"`

---

### Task 3.3: Filter detections to street areas only

Not all detections matter — we only care about obstacles **on the streets** (not in yards, parks, etc.).

- [ ] Load `all_detections.parquet` from Task 3.1
- [ ] Load road polygons from Phase 1 (`street_cleanup/road_polygons.geojson` in ENU coordinates)
- [ ] For each detection:
  - Create a `shapely.geometry.box(world_east_min, world_north_min, world_east_max, world_north_max)`
  - Test if this box **intersects** the road polygon
  - If yes, keep the detection; if no, discard it
- [ ] Save filtered detections to `street_cleanup/detections/street_detections.parquet` (same schema as `all_detections.parquet`)
- [ ] Print summary: total detections, detections on streets, detections off streets (discarded)

> [!TIP]
> Gabriel mentioned that the detected car positions are "best parking spots ever" — the detected bounding boxes of cars on streets can later be used as parking spot definitions. Consider saving a separate `parking_spots.parquet` with the car detection locations for future game features.

---

## Phase 4: Map Detections Back to 3D & Flatten Mesh

**Goal**: For each detection on a street, modify the GLB mesh to flatten the geometry in that area, removing the obstacle bump.

---

### Task 4.1: Write `street_cleanup/flatten_mesh.py` — Core mesh flattening logic

This script uses `trimesh` (or `pygltflib` which is already installed) to modify GLB files.

- [ ] Implement the core function `flatten_region(glb_path, output_glb_path, regions)`:
  - `regions` is a list of `(east_min, east_max, north_min, north_max)` rectangles (in ENU/Blender coordinates) where geometry should be flattened
  - Load the GLB using `trimesh.load(glb_path)`
  - For each mesh in the GLB scene:
    - Get the vertices as a numpy array
    - For each flatten region:
      - Find all vertices whose (X, Y) in Blender coords (East, North) fall within the region rectangle
      - **Note**: In the GLB/Bevy coordinate space: X=East, Z=-North. So check: `east_min <= vertex.x <= east_max` AND `north_min <= -vertex.z <= north_max` (since Z is negated North)
      - For the selected vertices, compute the **median Z height** (or the height of the surrounding road surface) of non-selected neighboring vertices along the edges of the region
      - Set the Z coordinate (Up) of all selected vertices to this median/interpolated height
      - This effectively "flattens" the obstacle into the road surface
    - Write modified vertex data back to the mesh
  - Export the modified mesh to `output_glb_path`
  - Preserve all texture/material data — only vertex positions change

> [!WARNING]  
> **UV and texture preservation**: When flattening vertices, we MUST NOT modify UV coordinates or texture data. Only the vertex positions (specifically the height/Z component) should change. The texture will still show the car/tree imagery, but the geometry will be flat. This is acceptable because the visual artifacts from top-down textures are minor compared to the physics problems caused by bumpy geometry.

---

### Task 4.2: Determine target flattening height for each detection

- [ ] For each detection region on a street:
  - Load the GLB mesh for that tile
  - Sample the height (Z in Blender / Y in Bevy) of vertices **surrounding** the detection bbox but still on the road
  - Compute the median height of these surrounding road-surface vertices
  - This median becomes the "target height" for flattening
- [ ] Alternative simpler approach: for each tile, compute the **road surface height** by:
  - Finding the most common height value (histogram mode) among all vertices that fall within road polygons but NOT within any detection bbox
  - Use this as the uniform target height for all flatten operations on this tile
- [ ] Store target heights alongside detections in `street_detections_with_heights.parquet`

---

### Task 4.3: Write `street_cleanup/apply_flattening.py` — Apply flattening to all affected tiles

- [ ] Load `street_detections_with_heights.parquet`
- [ ] Group detections by `octant_path` (tile)
- [ ] For each tile with detections:
  1. Read the original GLB from `data_out/{depth}/{last3}/{octant_path}.glb`
  2. Collect all detection regions for this tile
  3. Call `flatten_region(original_glb, patched_glb, regions)` from Task 4.1
  4. Save the patched GLB to `street_cleanup/patches/{depth}/{last3}/{octant_path}.glb` (DO NOT overwrite originals!)
  5. Log: `Flattened {N} regions in tile {octant_path}`
- [ ] After all tiles are processed, print summary: total tiles modified, total regions flattened
- [ ] Run with: `uv run python street_cleanup/apply_flattening.py`

> [!CAUTION]
> **Never overwrite the original GLB files.** Always write patches to a separate directory. This allows:
> 1. Easy comparison between original and patched versions
> 2. Rollback if something goes wrong
> 3. Incremental re-processing

---

### Task 4.4: Write `street_cleanup/install_patches.py` — Replace originals with patched files

This is a separate, explicit step to install the patches.

- [ ] Iterate all patched GLB files in `street_cleanup/patches/`
- [ ] For each patch:
  1. Create a backup of the original: copy `data_out/{path}.glb` to `data_out/{path}.glb.bak`
  2. Copy the patched GLB to overwrite the original: `data_out/{path}.glb`
- [ ] After installation, run `uv run rebuild_manifest.py` to regenerate the manifest with updated mesh stats
- [ ] Print summary: total files patched, total bytes saved
- [ ] Include a `--dry-run` flag that only prints what would be done without modifying files
- [ ] Include a `--rollback` flag that restores all `.glb.bak` files to their original paths

---

## Phase 5: Validation & Results

**Goal**: Verify that the flattening worked correctly.

---

### Task 5.1: Re-render patched tiles for visual comparison

- [ ] After patching, re-render the affected tiles using the same rendering pipeline from Phase 2
- [ ] Save re-renders to `street_cleanup/renders_patched/{octant_path}.jpg`
- [ ] Write a comparison script that creates side-by-side images: original render on the left, patched render on the right
- [ ] Save comparisons to `street_cleanup/comparisons/{octant_path}.jpg`

---

### Task 5.2: Generate detection results summary

- [ ] Write `street_cleanup/summarize.py` that produces a summary report:
  - Total tiles in the map: 6,114
  - Total tiles intersecting streets: N
  - Total tiles processed by YOLO: N
  - Total detections (all): N
  - Total detections on streets: N
  - Breakdown by class (cars, trucks, buses, motorcycles, trees)
  - Total tiles modified: N
  - Total vertices flattened: N
- [ ] Output the summary as both console text and a markdown file `street_cleanup/RESULTS.md`

---

### Task 5.3: Save parking spots for future use

- [ ] From the filtered street detections, extract all `car` and `truck` class detections
- [ ] For each detection, compute:
  - Center point in ENU coordinates
  - Orientation estimate (from the aspect ratio of the bounding box — wider = East-West, taller = North-South)
  - Approximate size (from the bounding box dimensions in meters)
- [ ] Save to `street_cleanup/parking_spots.parquet` with columns:
  - `east, north` (center position in ENU meters)
  - `width, length` (estimated vehicle/spot dimensions in meters)
  - `heading_deg` (estimated heading: 0=North, 90=East)
  - `source_octant_path` (which tile this came from)
- [ ] This data can later be loaded by the game to spawn parked cars or define parking spots

---

## Execution Order & Commands

Run these commands **in sequence** from the `_data/3d_data_v2/` directory:

```bash
# 0. Install dependencies
uv add shapely ultralytics trimesh opencv-python-headless
uv sync

# 1. Extract streets from OSM and find intersecting tiles
uv run python street_cleanup/extract_streets.py
uv run python street_cleanup/find_street_tiles.py

# 2. Render street tiles to top-down images  
uv run python street_cleanup/run_renders.py

# 3. Run YOLO object detection
uv run python street_cleanup/run_yolo.py

# 4. Filter detections to streets only
uv run python street_cleanup/filter_street_detections.py

# 5. Compute target heights and apply flattening
uv run python street_cleanup/apply_flattening.py

# 6. (Optional) Validate results
uv run python street_cleanup/run_renders.py --patched  # re-render patched tiles
uv run python street_cleanup/summarize.py

# 7. Install patches (only when satisfied with results)
uv run python street_cleanup/install_patches.py --dry-run  # preview
uv run python street_cleanup/install_patches.py             # apply
uv run rebuild_manifest.py                                   # rebuild manifest
```

---

## File Summary

| File | Purpose | Type |
|------|---------|------|
| `street_cleanup/coord_utils.py` | Coordinate conversion utilities (lat/lon ↔ ENU ↔ Bevy) | Library |
| `street_cleanup/extract_streets.py` | Parse OSM roads, create buffered road polygons | Script |
| `street_cleanup/find_street_tiles.py` | Find which tiles intersect with streets | Script |
| `street_cleanup/render_street_tiles.py` | Blender script: render tiles top-down orthographic | Blender Script |
| `street_cleanup/run_renders.py` | Orchestrate Blender rendering in batches | Script |
| `street_cleanup/run_yolo.py` | Run YOLO on rendered images, save detections | Script |
| `street_cleanup/filter_street_detections.py` | Filter detections to street areas only | Script |
| `street_cleanup/flatten_mesh.py` | Core mesh flattening logic using trimesh | Library |
| `street_cleanup/apply_flattening.py` | Apply flattening to all affected tiles | Script |
| `street_cleanup/install_patches.py` | Install patched GLBs (with backup & rollback) | Script |
| `street_cleanup/summarize.py` | Generate results summary report | Script |

---

## Open Questions

> [!IMPORTANT]
> **Q1: Should we also flatten sidewalks/curbs?** The current plan focuses on the road surface (carriageway). Sidewalks often have trees, benches, poles that create bumps. If the car physics only interacts with the road, we may want to extend the flattening to include sidewalk areas as well. What's your preference?

> [!IMPORTANT]
> **Q2: Blender vs trimesh for mesh modification?** The plan uses `trimesh` for programmatic mesh flattening (Phase 4). An alternative is to use Blender scripting (bpy) for mesh modification, which would be more consistent with the existing pipeline but slower. `trimesh` is faster and doesn't require a Blender process. Do you have a preference?

> [!IMPORTANT]  
> **Q3: Which YOLO model size?** Trade-off between speed and accuracy:
> - `yolo11n.pt` (nano): ~3.2M params, fastest, may miss small objects
> - `yolo11s.pt` (small): ~9.4M params, good balance
> - `yolo11m.pt` (medium): ~20M params, best accuracy for this use case
> 
> Recommendation: Start with `yolo11m.pt` since we're doing offline batch processing and accuracy matters more than speed.

> [!IMPORTANT]
> **Q4: Tree handling priority.** Trees are harder to detect than cars from top-down satellite views. Should we prioritize car/truck/bus removal first and tackle trees as a follow-up? Or should both be handled in the same pass?
