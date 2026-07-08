"""
Run YOLO on top-down renders one image at a time.

For each tile writes:
  - renders/<folder>/<octant_path>_yolo.png  annotated bboxes (red=vehicles, green=trees)
  - renders/<folder>/<octant_path>_yolo.json detection list with lat/lon and 3D positions

Run from _data/3d_data_v2/:
    uv run python street_cleanup/run_yolos.py
    uv run python street_cleanup/run_yolos.py --limit 10 --force
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import argparse
import logging

from street_cleanup.yolo_detect import run_yolo_on_tiles

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)],
)
logger = logging.getLogger("run_yolos")


def main() -> None:
    parser = argparse.ArgumentParser(description="YOLO detection on top-down renders (one image at a time).")
    parser.add_argument("--limit", type=int, default=0, help="Limit number of tiles (0 = all)")
    parser.add_argument("--force", action="store_true", help="Re-run YOLO even if outputs exist")
    parser.add_argument("--model", type=str, default="yolo11x.pt", help="YOLO weights file")
    args = parser.parse_args()

    logger.info("=" * 60)
    logger.info("YOLO Top-Down Detection (sequential)")
    logger.info("=" * 60)

    stats = run_yolo_on_tiles(limit=args.limit, force=args.force, model_name=args.model)

    logger.info("Processed: %s", stats["processed"])
    logger.info("Skipped: %s", stats["skipped"])
    logger.info("Failed: %s", stats["failed"])
    logger.info("Tiles with detections: %s", stats["tiles_with_detections"])
    logger.info("Total detections: %s", stats["detections"])
    logger.info("Outputs: street_cleanup/renders/<last3>/<octant_path>_yolo.png|.json")
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
