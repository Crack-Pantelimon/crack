"""
Orchestrate top-down orthographic tile rendering in Blender batches.

Run from _data/3d_data_v2/:
    uv run python street_cleanup/run_top_down_renders.py
    uv run python street_cleanup/run_top_down_renders.py --limit 10 --force
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import argparse
import json
import logging
import os
import subprocess
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

from street_cleanup.road_tiles import RoadTile, list_road_d20_tiles

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)],
)
logger = logging.getLogger("run_top_down_renders")

BLENDER_BATCH_SIZE = 16
MAX_WORKERS = 4


def tile_to_render_spec(tile: RoadTile) -> dict:
    return {
        "octant_path": tile.octant_path,
        "glb_path": str(tile.glb_path),
        "jpg_path": str(tile.render_jpg),
        "meta_path": str(tile.render_meta),
        "lat_lon_bbox": tile.lat_lon_bbox(),
    }


def run_blender_batch(batch_tiles: list[dict]) -> tuple[int, int]:
    spec = {"tiles": batch_tiles}
    with tempfile.NamedTemporaryFile("w", suffix=".json", prefix="topdown_batch_", delete=False) as tf:
        json.dump(spec, tf)
        tf_name = tf.name

    cmd = [
        "blender",
        "-b",
        "-P",
        "street_cleanup/render_top_down.py",
        "--",
        tf_name,
    ]

    t0 = time.time()
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True)
        if os.path.exists(tf_name):
            os.remove(tf_name)

        output = (proc.stdout or "") + (proc.stderr or "")
        if proc.returncode != 0:
            logger.error("Blender process failed (code=%s)\n%s", proc.returncode, output[-2000:])
            return 0, len(batch_tiles)

        success_count = output.count("RENDER_OK")
        fail_count = output.count("RENDER_FAIL")
        logger.info("Batch completed in %.1fs: %s OK, %s failed", time.time() - t0, success_count, fail_count)
        return success_count, fail_count
    except Exception as exc:
        logger.error("Error running Blender batch: %s", exc)
        if os.path.exists(tf_name):
            os.remove(tf_name)
        return 0, len(batch_tiles)


def main() -> None:
    parser = argparse.ArgumentParser(description="Render top-down orthographic tile images.")
    parser.add_argument("--limit", type=int, default=0, help="Limit number of tiles (0 = all)")
    parser.add_argument("--workers", type=int, default=MAX_WORKERS, help="Parallel Blender processes")
    parser.add_argument("--force", action="store_true", help="Re-render even if outputs exist")
    args = parser.parse_args()

    logger.info("=" * 60)
    logger.info("Top-Down Tile Renderer")
    logger.info("=" * 60)

    tiles = list_road_d20_tiles(only_existing=True)
    logger.info("Found %s road tiles with GLBs on disk", len(tiles))

    todo: list[RoadTile] = []
    for tile in tiles:
        if args.force or not (tile.render_jpg.exists() and tile.render_meta.exists()):
            todo.append(tile)

    logger.info("Already rendered: %s", len(tiles) - len(todo))
    logger.info("Remaining to render: %s", len(todo))

    if not todo:
        logger.info("Nothing to render.")
        return

    if args.limit > 0:
        todo = todo[: args.limit]
        logger.info("Limiting to %s tiles", len(todo))

    batches = [
        [tile_to_render_spec(tile) for tile in todo[i : i + BLENDER_BATCH_SIZE]]
        for i in range(0, len(todo), BLENDER_BATCH_SIZE)
    ]
    logger.info("Divided work into %s batches", len(batches))

    total_ok = 0
    total_fail = 0
    t_start = time.time()

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = {executor.submit(run_blender_batch, batch): batch for batch in batches}
        for i, future in enumerate(as_completed(futures), start=1):
            ok, fail = future.result()
            total_ok += ok
            total_fail += fail
            elapsed = time.time() - t_start
            eta = (elapsed / i) * (len(batches) - i) if i else 0.0
            logger.info(
                "Progress: %s/%s batches | OK: %s, Fail: %s | Elapsed: %.1fs, ETA: %.1fs",
                i,
                len(batches),
                total_ok,
                total_fail,
                elapsed,
                eta,
            )

    logger.info("=" * 60)
    logger.info("Rendering complete: %s OK, %s failed in %.1fs", total_ok, total_fail, time.time() - t_start)
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
