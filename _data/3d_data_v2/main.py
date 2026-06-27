"""
Google Earth 3D Tile Downloader → GLB Exporter

Downloads 3D photogrammetry tiles from Google Earth's octree for a given
lat/lon bounding box and exports them as .glb files with a JSON manifest.
"""

import os
import sys
import time
import math
import logging
import numpy as np
from pathlib import Path
import subprocess
from concurrent.futures import ThreadPoolExecutor, as_completed

from octree import (
    parse_bbox,
    compute_best_level,
    enumerate_octants_in_bbox,
    tile_grid_dimensions,
    octant_path_to_bbox,
)
from earth_client import (
    fetch_planetoid_metadata,
    resolve_node,
    download_node,
    find_tiles_in_bbox,
)
from mesh_decoder import decode_node
import hashlib
from manifest import write_manifest

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("main")


def render_tile_via_blender(blend_path: Path, jpg_path: Path, ref_point: np.ndarray):
    """Render a blend file using Blender script in Cycles CPU mode."""
    cmd = [
        "blender",
        "-b",
        "-P",
        "render_tile.py",
        "--",
        str(blend_path),
        str(jpg_path),
        str(ref_point[0]),
        str(ref_point[1]),
        str(ref_point[2]),
    ]
    try:
        subprocess.run(
            cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )
    except Exception as e:
        logger.warning(f"Blender rendering failed for {blend_path.name}: {e}")


# Configuration
BBOX_FILE = "data_in/zone-bbox.txt"
OUTPUT_DIR = "data_out"
TARGET_GRID = 5  # aim for roughly 3x3 tiles
REQUEST_DELAY = 0.1  # seconds between node downloads
GET_ALL_COARSER_LEVELS = True  # If True, download all levels of detail smaller than (coarser than or equal to) the 3x3 optimal level


def save_tile(
    octant_path: str,
    node_data,
    decoded_meshes,
    output_dir: str,
) -> dict:
    """
    Return metadata for the manifest.
    """
    depth = len(octant_path)
    filename = f"{depth}/{octant_path}.blend"
    filepath = Path(output_dir) / filename

    # Compute stats
    total_verts = sum(len(m.positions) for m in decoded_meshes)
    total_tris = sum(len(m.indices) // 3 for m in decoded_meshes)

    # Read file size from disk
    file_size_bytes = 0
    if filepath.exists():
        file_size_bytes = filepath.stat().st_size

    # Get tile bbox
    tile_bbox = octant_path_to_bbox(octant_path)

    metadata = {
        "octant_path": octant_path,
        "filename": filename,
        "file_size_bytes": file_size_bytes,
        "mesh_count": len(decoded_meshes),
        "vertex_count": total_verts,
        "triangle_count": total_tris,
        "matrix_globe_from_mesh": list(node_data.matrix_globe_from_mesh),
        "bbox_latlon": {
            "n": tile_bbox.north,
            "s": tile_bbox.south,
            "w": tile_bbox.west,
            "e": tile_bbox.east,
        },
    }

    return metadata


def compute_reference_point(bbox) -> np.ndarray:
    """
    Compute ECEF reference point from the bounding box center.
    This ensures the reference point is constant and congruent for all runs and levels.
    """
    lat_deg = (bbox.north + bbox.south) / 2.0
    lon_deg = (bbox.east + bbox.west) / 2.0
    
    # Convert to ECEF (WGS84 ellipsoid)
    lat = math.radians(lat_deg)
    lon = math.radians(lon_deg)
    a = 6378137.0
    e2 = 0.00669437999014
    N = a / math.sqrt(1.0 - e2 * math.sin(lat)**2)
    x = N * math.cos(lat) * math.cos(lon)
    y = N * math.cos(lat) * math.sin(lon)
    z = N * (1.0 - e2) * math.sin(lat)
    return np.array([x, y, z])


def main():
    """Main pipeline: parse bbox → compute level → download → export GLB → manifest."""

    # 1. Parse bounding box
    logger.info(f"Parsing bounding box from {BBOX_FILE}")
    bbox = parse_bbox(BBOX_FILE)
    logger.info(f"BBox: N={bbox.north}, S={bbox.south}, W={bbox.west}, E={bbox.east}")
    logger.info(f"Span: {bbox.lat_span:.6f}° lat × {bbox.lon_span:.6f}° lon")

    # Fetch planetoid metadata (root epoch)
    logger.info("Fetching PlanetoidMetadata...")
    planetoid = fetch_planetoid_metadata()
    root_epoch = planetoid.root_node_metadata.epoch
    logger.info(f"Root epoch: {root_epoch}")

    # 2. Compute target level (designated small tile size)
    target_level = compute_best_level(bbox, TARGET_GRID)
    logger.info(f"Target LOD level (designated small tile size): {target_level}")

    octant_paths = []
    # Fetch all intersecting tiles with a depth >= 10 until we reach target_level
    for lvl in range(10, target_level + 1):
        tiles = find_tiles_in_bbox(bbox, lvl, root_epoch)
        octant_paths.extend(tiles)
        logger.info(f"Level {lvl}: found {len(tiles)} intersecting tiles")

    logger.info(f"Total tiles selected across all levels: {len(octant_paths)}")

    # 3. Compute reference point (ECEF offset)
    logger.info("Computing reference point from bounding box center...")
    ref_point = compute_reference_point(bbox)
    logger.info(
        f"Reference point (ECEF): [{ref_point[0]:.1f}, {ref_point[1]:.1f}, {ref_point[2]:.1f}]"
    )

    # 4. Ensure output directory exists
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    # 5. Download and export each tile using a thread pool of 6 workers
    tiles_metadata = []
    failed = 0
    skipped = 0

    octant_paths_set = set(octant_paths)

    def process_tile(octant_path: str, index: int) -> dict | None:
        progress = f"[{index + 1}/{len(octant_paths)}]"

        # Resolve node through bulk metadata tree
        node_info = resolve_node(octant_path, root_epoch)
        if node_info is None:
            logger.debug(f"{progress} Skipped {octant_path} (no data)")
            return None

        # Download node data
        logger.info(f"{progress} Downloading {octant_path}...")
        node_data = download_node(node_info)

        # Decode meshes
        masked_octants = set()
        for o in range(8):
            child_path = octant_path + str(o)
            if child_path in octant_paths_set:
                masked_octants.add(o)

        decoded_meshes = decode_node(node_data, masked_octants)
        if not decoded_meshes:
            logger.warning(f"{progress} No meshes in {octant_path}")
            return None

        depth = len(octant_path)
        blend_path = Path(OUTPUT_DIR) / str(depth) / f"{octant_path}.blend"
        jpg_path = Path(OUTPUT_DIR) / str(depth) / f"{octant_path}.jpg"

        # Ensure parent directories exist
        blend_path.parent.mkdir(parents=True, exist_ok=True)
        jpg_path.parent.mkdir(parents=True, exist_ok=True)

        # Construct NodeData URL path for cache resolution
        url_path = f"NodeData/pb=!1m2!1s{node_info.path}!2u{node_info.epoch}!2e{node_info.texture_format}"
        if node_info.imagery_epoch is not None:
            url_path += f"!3u{node_info.imagery_epoch}"
        url_path += "!4b0"
        sha1 = hashlib.sha1(url_path.encode("utf-8")).hexdigest()
        json_path = Path("data_cache") / "json_decoded" / "NodeData" / sha1[:2] / f"{sha1}.json"

        # Build Blend using Blender script
        cmd = [
            "blender",
            "-b",
            "-P",
            "build_blend.py",
            "--",
            str(json_path),
            str(blend_path),
            str(ref_point[0]),
            str(ref_point[1]),
            str(ref_point[2]),
            ",".join(map(str, masked_octants))
        ]
        subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        if not blend_path.exists():
            logger.warning(f"{progress} blend was not generated for {octant_path}")
            return None

        # Save tile and collect metadata
        tile_meta = save_tile(
            octant_path, node_data, decoded_meshes, OUTPUT_DIR
        )

        # Render Blend tile using Blender for preview/diagnostics
        render_tile_via_blender(blend_path, jpg_path, ref_point)

        total_verts = tile_meta["vertex_count"]
        total_tris = tile_meta["triangle_count"]
        size_kb = tile_meta["file_size_bytes"] / 1024
        logger.info(
            f"{progress} Saved {octant_path}.blend and rendered preview "
            f"({tile_meta['mesh_count']} meshes, {total_verts} verts, {total_tris} tris, {size_kb:.1f} KB)"
        )
        return tile_meta

    logger.info("Starting download and export using 6 parallel workers...")
    with ThreadPoolExecutor(max_workers=6) as executor:
        futures = {
            executor.submit(process_tile, path, idx): path
            for idx, path in enumerate(octant_paths)
        }

        for future in as_completed(futures):
            path = futures[future]
            try:
                result = future.result()
                if result is not None:
                    tiles_metadata.append(result)
                else:
                    skipped += 1
            except Exception as e:
                logger.error(f"Task for {path} raised an exception: {e}")
                failed += 1

    # 6. Write manifest
    bbox_dict = {
        "north": bbox.north,
        "south": bbox.south,
        "west": bbox.west,
        "east": bbox.east,
    }
    write_manifest(
        tiles=tiles_metadata,
        bbox=bbox_dict,
        level=target_level,
        reference_point=ref_point.tolist(),
        output_dir=OUTPUT_DIR,
    )

    # Summary
    logger.info("=" * 60)
    logger.info(f"DONE! Exported {len(tiles_metadata)} tiles to {OUTPUT_DIR}/")
    logger.info(f"  Skipped: {skipped}, Failed: {failed}")
    logger.info(f"  Octree level: {target_level}")
    logger.info(f"  Manifest: {OUTPUT_DIR}/manifest.json")
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
