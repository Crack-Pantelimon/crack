# Plan: OSM post-process batch with top-down car detection

## Context

Post-processing currently produces, per depth-20 tile, a `.blend` file containing the
photogrammetry GLB terrain with OSM road centerlines draped onto it (via
`osm_realign_tile_height.py` → Blender worker `_blend_realign_files.py`).

We want to additionally detect **cars** from a top-down render of each tile and mark them
in the `.blend`. The car-detection logic was pasted into `car_detection_fane/` by a weaker
model — most of it is dead code. We pick up only two code paths (top-down 128×128 Blender
render, and YOLOv7 ONNX inference producing per-tile detections) into clean new scripts in
`_data/3d_data_v2/`, and leave the rest of `car_detection_fane/` to delete later.

New per-tile pipeline (all intermediary files live alongside the `.blend` in
`data_out/_demo_tile/`):
1. **Render** GLB top-down → `<tile>_render.jpg` (+ `<tile>_render.json` meta).
2. **Detect** cars with YOLOv7 ONNX → `<tile>_cars.json` (car bboxes in lat/lon).
3. **Build** `.blend`: drape roads (current behavior) **and** mark each car as an
   apex-raised pyramid projected onto the terrain.

Decisions confirmed with user: `osm_postprocess_batch.py` is a **uv Python orchestrator**;
car markers are **apex-raised pyramids**; scope is **cars only** (the itcvd ONNX is a
vehicle detector).

## What to pick up from `car_detection_fane/` (leave the rest)

- `render_top_down.py` — the Blender top-down ortho render logic. **Copy** → new
  `_blend_render_topdown.py`. Uses only `bpy`/`mathutils`; already writes the meta we need
  (`resolution`, `ortho_scale`, `camera_location`, `bbox_xyz`, `lat_lon_bbox`).
- The clean ONNX inference in `generate_detections_parquet.py::run_yolov7_onnx` (blob →
  forward → NMS) — **reimplement** in new `yolo_v7_sat.py`. (Prefer this over `demo.py`'s
  inline copy.)
- `yolov7-m_itcvd_qgis.onnx` (24 MB) — **copy** → `yolo_models/yolov7-m_itcvd_qgis.onnx`
  and commit (confirmed not git-ignored).
- Ignore/leave for later deletion: `demo.py`, `yolo_detect.py` (Ultralytics multi-class),
  `road_tiles.py`, `coord_utils.py`, all `run_*`/`sample_*`/`filter_*` scripts,
  `detections.parquet`, the `.pt`-based paths, and the tree/OSM projection.

## Files to change / create

### 1. `osm_realign_tile_height.py` → `osm_postprocess_batch.py` (git mv + extend)
Keep the existing batch-building logic (sample-tile rows, manifest lookup,
`find_roads_for_tile`, `build_work_item`). Extend `main()` to compute per-tile sidecar
paths next to the blend and run three stages:

- Per tile add paths: `render_jpg = OUTPUT_BLEND_DIR/<tile>_render.jpg`,
  `render_meta = OUTPUT_BLEND_DIR/<tile>_render.json`,
  `cars_json = OUTPUT_BLEND_DIR/<tile>_cars.json` (blend stays `<tile>.blend`).
- **Stage 1 – render:** write a render batch `{"tiles":[{octant_path, glb_path,
  jpg_path=render_jpg, meta_path=render_meta, lat_lon_bbox{lat_south,lat_north,lon_west,
  lon_east}, resolution:[128,128]} …]}` and run
  `run_blender_batch("_blend_render_topdown.py", …)` (reuse existing helper).
- **Stage 2 – detect + project:** `net = yolo_v7_sat.load_net()`; for each tile
  `img = cv2.imread(render_jpg)`, `dets = yolo_v7_sat.detect_cars(net, img)`; load
  `render_meta`; project each pixel bbox to lat/lon (helper below); write
  `<tile>_cars.json`: `{octant_path, resolution, cars:[{bbox_pixel,[x1,y1,x2,y2],
  confidence, corners_latlon:[[lat,lon]×4], center_latlon:[lat,lon]} …]}`.
- **Stage 3 – blend:** extend each work item with `"cars_json"` and run
  `run_blender_batch("_blend_build_map.py", …)`.

**Pixel→lat/lon helper** (add to this file; ortho-aware, avoids the pasted code's bug of
assuming the image spans the tile bbox — the ortho camera frames the *mesh* bbox with 1.05
padding). Consistent inverse of `latlon_to_xy` used in the blend worker:
```
W,H = meta["resolution"]; cx,cy = meta["camera_location"][:2]; osc = meta["ortho_scale"]
sx = osc*(W/max(W,H)); sy = osc*(H/max(W,H))          # square render → sx=sy=osc
wx = cx + (px/W - 0.5)*sx;  wy = cy + (0.5 - py/H)*sy  # image y-down → world y-up (north)
m = meta["bbox_xyz"]; fx=(wx-m["min"][0])/(m["max"][0]-m["min"][0]); fy=(wy-m["min"][1])/(m["max"][1]-m["min"][1])
b = meta["lat_lon_bbox"]; lon=b["lon_west"]+fx*(b["lon_east"]-b["lon_west"]); lat=b["lat_south"]+fy*(b["lat_north"]-b["lat_south"])
```
Do **not** rely on `coord_utils` REF-based ENU — the GLB frame ≠ Cora-Pantelimon ENU
(camera_location north ≈ 21 km, so `enu_to_latlon` would be wrong). The mesh-bbox↔geo-bbox
affine is the per-tile mapping the blend worker already uses.

### 2. `_blend_realign_files.py` → `_blend_build_map.py` (git mv + add cars)
Keep all road-draping code. Add car pyramids in `process_item`:
- After roads, `cars_json = item.get("cars_json")`; if the file exists, load it.
- For each car: map `corners_latlon` (4) + `center_latlon` via existing `latlon_to_xy`
  (item `latlon_bbox` uses keys `north/south/west/east`), raycast z for each with existing
  `raycast_height`, resolve misses to the terrain top.
- Build a pyramid mesh: 4 base verts = corners at terrain z; 1 apex vert = center (x,y) at
  `terrain_z + APEX_OFFSET` (constant, e.g. `3.0` m). Faces = base quad + 4 side tris. Link
  into a new `cars` collection (mirror `get_or_create_collection`/`create_road_object`).
- Guard when there is no `cars_json` or zero cars (roads-only tiles still save).

### 3. `yolo_v7_sat.py` (new, uv)
- `DEFAULT_ONNX = Path(__file__).parent/"yolo_models/yolov7-m_itcvd_qgis.onnx"`.
- `load_net(onnx_path=DEFAULT_ONNX)` → `cv2.dnn.readNetFromONNX`.
- `detect_cars(net, image_bgr, conf=0.20, nms=0.4)` → `[{"bbox_pixel":[x1,y1,x2,y2],
  "confidence":float}]`. Port `run_yolov7_onnx`: 640×640 blob (`1/255`, `swapRB`), forward,
  `obj_conf*class_prob ≥ conf`, scale to image size, `cv2.dnn.NMSBoxes`.

### 4. `_blend_render_topdown.py` (new, Blender worker)
Copy `car_detection_fane/render_top_down.py` essentially verbatim (`RENDER_SIZE = 128`,
same batch/meta contract). No `street_cleanup` imports — self-contained on `bpy`.

### 5. `yolo_models/yolov7-m_itcvd_qgis.onnx` (new, committed)
`cp car_detection_fane/yolov7-m_itcvd_qgis.onnx yolo_models/`.

### 6. `pyproject.toml`
Add `opencv-python-headless` (cv2 currently only transitively present) so `yolo_v7_sat`
has a declared dependency.

## Verification
Run the whole pipeline on the 4 sample tiles:
```
cd _data/3d_data_v2 && PYTHONPATH=. uv run osm_postprocess_batch.py
```
Confirm end-to-end:
- For each sample tile in `data_out/_demo_tile/`: `<tile>_render.jpg`, `<tile>_render.json`,
  `<tile>_cars.json`, `<tile>.blend` all exist.
- Spot-check a `<tile>_cars.json`: every `center_latlon` falls inside that tile's
  `octant_path_to_bbox` bounds (sanity of the projection).
- `yolo_v7_sat` unit-smoke: `detect_cars(load_net(), cv2.imread(<tile>_render.jpg))` returns
  a bbox list without error.
- Open one `.blend` headless to confirm a `cars` collection with pyramid objects exists,
  e.g. `blender -b <tile>.blend -P -` printing
  `bpy.data.collections['cars']` object count > 0 for a tile with detections.
- `git status` shows `yolo_models/yolov7-m_itcvd_qgis.onnx` tracked (not ignored).

## Notes / assumptions
- Render is square 128×128, so ortho x/y scale are equal; the helper handles non-square
  defensively anyway.
- `car_detection_fane/` is left intact for later deletion; nothing new depends on it.
- The auto-generated `.github/context-*.md` signature files are left to their tooling.
