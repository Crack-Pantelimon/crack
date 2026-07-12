# /// script
# dependencies = [
#     "pyarrow",
#     "numpy",
#     "opencv-python-headless",
# ]
# ///

"""
OSM post-process batch: top-down car detection + road-draped .blend tiles.

For each depth-20 tile in data_in/sample_tiles_with_cars.txt:
1. Render GLB top-down (128×128) → <tile>_render.jpg + meta JSON
2. Detect cars with YOLOv7 ONNX → <tile>_cars.json
3. Build .blend with draped roads and apex-raised car pyramids
"""

import json
import logging
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import cv2
import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.dataset as ds

from octree import octant_path_to_bbox
import yolo_v8_obb_sat

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)],
)
logger = logging.getLogger("osm_postprocess_batch")

SAMPLE_TILES_FILE = Path("data_in/sample_tiles_with_cars.txt")
MANIFEST_PATH = Path("data_out/manifest.parquet")
OCTTREE_FEATURES_PATH = Path("data_osm/octtree_features.parquet")
FEATURES_PATH = Path("data_osm/features.parquet")
OUTPUT_BLEND_DIR = Path("data_out/_demo_tile")
RENDER_SCRIPT = "_blend_render_topdown.py"
BLEND_SCRIPT = "_blend_build_map.py"
RENDER_RESOLUTION = [288, 288]
ROOT_DEPTH = 10


def run_blender_batch(script: str, batch_json_path: str) -> str:
    """
    Run a Blender -P script over a batch JSON file, streaming its output live as it's
    produced (rather than buffering until exit) so progress is visible even if the
    process later crashes/segfaults without a chance to flush a final report.
    """
    cmd = ["blender", "-b", "-P", script, "--", batch_json_path]
    env = {**os.environ, "PYTHONUNBUFFERED": "1"}
    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
        env=env,
    )
    lines = []
    for line in proc.stdout:
        line = line.rstrip("\n")
        lines.append(line)
        logger.info(f"[blender] {line}")
    proc.wait()
    output = "\n".join(lines)
    if proc.returncode != 0:
        raise RuntimeError(
            f"Blender script {script} crashed (returncode={proc.returncode}).\n"
            f"---- blender output ----\n{output.strip()}\n------------------------"
        )
    return output


def glb_path_for_tile(tile: str) -> Path:
    """Return the on-disk GLB path for an octant path (matches main.py)."""
    depth = len(tile)
    last_three = tile[-3:] if depth >= 3 else tile
    return Path("data_out") / str(depth) / last_three / f"{tile}.glb"


def tile_sidecar_paths(tile: str) -> dict[str, Path]:
    base = OUTPUT_BLEND_DIR / tile
    return {
        "render_jpg": Path(f"{base}_render.jpg"),
        "render_meta": Path(f"{base}_render.json"),
        "cars_json": Path(f"{base}_cars.json"),
        "blend": Path(f"{base}.blend"),
    }


def pixel_to_latlon(px: float, py: float, meta: dict) -> tuple[float, float]:
    """Map render pixel to lat/lon using ortho camera + mesh-bbox affine."""
    w, h = meta["resolution"]
    cx, cy = meta["camera_location"][:2]
    osc = meta["ortho_scale"]
    sx = osc * (w / max(w, h))
    sy = osc * (h / max(w, h))

    wx = cx + (px / w - 0.5) * sx
    wy = cy + (0.5 - py / h) * sy

    m = meta["bbox_xyz"]
    x_span = m["max"][0] - m["min"][0]
    y_span = m["max"][1] - m["min"][1]
    fx = (wx - m["min"][0]) / x_span if x_span else 0.5
    fy = (wy - m["min"][1]) / y_span if y_span else 0.5

    b = meta["lat_lon_bbox"]
    lon = b["lon_west"] + fx * (b["lon_east"] - b["lon_west"])
    lat = b["lat_south"] + fy * (b["lat_north"] - b["lat_south"])
    return lat, lon


def obb_pixel_to_latlon_corners(
    corners_pixel: list[list[float]], meta: dict
) -> tuple[list[list[float]], list[float]]:
    corners = [list(pixel_to_latlon(px, py, meta)) for px, py in corners_pixel]
    cx = sum(p[0] for p in corners_pixel) / len(corners_pixel)
    cy = sum(p[1] for p in corners_pixel) / len(corners_pixel)
    center = list(pixel_to_latlon(cx, cy, meta))
    return corners, center


def node_inside_bbox(lon: float, lat: float, bbox) -> bool:
    """Half-open containment: south <= lat < north, west <= lon < east."""
    return bbox.south <= lat < bbox.north and bbox.west <= lon < bbox.east


def trim_road_feature(feature: dict, bbox) -> dict | None:
    """
    Keep coordinate indices inside bbox or adjacent to an inside node.
    Returns a trimmed copy, or None if no nodes remain.
    """
    geometry = feature.get("geometry")
    if not geometry or geometry.get("type") != "LineString":
        return None

    coords = geometry.get("coordinates", [])
    props = feature.get("properties", {})
    nodes = props.get("nodes", [])

    if len(coords) < 2:
        return None

    inside = [
        node_inside_bbox(lon, lat, bbox)
        for lon, lat in coords
    ]

    keep = []
    for i in range(len(coords)):
        if inside[i]:
            keep.append(i)
        elif i > 0 and inside[i - 1]:
            keep.append(i)
        elif i + 1 < len(coords) and inside[i + 1]:
            keep.append(i)

    if not keep:
        return None

    trimmed = json.loads(json.dumps(feature))
    trimmed["geometry"]["coordinates"] = [coords[i] for i in keep]
    if nodes and len(nodes) == len(coords):
        trimmed["properties"]["nodes"] = [nodes[i] for i in keep]
    return trimmed


def has_lanes(feature: dict) -> bool:
    tags = feature.get("properties", {}).get("tags", {})
    return "lanes" in tags


def lookup_manifest_row(manifest_dataset, tile: str) -> dict | None:
    table = manifest_dataset.to_table(
        filter=pc.field("octant_path") == tile,
    )
    if table.num_rows == 0:
        return None
    return table.to_pylist()[0]


def query_road_feature_ids(octtree_dataset, candidate: str) -> list:
    table = octtree_dataset.to_table(
        filter=(
            (pc.field("octtree_path") == candidate)
            & (pc.field("collection_name") == "roads")
            & (pc.field("feature_type") == "way")
        ),
        columns=["feature_id"],
    )
    if table.num_rows == 0:
        return []
    return table.column("feature_id").to_pylist()


def load_road_features(features_dataset, feature_ids: list[int]) -> list[dict]:
    if not feature_ids:
        return []

    table = features_dataset.to_table(
        filter=(
            pc.is_in(pc.field("feature_id"), pa.array(feature_ids))
            & (pc.field("collection_name") == "roads")
            & (pc.field("feature_type") == "way")
        ),
        columns=["feature_json"],
    )
    roads = []
    for row in table.to_pylist():
        feature = json.loads(row["feature_json"])
        if has_lanes(feature):
            roads.append(feature)
    return roads


def find_roads_for_tile(
    tile: str,
    octtree_dataset,
    features_dataset,
) -> tuple[str | None, list[dict]]:
    """
    Walk up the parent chain from tile until qualifying roads are found.
    Returns (road_source_path, trimmed_road_features).
    """
    candidate = tile
    while len(candidate) >= ROOT_DEPTH:
        feature_ids = query_road_feature_ids(octtree_dataset, candidate)
        roads = load_road_features(features_dataset, feature_ids)
        if roads:
            road_bbox = octant_path_to_bbox(candidate)
            trimmed = []
            for feature in roads:
                kept = trim_road_feature(feature, road_bbox)
                if kept is not None:
                    trimmed.append(kept)
            if trimmed:
                return candidate, trimmed
        if len(candidate) == ROOT_DEPTH:
            break
        candidate = candidate[:-1]
    return None, []


def build_work_item(
    tile: str,
    manifest_row: dict,
    road_source_path: str | None,
    roads: list[dict],
    sidecars: dict[str, Path],
) -> dict:
    tile_bbox = octant_path_to_bbox(tile)
    glb_path = manifest_row["glb_path"]

    return {
        "octtree_path": tile,
        "road_source_path": road_source_path or "",
        "glb_path": glb_path,
        "out_blend_path": str(sidecars["blend"]),
        "cars_json": str(sidecars["cars_json"]),
        "latlon_bbox": {
            "north": tile_bbox.north,
            "south": tile_bbox.south,
            "west": tile_bbox.west,
            "east": tile_bbox.east,
        },
        "xyz_bbox_gltf": {
            "x_min": manifest_row["x_min"],
            "y_min": manifest_row["y_min"],
            "z_min": manifest_row["z_min"],
            "x_max": manifest_row["x_max"],
            "y_max": manifest_row["y_max"],
            "z_max": manifest_row["z_max"],
        },
        "roads": [{"feature": road} for road in roads],
    }


def load_sample_tiles() -> list[str]:
    lines = SAMPLE_TILES_FILE.read_text(encoding="utf-8").splitlines()
    return [line.strip() for line in lines if line.strip()]


def run_render_stage(tile_specs: list[dict]) -> None:
    batch = {"tiles": tile_specs}
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False, encoding="utf-8"
    ) as tmp:
        json.dump(batch, tmp)
        batch_path = tmp.name

    logger.info(f"Stage 1 – render: {len(tile_specs)} tiles via {RENDER_SCRIPT}")
    try:
        output = run_blender_batch(RENDER_SCRIPT, batch_path)
        if output.strip():
            logger.info(f"Render output:\n{output.strip()}")
    finally:
        os.unlink(batch_path)


def run_detect_stage(tile_records: list[dict], net) -> None:
    logger.info(f"Stage 2 – detect: {len(tile_records)} tiles")
    for rec in tile_records:
        tile = rec["tile"]
        sidecars = rec["sidecars"]
        render_jpg = sidecars["render_jpg"]
        render_meta = sidecars["render_meta"]
        cars_json = sidecars["cars_json"]

        if not render_jpg.is_file():
            logger.warning(f"{tile}: render missing at {render_jpg}, skipping detection")
            cars_payload = {
                "octant_path": tile,
                "resolution": RENDER_RESOLUTION,
                "cars": [],
            }
            cars_json.write_text(json.dumps(cars_payload, indent=2), encoding="utf-8")
            continue

        img = cv2.imread(str(render_jpg))
        if img is None:
            logger.warning(f"{tile}: failed to read {render_jpg}")
            continue

        dets = yolo_v8_obb_sat.detect_cars(net, img)
        with open(render_meta, encoding="utf-8") as f:
            meta = json.load(f)

        cars = []
        for det in dets:
            corners, center = obb_pixel_to_latlon_corners(det["corners_pixel"], meta)
            cars.append(
                {
                    "bbox_pixel": det["bbox_pixel"],
                    "angle_deg": det["angle_deg"],
                    "class_name": det["class_name"],
                    "confidence": det["confidence"],
                    "corners_latlon": corners,
                    "center_latlon": center,
                }
            )

        payload = {
            "octant_path": tile,
            "resolution": meta.get("resolution", RENDER_RESOLUTION),
            "cars": cars,
        }
        cars_json.write_text(json.dumps(payload, indent=2), encoding="utf-8")
        logger.info(f"{tile}: {len(cars)} car(s) detected → {cars_json}")


def run_blend_stage(items: list[dict]) -> None:
    batch = {"items": items}
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False, encoding="utf-8"
    ) as tmp:
        json.dump(batch, tmp)
        batch_path = tmp.name

    logger.info(f"Stage 3 – blend: {len(items)} items via {BLEND_SCRIPT}")
    try:
        output = run_blender_batch(BLEND_SCRIPT, batch_path)
        if output.strip():
            logger.info(f"Blend output:\n{output.strip()}")
    finally:
        os.unlink(batch_path)


def main():
    tiles = load_sample_tiles()
    logger.info(f"Processing {len(tiles)} sample tiles from {SAMPLE_TILES_FILE}")

    manifest_dataset = ds.dataset(str(MANIFEST_PATH), format="parquet")
    octtree_dataset = ds.dataset(str(OCTTREE_FEATURES_PATH), format="parquet")
    features_dataset = ds.dataset(str(FEATURES_PATH), format="parquet")

    items: list[dict] = []
    tile_specs: list[dict] = []
    tile_records: list[dict] = []
    summaries: list[tuple[str, str, int, int]] = []

    for tile in tiles:
        manifest_row = lookup_manifest_row(manifest_dataset, tile)
        if manifest_row is None:
            logger.warning(f"Skipping {tile}: not found in manifest")
            continue

        glb_path = Path(manifest_row["glb_path"])
        if not glb_path.is_file():
            expected = glb_path_for_tile(tile)
            if expected.is_file():
                glb_path = expected
                manifest_row = dict(manifest_row)
                manifest_row["glb_path"] = str(expected)
            else:
                logger.warning(f"Skipping {tile}: GLB missing at {glb_path}")
                continue

        road_source_path, roads = find_roads_for_tile(
            tile, octtree_dataset, features_dataset
        )
        if not roads:
            logger.warning(
                f"{tile}: no qualifying roads found up parent chain; "
                "emitting terrain-only blend"
            )

        tile_bbox = octant_path_to_bbox(tile)
        sidecars = tile_sidecar_paths(tile)
        OUTPUT_BLEND_DIR.mkdir(parents=True, exist_ok=True)

        tile_specs.append(
            {
                "octant_path": tile,
                "glb_path": str(glb_path),
                "jpg_path": str(sidecars["render_jpg"]),
                "meta_path": str(sidecars["render_meta"]),
                "lat_lon_bbox": {
                    "lat_south": tile_bbox.south,
                    "lat_north": tile_bbox.north,
                    "lon_west": tile_bbox.west,
                    "lon_east": tile_bbox.east,
                },
                "resolution": RENDER_RESOLUTION,
            }
        )
        tile_records.append({"tile": tile, "sidecars": sidecars})
        items.append(
            build_work_item(tile, manifest_row, road_source_path, roads, sidecars)
        )

        source_depth = len(road_source_path) if road_source_path else 0
        summaries.append((tile, road_source_path or "(none)", source_depth, len(roads)))

    if not items:
        logger.error("No work items to process")
        sys.exit(1)

    run_render_stage(tile_specs)

    net = yolo_v8_obb_sat.load_net()
    run_detect_stage(tile_records, net)

    run_blend_stage(items)

    logger.info("=" * 60)
    logger.info("Per-tile summary:")
    for tile, source, depth, road_count in summaries:
        sidecars = tile_sidecar_paths(tile)
        blend_ok = sidecars["blend"].is_file()
        render_ok = sidecars["render_jpg"].is_file()
        cars_ok = sidecars["cars_json"].is_file()
        status = "written" if blend_ok else "MISSING"
        logger.info(
            f"  {tile}: roads={road_count}, source={source} (depth={depth}), "
            f"render={'ok' if render_ok else 'MISSING'}, "
            f"cars_json={'ok' if cars_ok else 'MISSING'}, "
            f"blend {status}"
        )
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
