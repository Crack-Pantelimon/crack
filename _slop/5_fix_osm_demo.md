# Realign OSM road heights onto photogrammetry tiles

## Context

The pipeline in `_data/3d_data_v2/` downloads Google Earth photogrammetry octree
tiles as `.glb` files (`main.py` → `build_blend.py`) and cross-references OSM
features against those tiles (`osm_download.py` → `data_osm/features.parquet` +
`data_osm/octtree_features.parquet`). We now want to see OSM roads *draped onto the
actual 3D terrain* of specific tiles so we can judge whether road geometry can be
snapped to the photogrammetry surface (a precursor to placing cars on roads).

`data_in/sample_tiles_with_cars.txt` lists depth‑20 octant paths (tiles where cars
were detected). For each, we build a Blender `.blend` that contains the tile's GLB
terrain plus road centerlines projected straight down onto that terrain, saved to
`data_out/_demo_tile/<tile>.blend` for manual inspection.

Two new scripts, mirroring the existing orchestrator→Blender-subprocess pattern:
- `osm_realign_tile_height.py` — orchestrator (pyarrow point queries + batch JSON).
- `_blend_realign_files.py` — Blender script that loads the GLB, projects roads, saves the `.blend`.

## Key facts established during exploration

- **GLB path formula** (inline in `main.py:156-158`): `data_out/{len(id)}/{id[-3:]}/{id}.glb`.
  All 4 sample tiles have exact depth‑20 GLBs. The tile loaded/saved is **always** the
  exact sample‑file id (per user); roads fall back up the parent chain only.
- **lat/lon bbox** from `octree.octant_path_to_bbox(path)` (`octree.py:132`); also stored
  per tile in `data_out/manifest.parquet`.
- **xyz bbox** per tile is in `data_out/manifest.parquet` columns
  `x_min,y_min,z_min,x_max,y_max,z_max`. These come from `rebuild_manifest.get_glb_stats`
  which reads the **raw glTF POSITION accessor**, so they are in **glTF axes**
  (X=East, Y=Up, Z=−North), *not* Blender axes. Confirmed by the value ranges on a sample
  tile (y≈3356–3364 = height; z≈−21049…−21010 = north; x≈44–72 = east).
- **Blender re-import**: `bpy.ops.import_scene.gltf` converts glTF Y-up back to Blender
  Z-up, so in the imported scene coords are ENU (x=East, y=North, z=Up) — the frame we
  ray-cast in. To avoid axis-conversion bugs, `_blend_realign_files.py` will **measure the
  imported terrain's bounding box directly in Blender coords** and use that for mapping;
  the manifest xyz bbox is still written into the JSON (per the user's spec) for reference.
- **Blender invocation** (reuse `main.py:43` pattern): `blender -b -P <script> -- <batch.json>`;
  script finds `--` in `sys.argv`. `blender` (v5.1.2) is on PATH.
- **octtree_features join**: filter `octtree_path == <path>` → `(collection_name, feature_type,
  feature_id)`; join `feature_id` back to `features.parquet` for the full GeoJSON in
  `feature_json`. Road = `collection_name=='roads'` AND `feature_type=='way'` AND
  `properties.tags.lanes` present. Road geometry is a `LineString` with
  `geometry.coordinates=[[lon,lat],…]` parallel to `properties.nodes=[id,…]`.

## File 1 — `osm_realign_tile_height.py` (orchestrator)

PEP‑723 header (deps: `pyarrow`, `numpy`), run via `uv run osm_realign_tile_height.py`,
cwd = `_data/3d_data_v2`. Reuse `from octree import octant_path_to_bbox`.

Open parquet files **without loading into RAM** — use `pyarrow.dataset.dataset(...)` and
issue filtered `to_table` point queries (do not `read()` the whole file):
- `data_osm/octtree_features.parquet`
- `data_osm/features.parquet`
- `data_out/manifest.parquet`

For each tile id in `data_in/sample_tiles_with_cars.txt`:
1. **Manifest lookup** (filter `octant_path == tile`): get `glb_path`, lat/lon bbox,
   and xyz bbox. Skip tile with a warning if absent or GLB file missing on disk.
2. **Find road-source path**: starting `candidate = tile`, walk down `candidate[:-1]`
   until `len(candidate) >= 10` (root depth):
   - filter `octtree_features` by `octtree_path == candidate`,
     `collection_name=='roads'`, `feature_type=='way'` → set of `feature_id`s.
   - filter `features` by those ids (+ same collection/type), `json.loads` each,
     keep ways whose `properties.tags` has `lanes`.
   - if ≥1 qualifying road: use them, record `road_source_path = candidate`, stop.
   - else go to the parent. If none found up to root: emit item with empty roads
     (terrain-only blend) and log it.
3. **Trim geometry** to `road_source_bbox = octant_path_to_bbox(road_source_path)`
   (half-open `s<=lat<n and w<=lon<e`, matching `osm_download.collect_octant_paths`):
   for each road, keep coordinate index `i` if node `i` is inside the bbox **or** its
   immediate chain neighbor (`i-1` or `i+1`) is inside; drop roads left with no kept nodes.
   Replace the feature's `geometry.coordinates` (and `properties.nodes`) with the kept
   subset so the emitted `feature_json` already contains only the trimmed geometry.
4. **Build work item**:
   ```json
   {
     "octtree_path": "<tile, depth 20>",
     "road_source_path": "<tile or ancestor>",
     "glb_path": "data_out/20/633/<tile>.glb",
     "out_blend_path": "data_out/_demo_tile/<tile>.blend",
     "latlon_bbox": {"north":…, "south":…, "west":…, "east":…},   // sample tile
     "xyz_bbox_gltf": {"x_min":…,…,"z_max":…},                    // manifest, glTF axes (reference)
     "roads": [ { "feature": <trimmed GeoJSON feature> }, … ]
   }
   ```
5. Write all items to one temp JSON (`{"items": [...]}`) via `tempfile.NamedTemporaryFile`,
   run `blender -b -P _blend_realign_files.py -- <tmp>` (reuse a `run_blender_batch`-style
   wrapper; treat non-zero return code as fatal, per-item failures reported in stdout).
6. Log a per-tile summary (roads kept, source depth, blend written).

## File 2 — `_blend_realign_files.py` (Blender script)

Parse `--` arg → batch JSON. For each item:
1. `bpy.ops.wm.read_factory_settings(use_empty=True)` (clear scene, as `build_blend.py:284`).
2. `bpy.ops.import_scene.gltf(filepath=os.path.abspath(glb_path))` (default settings → ENU).
3. **Measure terrain bbox in Blender coords**: iterate imported mesh objects, transform
   `bound_box`/vertices by `obj.matrix_world`, get `(x,y,z)` min/max → `east=[xmin,xmax]`,
   `north=[ymin,ymax]`, `up=[zmin,zmax]`. `top = up_max` ("max height of the map bbox").
4. **Map each road node lat/lon → Blender (x,y)** by bilinear extrapolation between
   `latlon_bbox` and the measured terrain bbox (values outside [0,1] extrapolate, which is
   correct for nodes beyond the tile since the local ENU frame is linear):
   `fx=(lon-west)/(east-west); x=east_min+fx*(east_max-east_min)`;
   `fy=(lat-south)/(north-south); y=north_min+fy*(north_max-north_min)`.
5. **Project down**: `depsgraph=bpy.context.evaluated_depsgraph_get()`; for each node
   `hit,loc,*_ = bpy.context.scene.ray_cast(depsgraph, (x,y,top+ε), (0,0,-1))`.
   Record `loc.z` on hit, else `None`.
6. **Resolve misses** per road: if a node missed, take the height of its nearest chain
   neighbor that hit (walk out from the node). If the **whole road has zero hits**, drop the
   entire road. (Matches "same height as immediate neighbor … if no neighbor, drop the road.")
7. **Create road object**: a mesh polyline (verts at resolved `(x,y,z)`, edges between
   consecutive nodes), named `road_<feature_id>`, linked into a `roads` collection.
8. `os.makedirs(dirname(out_blend_path))`; `bpy.ops.wm.save_as_mainfile(filepath=abs(out_blend_path))`.
9. Print `REALIGN_OK <tile>` / `REALIGN_FAIL <tile>: <err>` per item (per-item try/except so
   one failure doesn't abort the batch), like `build_blend.py`.

## Critical files
- Create: `_data/3d_data_v2/osm_realign_tile_height.py`, `_data/3d_data_v2/_blend_realign_files.py`
- Reuse: `octree.octant_path_to_bbox` (`octree.py:132`), GLB-path formula (`main.py:156-158`),
  Blender-subprocess pattern (`main.py:43`), scene-clear/arg-parse patterns (`build_blend.py:284,441`),
  half-open containment (`osm_download.py:346-349`).
- Inputs: `data_in/sample_tiles_with_cars.txt`, `data_osm/{features,octtree_features}.parquet`,
  `data_out/manifest.parquet`, `data_out/20/**/<tile>.glb`.

## Verification
1. `cd _data/3d_data_v2 && uv run osm_realign_tile_height.py` — expect 4 `.blend` files in
   `data_out/_demo_tile/` and per-tile logs (roads kept, source depth).
2. Confirm files exist and are non-trivial: `ls -la data_out/_demo_tile/`.
3. Sanity-open one in Blender headless to assert it loads and contains a terrain mesh plus
   `road_*` objects:
   `blender -b data_out/_demo_tile/<tile>.blend --python-expr "import bpy;print([o.name for o in bpy.data.objects])"`.
4. Spot-check a road: its polyline z-values should sit within the terrain's z range
   (draped on the surface, not at the flat `top`), and (x,y) within/near the terrain bbox.
5. Manually open a `.blend` in the GUI to eyeball road drape quality (the intended deliverable).
