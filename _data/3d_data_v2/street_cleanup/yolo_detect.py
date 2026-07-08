"""
Shared YOLO detection logic for top-down tile renders.

Processes one render at a time, projects detections to lat/lon and 3D, and
writes per-tile outputs:
  - <octant_path>_yolo.png  annotated render (red=vehicles, green=trees)
  - <octant_path>_yolo.json detection list with projections
"""

from __future__ import annotations

import json
import logging
from pathlib import Path

import cv2
from ultralytics import YOLO

from street_cleanup.coord_utils import (
    bbox_center_blender_xyz,
    bbox_center_latlon,
    blender_to_enu,
)
from street_cleanup.road_tiles import RoadTile, list_road_d20_tiles

logger = logging.getLogger("yolo_detect")

TARGET_CLASSES = {
    2: "car",
    3: "motorcycle",
    5: "bus",
    7: "truck",
    58: "tree",
}

CAR_BGR = (0, 0, 255)
TREE_BGR = (0, 255, 0)


def class_name_for_id(cls_id: int) -> str:
    return "tree" if cls_id == 58 else TARGET_CLASSES[cls_id]


def bbox_color_bgr(class_name: str) -> tuple[int, int, int]:
    return TREE_BGR if class_name == "tree" else CAR_BGR


def project_detection(bbox_pixel: list[float], meta: dict) -> dict:
    width, height = meta["resolution"]
    lat_lon_bbox = meta["lat_lon_bbox"]
    bbox_xyz = meta["bbox_xyz"]

    x1, y1, x2, y2 = bbox_pixel
    lat, lon = bbox_center_latlon(x1, y1, x2, y2, width, height, lat_lon_bbox)
    bx, by, bz = bbox_center_blender_xyz(x1, y1, x2, y2, width, height, bbox_xyz)
    east, north, up = blender_to_enu(bx, by, bz)

    return {
        "bbox_pixel": [round(v, 1) for v in bbox_pixel],
        "lat": round(lat, 8),
        "lon": round(lon, 8),
        "pos_blender": [round(bx, 2), round(by, 2), round(bz, 2)],
        "pos_enu": [round(east, 2), round(north, 2), round(up, 2)],
    }


def draw_annotated_image(image_path: Path, detections: list[dict], out_path: Path) -> None:
    image = cv2.imread(str(image_path))
    if image is None:
        raise RuntimeError(f"Failed to read image: {image_path}")

    for det in detections:
        x1, y1, x2, y2 = (int(v) for v in det["bbox_pixel"])
        color = bbox_color_bgr(det["class_name"])
        cv2.rectangle(image, (x1, y1), (x2, y2), color, 1)
        label = f"{det['class_name']} {det['confidence']:.2f}"
        cv2.putText(
            image,
            label,
            (x1, max(y1 - 2, 8)),
            cv2.FONT_HERSHEY_SIMPLEX,
            0.35,
            color,
            1,
            cv2.LINE_AA,
        )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    cv2.imwrite(str(out_path), image)


def outputs_complete(tile: RoadTile) -> bool:
    return tile.yolo_png.exists() and tile.yolo_json.exists()


def process_tile(model: YOLO, tile: RoadTile, *, force: bool) -> int:
    if not tile.render_jpg.exists() or not tile.render_meta.exists():
        return -1

    if outputs_complete(tile) and not force:
        return 0

    with open(tile.render_meta, "r", encoding="utf-8") as f:
        meta = json.load(f)

    results = model.predict(
        source=str(tile.render_jpg),
        conf=0.25,
        iou=0.45,
        classes=list(TARGET_CLASSES.keys()),
        verbose=False,
    )

    detections: list[dict] = []
    for box in results[0].boxes:
        cls_id = int(box.cls[0])
        cls_name = class_name_for_id(cls_id)
        bbox_pixel = box.xyxy[0].tolist()
        projected = project_detection(bbox_pixel, meta)
        detections.append(
            {
                "class_id": cls_id,
                "class_name": cls_name,
                "confidence": round(float(box.conf[0]), 4),
                **projected,
            }
        )

    output = {
        "octant_path": tile.octant_path,
        "glb_path": str(tile.glb_path),
        "render_jpg": str(tile.render_jpg),
        "render_meta": str(tile.render_meta),
        "yolo_png": str(tile.yolo_png),
        "resolution": meta["resolution"],
        "lat_lon_bbox": meta.get("lat_lon_bbox"),
        "bbox_xyz": meta.get("bbox_xyz"),
        "camera_location": meta.get("camera_location"),
        "detections": detections,
    }

    tile.yolo_json.parent.mkdir(parents=True, exist_ok=True)
    with open(tile.yolo_json, "w", encoding="utf-8") as f:
        json.dump(output, f, indent=2)

    draw_annotated_image(tile.render_jpg, detections, tile.yolo_png)
    return len(detections)


def tiles_with_renders(*, limit: int = 0) -> list[RoadTile]:
    tiles = [t for t in list_road_d20_tiles(only_existing=True) if t.render_jpg.exists()]
    if limit > 0:
        tiles = tiles[:limit]
    return tiles


def run_yolo_on_tiles(
    *,
    limit: int = 0,
    force: bool = False,
    model_name: str = "yolo11x.pt",
) -> dict[str, int]:
    tiles = tiles_with_renders(limit=limit)
    if not tiles:
        logger.warning("No rendered tiles found. Run run_top_down_renders.py first.")
        return {"processed": 0, "skipped": 0, "failed": 0, "detections": 0, "tiles_with_detections": 0}

    logger.info("Loading YOLO model: %s", model_name)
    model = YOLO(model_name)

    stats = {
        "processed": 0,
        "skipped": 0,
        "failed": 0,
        "detections": 0,
        "tiles_with_detections": 0,
    }
    class_counts: dict[str, int] = {}

    for i, tile in enumerate(tiles, start=1):
        try:
            count = process_tile(model, tile, force=force)
        except Exception as exc:
            stats["failed"] += 1
            logger.error("YOLO failed for %s: %s", tile.octant_path, exc)
            continue

        if count < 0:
            stats["skipped"] += 1
            continue
        if count == 0 and outputs_complete(tile) and not force:
            stats["skipped"] += 1
        else:
            stats["processed"] += 1
            if count > 0:
                stats["tiles_with_detections"] += 1
                stats["detections"] += count
                with open(tile.yolo_json, "r", encoding="utf-8") as f:
                    payload = json.load(f)
                for det in payload["detections"]:
                    name = det["class_name"]
                    class_counts[name] = class_counts.get(name, 0) + 1

        if i % 25 == 0 or i == len(tiles):
            logger.info("Processed %s/%s tiles", i, len(tiles))

    for name, count in sorted(class_counts.items()):
        logger.info("  %s: %s", name, count)

    return stats
