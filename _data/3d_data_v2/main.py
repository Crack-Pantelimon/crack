"""
Google Earth 3D Tile Downloader → GLB Exporter

Downloads 3D photogrammetry tiles from Google Earth's octree for a given
lat/lon bounding box and exports them as .blend + .glb files (plus .jpg previews).

This script does NOT write a manifest. Run `rebuild_manifest.py` afterwards to
(re)compute the manifest from the exported .glb files on disk.
"""

import os
import time
import math
import queue
import logging
import threading
import numpy as np
from pathlib import Path
import subprocess

from octree import (
    parse_bbox,
    compute_best_level,
)
from earth_client import (
    fetch_planetoid_metadata,
    resolve_node,
    download_node,
    find_tiles_in_bbox,
)
from mesh_decoder import decode_node
import hashlib

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("main")


def run_blender_script(script: str, script_args: list[str]):
    """
    Run a Blender -P script and fail loudly on script-level errors.

    Blender returns exit code 0 even when the embedded Python script raises, so a
    non-zero return code is not enough to detect failure. We capture the combined
    output and treat any Python traceback / Blender error as a hard failure.
    """
    cmd = ["blender", "-b", "-P", script, "--", *script_args]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    output = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0 or "Traceback (most recent call last)" in output or "\nError: " in output:
        raise RuntimeError(
            f"Blender script {script} failed (returncode={proc.returncode}).\n"
            f"---- blender output ----\n{output.strip()}\n------------------------"
        )


def render_tile_via_blender(blend_path: Path, jpg_path: Path, ref_point: np.ndarray):
    """Render a blend file using Blender script in Cycles CPU mode."""
    try:
        run_blender_script(
            "render_tile.py",
            [
                str(blend_path),
                str(jpg_path),
                str(ref_point[0]),
                str(ref_point[1]),
                str(ref_point[2]),
            ],
        )
    except Exception as e:
        logger.warning(f"Blender rendering failed for {blend_path.name}: {e}")


# Configuration
BBOX_FILE = "data_in/zone-bbox.txt"
OUTPUT_DIR = "data_out"
TARGET_GRID = 512  # aim for roughly 3x3 tiles
REQUEST_DELAY = 0.01  # seconds between node downloads
GET_ALL_COARSER_LEVELS = True  # If True, download all levels of detail smaller than (coarser than or equal to) the 3x3 optimal level
NETWORK_WORKERS = 100  # threads fetching + caching node data from the network
BLENDER_WORKERS = 7  # threads running the Blender build/render subprocesses


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
    """Main pipeline: parse bbox → compute level → download → export .blend/.glb + preview."""

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

    # 5. Two-stage parallel pipeline connected by thread-safe queues:
    #      - a network pool (NETWORK_WORKERS threads) that resolves each node,
    #        downloads + caches it via _fetch_raw (download_node), and decodes it to
    #        validate that it has geometry;
    #      - a Blender pool (BLENDER_WORKERS threads) that turns each cached node into
    #        a .blend/.glb and renders a .jpg preview.
    #    Network workers push ready work items onto process_q; Blender workers consume.
    total = len(octant_paths)
    fetch_q: queue.Queue = queue.Queue()
    process_q: queue.Queue = queue.Queue()

    counts = {"exported": 0, "skipped": 0, "failed": 0}
    counts_lock = threading.Lock()

    def bump(key: str):
        with counts_lock:
            counts[key] += 1

    def fetch_one(index: int, octant_path: str) -> dict | None:
        """Network stage: resolve → download (cached) → decode-validate a single node."""
        progress = f"[{index + 1}/{total}]"
        depth = len(octant_path)
        glb_path = Path(OUTPUT_DIR) / str(depth) / f"{octant_path}.glb"
        jpg_path = Path(OUTPUT_DIR) / str(depth) / f"{octant_path}.jpg"

        # Reentrancy: if this entry already has both its GLB and JPG, skip it.
        if glb_path.exists() and jpg_path.exists():
            logger.info(f"{progress} Skipping {octant_path} (glb + jpg already present)")
            bump("skipped")
            return None

        # Resolve node through bulk metadata tree
        node_info = resolve_node(octant_path, root_epoch)
        if node_info is None:
            logger.debug(f"{progress} Skipped {octant_path} (no data)")
            bump("skipped")
            return None

        # Download node data (fetches + caches raw bytes and the decoded JSON)
        logger.info(f"{progress} Downloading {octant_path}...")
        node_data = download_node(node_info)

        # Decode meshes. No octant masking: tiles are kept whole so coarse LODs
        # (e.g. levels 10/11) are not carved up by octants that have finer tiles.
        decoded_meshes = decode_node(node_data)
        if not decoded_meshes:
            logger.warning(f"{progress} No meshes in {octant_path}")
            bump("skipped")
            return None

        blend_path = Path(OUTPUT_DIR) / str(depth) / f"{octant_path}.blend"

        # Ensure parent directories exist
        blend_path.parent.mkdir(parents=True, exist_ok=True)
        jpg_path.parent.mkdir(parents=True, exist_ok=True)

        # Construct NodeData URL path for cache resolution (consumed by build_blend.py)
        url_path = f"NodeData/pb=!1m2!1s{node_info.path}!2u{node_info.epoch}!2e{node_info.texture_format}"
        if node_info.imagery_epoch is not None:
            url_path += f"!3u{node_info.imagery_epoch}"
        url_path += "!4b0"
        sha1 = hashlib.sha1(url_path.encode("utf-8")).hexdigest()
        json_path = Path("data_cache") / "json_decoded" / "NodeData" / sha1[:2] / f"{sha1}.json"

        return {
            "progress": progress,
            "octant_path": octant_path,
            "json_path": json_path,
            "blend_path": blend_path,
            "glb_path": glb_path,
            "jpg_path": jpg_path,
            "mesh_count": len(decoded_meshes),
            "vertex_count": sum(len(m.positions) for m in decoded_meshes),
            "triangle_count": sum(len(m.indices) // 3 for m in decoded_meshes),
        }

    def process_item(item: dict):
        """Blender stage: build .blend/.glb, verify freshness, render preview."""
        progress = item["progress"]
        octant_path = item["octant_path"]
        blend_path = item["blend_path"]
        glb_path = item["glb_path"]
        jpg_path = item["jpg_path"]

        # Build Blend (and GLB) using Blender script.
        build_start = time.time()
        run_blender_script(
            "build_blend.py",
            [
                str(item["json_path"]),
                str(blend_path),
                str(ref_point[0]),
                str(ref_point[1]),
                str(ref_point[2]),
            ],
        )

        # Require freshly written artifacts; a stale pre-existing file must not pass.
        for artifact in (blend_path, glb_path):
            if not artifact.exists() or artifact.stat().st_mtime < build_start:
                logger.warning(f"{progress} {artifact.name} was not (re)generated for {octant_path}")
                bump("skipped")
                return

        # Render Blend tile using Blender for preview/diagnostics
        render_tile_via_blender(blend_path, jpg_path, ref_point)

        logger.info(
            f"{progress} Saved {octant_path}.blend/.glb and rendered preview "
            f"({item['mesh_count']} meshes, {item['vertex_count']} verts, {item['triangle_count']} tris)"
        )
        bump("exported")

    def network_worker():
        while True:
            task = fetch_q.get()
            try:
                if task is None:  # sentinel: no more fetch work
                    return
                index, octant_path = task
                try:
                    work = fetch_one(index, octant_path)
                    if work is not None:
                        process_q.put(work)
                except Exception as e:
                    logger.error(f"Fetch for {octant_path} raised an exception: {e}")
                    bump("failed")
            finally:
                fetch_q.task_done()

    def blender_worker():
        while True:
            item = process_q.get()
            try:
                if item is None:  # sentinel: no more Blender work
                    return
                try:
                    process_item(item)
                except Exception as e:
                    logger.error(f"Blender stage for {item['octant_path']} raised an exception: {e}")
                    bump("failed")
            finally:
                process_q.task_done()

    logger.info(
        f"Starting pipeline: {NETWORK_WORKERS} network workers → {BLENDER_WORKERS} Blender workers..."
    )
    net_threads = [
        threading.Thread(target=network_worker, name=f"net-{i}", daemon=True)
        for i in range(NETWORK_WORKERS)
    ]
    blend_threads = [
        threading.Thread(target=blender_worker, name=f"blend-{i}", daemon=True)
        for i in range(BLENDER_WORKERS)
    ]
    for t in net_threads + blend_threads:
        t.start()

    # Enqueue every fetch task, then one sentinel per network worker.
    for idx, path in enumerate(octant_paths):
        fetch_q.put((idx, path))
    for _ in net_threads:
        fetch_q.put(None)

    # Wait for the network stage to fully drain before signalling the Blender stage,
    # so all real work items are enqueued on process_q ahead of its sentinels.
    for t in net_threads:
        t.join()
    for _ in blend_threads:
        process_q.put(None)
    for t in blend_threads:
        t.join()

    # Summary
    logger.info("=" * 60)
    logger.info(f"DONE! Exported {counts['exported']} tiles to {OUTPUT_DIR}/")
    logger.info(f"  Skipped: {counts['skipped']}, Failed: {counts['failed']}")
    logger.info(f"  Octree level: {target_level}")
    logger.info("  Run rebuild_manifest.py to (re)generate the manifest.")
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
