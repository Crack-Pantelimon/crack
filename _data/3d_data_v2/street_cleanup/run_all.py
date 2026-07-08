"""
Run the full street cleanup pipeline for Pantelimon road tiles.

Stages:
  1. Download missing depth-20 tiles along the road
  2. Render top-down orthographic images
  3. Run YOLO and project detections to lat/lon and 3D

Run from _data/3d_data_v2/:
    uv run python street_cleanup/run_all.py
    uv run python street_cleanup/run_all.py --skip-download --limit 5
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import argparse
import logging
import subprocess
import sys

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)],
)
logger = logging.getLogger("run_all")


def run_step(label: str, cmd: list[str]) -> None:
    logger.info("=" * 60)
    logger.info("%s", label)
    logger.info("=" * 60)
    logger.info("Running: %s", " ".join(cmd))
    proc = subprocess.run(cmd)
    if proc.returncode != 0:
        logger.error("Step failed: %s (exit code %s)", label, proc.returncode)
        sys.exit(proc.returncode)


def main() -> None:
    parser = argparse.ArgumentParser(description="Run download -> render -> YOLO pipeline.")
    parser.add_argument("--skip-download", action="store_true", help="Skip tile download stage")
    parser.add_argument("--skip-render", action="store_true", help="Skip render stage")
    parser.add_argument("--skip-yolo", action="store_true", help="Skip YOLO stage")
    parser.add_argument("--limit", type=int, default=0, help="Limit tiles for render/YOLO stages")
    parser.add_argument("--force-render", action="store_true", help="Force re-render")
    parser.add_argument("--force-yolo", action="store_true", help="Force re-run YOLO")
    parser.add_argument("--workers", type=int, default=4, help="Parallel Blender workers for render")
    args = parser.parse_args()

    limit_args = ["--limit", str(args.limit)] if args.limit > 0 else []

    if not args.skip_download:
        run_step("Stage 1/3: Download road tiles", ["uv", "run", "python", "street_cleanup/download_pantelimon_d20.py"])

    if not args.skip_render:
        render_cmd = ["uv", "run", "python", "street_cleanup/run_top_down_renders.py", *limit_args]
        if args.force_render:
            render_cmd.append("--force")
        render_cmd.extend(["--workers", str(args.workers)])
        run_step("Stage 2/3: Top-down rendering", render_cmd)

    if not args.skip_yolo:
        yolo_cmd = ["uv", "run", "python", "street_cleanup/run_yolos.py", *limit_args]
        if args.force_yolo:
            yolo_cmd.append("--force")
        run_step("Stage 3/3: YOLO detection", yolo_cmd)

    logger.info("=" * 60)
    logger.info("Pipeline complete")
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
