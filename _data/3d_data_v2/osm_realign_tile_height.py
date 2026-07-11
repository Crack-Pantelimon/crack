# /// script
# dependencies = [
#     "pyarrow",
#     "numpy",
# ]
# ///

"""
Realign OSM road heights onto photogrammetry tiles.

For each depth-20 tile in data_in/sample_tiles_with_cars.txt, builds a Blender
.blend containing the tile GLB terrain plus road centerlines draped onto the
surface via downward ray-casts.
"""

import json
import logging
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.dataset as ds

from octree import octant_path_to_bbox

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)],
)
logger = logging.getLogger("osm_realign_tile_height")

SAMPLE_TILES_FILE = Path("data_in/sample_tiles_with_cars.txt")
MANIFEST_PATH = Path("data_out/manifest.parquet")
OCTTREE_FEATURES_PATH = Path("data_osm/octtree_features.parquet")
FEATURES_PATH = Path("data_osm/features.parquet")
OUTPUT_BLEND_DIR = Path("data_out/_demo_tile")
BLENDER_SCRIPT = "_blend_realign_files.py"
ROOT_DEPTH = 10


def run_blender_batch(script: str, batch_json_path: str) -> str:
    """Run a Blender -P script over a batch JSON file; fatal on non-zero exit."""
    cmd = ["blender", "-b", "-P", script, "--", batch_json_path]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    output = (proc.stdout or "") + (proc.stderr or "")
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
) -> dict:
    tile_bbox = octant_path_to_bbox(tile)
    glb_path = manifest_row["glb_path"]
    out_blend_path = str(OUTPUT_BLEND_DIR / f"{tile}.blend")

    return {
        "octtree_path": tile,
        "road_source_path": road_source_path or "",
        "glb_path": glb_path,
        "out_blend_path": out_blend_path,
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


def main():
    tiles = load_sample_tiles()
    logger.info(f"Processing {len(tiles)} sample tiles from {SAMPLE_TILES_FILE}")

    manifest_dataset = ds.dataset(str(MANIFEST_PATH), format="parquet")
    octtree_dataset = ds.dataset(str(OCTTREE_FEATURES_PATH), format="parquet")
    features_dataset = ds.dataset(str(FEATURES_PATH), format="parquet")

    items: list[dict] = []
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

        item = build_work_item(tile, manifest_row, road_source_path, roads)
        items.append(item)

        source_depth = len(road_source_path) if road_source_path else 0
        summaries.append((tile, road_source_path or "(none)", source_depth, len(roads)))

    if not items:
        logger.error("No work items to process")
        sys.exit(1)

    batch = {"items": items}
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False, encoding="utf-8"
    ) as tmp:
        json.dump(batch, tmp)
        batch_path = tmp.name

    logger.info(f"Running Blender batch ({len(items)} items) via {BLENDER_SCRIPT}")
    try:
        output = run_blender_batch(BLENDER_SCRIPT, batch_path)
        if output.strip():
            logger.info(f"Blender output:\n{output.strip()}")
    finally:
        os.unlink(batch_path)

    logger.info("=" * 60)
    logger.info("Per-tile summary:")
    for tile, source, depth, road_count in summaries:
        blend_path = OUTPUT_BLEND_DIR / f"{tile}.blend"
        status = "written" if blend_path.is_file() else "MISSING"
        logger.info(
            f"  {tile}: roads={road_count}, source={source} (depth={depth}), "
            f"blend {status}"
        )
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
