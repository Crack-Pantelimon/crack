"""
Google Earth 3D Tile Downloader → GLB Exporter

Downloads 3D photogrammetry tiles from Google Earth's octree for a given
lat/lon bounding box and exports them as .glb files with a JSON manifest.
"""

import os
import sys
import time
import logging
import numpy as np
from pathlib import Path

from octree import parse_bbox, compute_best_level, enumerate_octants_in_bbox, tile_grid_dimensions, octant_path_to_bbox
from earth_client import fetch_planetoid_metadata, resolve_node, download_node, find_tiles_in_bbox
from mesh_decoder import decode_node
from glb_builder import build_glb
from manifest import write_manifest

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("main")

# Configuration
BBOX_FILE = "data_in/zone-bbox.txt"
OUTPUT_DIR = "data_out"
TARGET_GRID = 8  # aim for roughly 8×8 tiles
REQUEST_DELAY = 0.1  # seconds between node downloads


def save_tile(
    glb_bytes: bytes,
    octant_path: str,
    node_data,
    decoded_meshes,
    output_dir: str,
) -> dict:
    """
    Save a GLB file and return metadata for the manifest.
    """
    filename = f"{octant_path}.glb"
    filepath = Path(output_dir) / filename

    with open(filepath, "wb") as f:
        f.write(glb_bytes)

    # Compute stats
    total_verts = sum(len(m.positions) for m in decoded_meshes)
    total_tris = sum(len(m.indices) // 3 for m in decoded_meshes)

    # Get tile bbox
    tile_bbox = octant_path_to_bbox(octant_path)

    metadata = {
        "octant_path": octant_path,
        "filename": filename,
        "file_size_bytes": len(glb_bytes),
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


def compute_reference_point(octant_paths: list[str], root_epoch: int) -> np.ndarray:
    """
    Compute ECEF reference point from the first available tile's center.
    This will be subtracted from all tile positions to keep them near-origin.
    """
    for path in octant_paths[:5]:  # try up to 5 paths
        node_info = resolve_node(path, root_epoch)
        if node_info is None:
            continue
        try:
            node_data = download_node(node_info)
            meshes = decode_node(node_data)
            if meshes and len(meshes[0].positions) > 0:
                # Use the centroid of the first mesh
                return meshes[0].positions.mean(axis=0)
        except Exception as e:
            logger.warning(f"Failed to get reference point from {path}: {e}")
            continue

    # Fallback: compute from lat/lon
    logger.warning("Could not compute reference point from tiles, using bbox center")
    return np.array([0.0, 0.0, 0.0])


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

    # 2. Compute optimal level dynamically by searching the actual octree
    target_count = TARGET_GRID * TARGET_GRID
    level = 16  # default fallback
    octant_paths = []
    closest_diff = float("inf")

    logger.info(f"Dynamically searching octree levels to find best detail closest to {target_count} tiles...")
    # Search levels 13 to 18
    for lvl in range(13, 19):
        tiles = find_tiles_in_bbox(bbox, lvl, root_epoch)
        diff = abs(len(tiles) - target_count)
        logger.info(f"Level {lvl}: found {len(tiles)} tiles (diff to {target_count}: {diff})")
        if diff < closest_diff:
            closest_diff = diff
            level = lvl
            octant_paths = tiles

    logger.info(f"Selected optimal level: {level} with {len(octant_paths)} tiles")

    # 3. Compute reference point (ECEF offset)
    logger.info("Computing reference point from first tile...")
    ref_point = compute_reference_point(octant_paths, root_epoch)
    logger.info(f"Reference point (ECEF): [{ref_point[0]:.1f}, {ref_point[1]:.1f}, {ref_point[2]:.1f}]")

    # 4. Ensure output directory exists
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    # 5. Download and export each tile
    tiles_metadata = []
    failed = 0
    skipped = 0

    for i, octant_path in enumerate(octant_paths):
        progress = f"[{i+1}/{len(octant_paths)}]"

        # Resolve node through bulk metadata tree
        node_info = resolve_node(octant_path, root_epoch)
        if node_info is None:
            logger.debug(f"{progress} Skipped {octant_path} (no data)")
            skipped += 1
            continue

        try:
            # Download node data
            logger.info(f"{progress} Downloading {octant_path}...")
            node_data = download_node(node_info)

            # Decode meshes
            decoded_meshes = decode_node(node_data)
            if not decoded_meshes:
                logger.warning(f"{progress} No meshes in {octant_path}")
                skipped += 1
                continue

            # Build GLB
            glb_bytes = build_glb(decoded_meshes, octant_path, reference_point=ref_point)
            if not glb_bytes:
                logger.warning(f"{progress} Empty GLB for {octant_path}")
                skipped += 1
                continue

            # Save tile and collect metadata
            tile_meta = save_tile(glb_bytes, octant_path, node_data, decoded_meshes, OUTPUT_DIR)
            tiles_metadata.append(tile_meta)

            total_verts = tile_meta["vertex_count"]
            total_tris = tile_meta["triangle_count"]
            size_kb = tile_meta["file_size_bytes"] / 1024
            logger.info(
                f"{progress} Saved {octant_path}.glb "
                f"({tile_meta['mesh_count']} meshes, {total_verts} verts, {total_tris} tris, {size_kb:.1f} KB)"
            )

        except Exception as e:
            logger.error(f"{progress} Failed {octant_path}: {e}")
            failed += 1

        # Rate limiting
        if i < len(octant_paths) - 1:
            time.sleep(REQUEST_DELAY)

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
        level=level,
        reference_point=ref_point.tolist(),
        output_dir=OUTPUT_DIR,
    )

    # Summary
    logger.info("=" * 60)
    logger.info(f"DONE! Exported {len(tiles_metadata)} tiles to {OUTPUT_DIR}/")
    logger.info(f"  Skipped: {skipped}, Failed: {failed}")
    logger.info(f"  Octree level: {level}")
    logger.info(f"  Manifest: {OUTPUT_DIR}/manifest.json")
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
