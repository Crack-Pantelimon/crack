"""
Google Earth 3D Tile Downloader → GLB Exporter

Downloads 3D photogrammetry tiles from Google Earth's octree for a given
lat/lon bounding box and exports them as .blend + .glb files (plus .jpg previews).

This script does NOT write a manifest. Run `rebuild_manifest.py` afterwards to
(re)compute the manifest from the exported .glb files on disk.
"""

import os
import time
import json
import math
import queue
import logging
import tempfile
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
    find_tiles_in_bbox_levels,
)
from mesh_decoder import decode_node
import hashlib

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("main")


def run_blender_batch(script: str, batch_json_path: str) -> str:
    """
    Run a Blender -P script over a whole batch (single process) and return its output.

    Each batched Blender script tolerates per-node failures internally (it keeps
    going and reports them), so here we only treat a hard crash (non-zero return
    code, i.e. Blender itself died/segfaulted) as fatal. Per-node success is
    verified by the caller via on-disk artifact freshness checks.
    """
    cmd = ["blender", "-b", "-P", script, "--", batch_json_path]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    output = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0:
        raise RuntimeError(
            f"Blender script {script} crashed (returncode={proc.returncode}).\n"
            f"---- blender output ----\n{output.strip()}\n------------------------"
        )
    return output


# Configuration
BBOX_FILE = "data_in/zone-bbox.txt"
OUTPUT_DIR = "data_out"
TARGET_GRID = 256  # aim for roughly 3x3 tiles
REQUEST_DELAY = 0.01  # seconds between node downloads
GET_ALL_COARSER_LEVELS = True  # If True, download all levels of detail smaller than (coarser than or equal to) the 3x3 optimal level
NETWORK_WORKERS = 100  # threads fetching + caching node data from the network
BLENDER_WORKERS = 7  # threads running the Blender build/render subprocesses
BLENDER_BATCH_SIZE = 32  # nodes handled per single Blender process (amortizes startup cost)


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
    # Enumerate all intersecting tiles for levels 10..target_level in a single
    # breadth-first pass (parallelized bulk fetches), instead of one full
    # re-traversal per level.
    level_tiles = find_tiles_in_bbox_levels(bbox, 10, target_level, root_epoch)
    for lvl in range(10, target_level + 1):
        tiles = level_tiles.get(lvl, [])
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
        # jpg_path = Path(OUTPUT_DIR) / str(depth) / f"{octant_path}.jpg"

        # Reentrancy: if this entry already has both its GLB and JPG, skip it.
        if glb_path.exists():
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
        # jpg_path.parent.mkdir(parents=True, exist_ok=True)

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
            # "jpg_path": jpg_path,
            "mesh_count": len(decoded_meshes),
            "vertex_count": sum(len(m.positions) for m in decoded_meshes),
            "triangle_count": sum(len(m.indices) // 3 for m in decoded_meshes),
        }

    def process_batch(batch: list[dict]):
        """
        Blender stage for a whole batch of nodes in two single Blender runs:
        one build_blend.py run (→ .blend + .glb) and one render_tile.py run (→ .jpg).
        The batch spec is handed to Blender through a single temp JSON file.
        """
        ref_list = [float(ref_point[0]), float(ref_point[1]), float(ref_point[2])]
        spec = {
            "ref_point": ref_list,
            "nodes": [
                {
                    "octant_path": it["octant_path"],
                    "json_path": str(it["json_path"]),
                    "blend_path": str(it["blend_path"]),
                    "glb_path": str(it["glb_path"]),
                    # "jpg_path": str(it["jpg_path"]),
                }
                for it in batch
            ],
        }

        build_start = time.time()
        with tempfile.NamedTemporaryFile(
            "w", suffix=".json", prefix="blender_batch_", delete=True
        ) as tf:
            json.dump(spec, tf)
            tf.flush()

            # 1. Build .blend + .glb for the whole batch in one Blender process.
            try:
                run_blender_batch("build_blend.py", tf.name)
            except Exception as e:
                logger.error(f"build_blend batch crashed ({len(batch)} nodes): {e}")
                for _ in batch:
                    bump("failed")
                return

            # 2. Verify each node produced fresh artifacts.
            built = []
            for it in batch:
                fresh = all(
                    p.exists() and p.stat().st_mtime >= build_start
                    for p in (it["blend_path"], it["glb_path"])
                )
                if fresh:
                    built.append(it)
                    bump("exported")
                    logger.info(
                        f"{it['progress']} Saved {it['octant_path']}.blend/.glb "
                        f"({it['mesh_count']} meshes, {it['vertex_count']} verts, "
                        f"{it['triangle_count']} tris)"
                    )
                else:
                    bump("skipped")
                    logger.warning(
                        f"{it['progress']} {it['octant_path']} was not (re)generated by build_blend"
                    )

            # 3. Render previews for everything that built (best-effort, non-fatal).
            # if built:
            #     render_spec = {"ref_point": ref_list, "nodes": [
            #         {"octant_path": it["octant_path"],
            #          "blend_path": str(it["blend_path"]),
            #          "jpg_path": str(it["jpg_path"])}
            #         for it in built
            #     ]}
            #     with tempfile.NamedTemporaryFile(
            #         "w", suffix=".json", prefix="render_batch_", delete=True
            #     ) as rtf:
            #         json.dump(render_spec, rtf)
            #         rtf.flush()
            #         try:
            #             run_blender_batch("render_tile.py", rtf.name)
            #         except Exception as e:
            #             logger.warning(f"render_tile batch failed ({len(built)} nodes): {e}")

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
            # Block for the first item, then greedily drain up to a full batch so a
            # single Blender process can handle many nodes at once.
            first = process_q.get()
            if first is None:  # sentinel: no more Blender work
                return
            batch = [first]
            while len(batch) < BLENDER_BATCH_SIZE:
                try:
                    nxt = process_q.get_nowait()
                except queue.Empty:
                    break
                if nxt is None:
                    # Hand the shutdown sentinel back so another worker (or our next
                    # loop) can observe it, then stop accumulating.
                    process_q.put(None)
                    break
                batch.append(nxt)

            try:
                process_batch(batch)
            except Exception as e:
                logger.error(f"Blender batch of {len(batch)} nodes raised: {e}")
                for _ in batch:
                    bump("failed")

    logger.info(
        f"Starting pipeline: {NETWORK_WORKERS} network workers → "
        f"{BLENDER_WORKERS} Blender workers (batch size {BLENDER_BATCH_SIZE})..."
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
