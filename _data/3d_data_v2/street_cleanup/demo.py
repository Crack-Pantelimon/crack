"""
Demo: render one tile at 1024x1024 and run all YOLO models on it.

Writes:
  street_cleanup/demo.sh             Blender render command
  street_cleanup/demo/demo1.jpg      Top-down render
  street_cleanup/demo/demo1.json     Render metadata
  street_cleanup/demo/<model>.jpg    Annotated detections (all classes)
  street_cleanup/demo/<model>.json   Detection list + full class catalog

Run from _data/3d_data_v2/:
    uv run street_cleanup/demo.py
"""

from __future__ import annotations

import json
import logging
import subprocess
import sys
import time
from pathlib import Path

import cv2
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ultralytics import YOLO

from street_cleanup.road_tiles import glb_path_for_octant

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)],
)
logger = logging.getLogger("demo")

OCTANT_PATH = "30436272704361707403"
RENDER_SIZE = 128
DEMO_DIR = Path("street_cleanup/demo")
DEMO_JPG = DEMO_DIR / "demo1.jpg"
DEMO_JSON = DEMO_DIR / "demo1.json"
DEMO_BATCH = DEMO_DIR / "demo_batch.json"
DEMO_SH = Path("street_cleanup/demo.sh")


def lat_lon_bbox_for_octant(octant_path: str) -> dict[str, float]:
    existing_meta = Path("street_cleanup/renders") / octant_path[-3:] / f"{octant_path}.json"
    if existing_meta.exists():
        with open(existing_meta, "r", encoding="utf-8") as f:
            return json.load(f)["lat_lon_bbox"]

    from octree import octant_path_to_bbox

    tile_box = octant_path_to_bbox(octant_path)
    return {
        "lat_south": tile_box.south,
        "lat_north": tile_box.north,
        "lon_west": tile_box.west,
        "lon_east": tile_box.east,
    }


def build_render_spec() -> dict:
    glb_path = glb_path_for_octant(OCTANT_PATH)
    if not glb_path.exists():
        raise FileNotFoundError(f"GLB not found: {glb_path}")

    return {
        "octant_path": OCTANT_PATH,
        "glb_path": str(glb_path),
        "jpg_path": str(DEMO_JPG),
        "meta_path": str(DEMO_JSON),
        "lat_lon_bbox": lat_lon_bbox_for_octant(OCTANT_PATH),
        "resolution": [RENDER_SIZE, RENDER_SIZE],
    }


def write_demo_sh(batch_path: Path) -> None:
    script = f"""#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
blender -b -P street_cleanup/render_top_down.py -- {batch_path.as_posix()}
"""
    DEMO_SH.write_text(script, encoding="utf-8")
    DEMO_SH.chmod(0o755)
    logger.info("Wrote %s", DEMO_SH)


def run_render(batch_path: Path) -> None:
    cmd = ["blender", "-b", "-P", "street_cleanup/render_top_down.py", "--", str(batch_path)]
    logger.info("Running: %s", " ".join(cmd))
    proc = subprocess.run(cmd, capture_output=True, text=True)
    output = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0 or "RENDER_OK" not in output:
        logger.error("Blender render failed (code=%s)\n%s", proc.returncode, output[-3000:])
        raise RuntimeError("Blender render failed")
    logger.info("Render complete: %s", DEMO_JPG)


def discover_yolo_models() -> list[Path]:
    root = Path(".")
    models = sorted(root.glob("*.pt"))
    if not models:
        raise FileNotFoundError("No YOLO .pt weights found in project root")
    return models


def class_catalog(model: YOLO) -> dict[str, str]:
    return {str(cls_id): str(name) for cls_id, name in model.names.items()}


def bbox_color_bgr(cls_id: int) -> tuple[int, int, int]:
    hue = (cls_id * 37) % 180
    hsv = np.uint8([[[hue, 220, 220]]])
    bgr = cv2.cvtColor(hsv, cv2.COLOR_HSV2BGR)
    return int(bgr[0, 0, 0]), int(bgr[0, 0, 1]), int(bgr[0, 0, 2])


def draw_all_class_detections(image_path: Path, detections: list[dict], out_path: Path) -> None:
    image = cv2.imread(str(image_path))
    if image is None:
        raise RuntimeError(f"Failed to read image: {image_path}")

    for det in detections:
        x1, y1, x2, y2 = (int(v) for v in det["bbox_pixel"])
        color = bbox_color_bgr(det["class_id"])
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


def run_yolo_on_demo(image_path: Path, model_path: Path, out_jpg: Path, out_json: Path) -> int:
    model = YOLO(str(model_path))
    classes = class_catalog(model)
    results = model.predict(
        source=str(image_path),
        conf=0.25,
        iou=0.45,
        verbose=False,
    )

    detections: list[dict] = []
    for box in results[0].boxes:
        cls_id = int(box.cls[0])
        detections.append(
            {
                "class_id": cls_id,
                "class_name": classes[str(cls_id)],
                "confidence": round(float(box.conf[0]), 4),
                "bbox_pixel": [round(v, 1) for v in box.xyxy[0].tolist()],
            }
        )

    payload = {
        "model": model_path.name,
        "source_image": str(image_path),
        "output_image": str(out_jpg),
        "classes": classes,
        "detections": detections,
    }
    out_json.parent.mkdir(parents=True, exist_ok=True)
    with open(out_json, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2)

    draw_all_class_detections(image_path, detections, out_jpg)
    return len(detections)


def main() -> None:
    logger.info("=" * 60)
    logger.info("Street cleanup demo tile: %s @ %sx%s", OCTANT_PATH, RENDER_SIZE, RENDER_SIZE)
    logger.info("=" * 60)

    DEMO_DIR.mkdir(parents=True, exist_ok=True)

    tile = build_render_spec()
    batch = {"tiles": [tile]}
    with open(DEMO_BATCH, "w", encoding="utf-8") as f:
        json.dump(batch, f, indent=2)

    write_demo_sh(DEMO_BATCH)
    run_render(DEMO_BATCH)

    if not DEMO_JPG.exists():
        raise FileNotFoundError(f"Expected render output missing: {DEMO_JPG}")

    # 1. Run YOLOv7 ONNX (OpenCV DNN)
    logger.info("Running YOLOv7 ONNX (OpenCV DNN)...")
    net = cv2.dnn.readNetFromONNX("street_cleanup/yolov7-m_itcvd_qgis.onnx")
    img = cv2.imread(str(DEMO_JPG))
    
    t0 = time.time()
    # Run YOLOv7 ONNX
    blob = cv2.dnn.blobFromImage(img, 1.0 / 255.0, (640, 640), (0, 0, 0), swapRB=True, crop=False)
    net.setInput(blob)
    preds = net.forward()[0]
    # NMS
    raw_dets = []
    h_img, w_img, _ = img.shape
    for pred in preds:
        obj_conf = pred[4]
        class_prob = pred[5]
        conf = obj_conf * class_prob
        if conf >= 0.20:
            x_c, y_c, w, h = pred[0:4]
            x_c = (x_c / 640.0) * w_img
            y_c = (y_c / 640.0) * h_img
            w = (w / 640.0) * w_img
            h = (h / 640.0) * h_img
            raw_dets.append({
                "bbox": [int(x_c - w/2), int(y_c - h/2), int(x_c + w/2), int(y_c + h/2)],
                "confidence": float(conf)
            })
    boxes = [[d["bbox"][0], d["bbox"][1], d["bbox"][2]-d["bbox"][0], d["bbox"][3]-d["bbox"][1]] for d in raw_dets]
    scores = [d["confidence"] for d in raw_dets]
    indices = cv2.dnn.NMSBoxes(boxes, scores, score_threshold=0.01, nms_threshold=0.4)
    yolov7_count = len(indices)
    t_yolov7 = time.time() - t0
    logger.info("  YOLOv7 ONNX completed in %.4f seconds (detected %d cars)", t_yolov7, yolov7_count)

    # 2. Run YOLOv11x (Ultralytics)
    models = discover_yolo_models()
    t_yolo11 = 0.0
    yolo11_count = 0
    if models:
        logger.info("Running YOLOv11x (Ultralytics)...")
        t0 = time.time()
        model = YOLO(str(models[0]))
        results = model.predict(source=str(DEMO_JPG), conf=0.25, verbose=False)
        yolo11_count = len(results[0].boxes)
        t_yolo11 = time.time() - t0
        logger.info("  YOLOv11x completed in %.4f seconds (detected %d cars)", t_yolo11, yolo11_count)

    logger.info("=" * 60)
    logger.info("DETECTOR PERFORMANCE COMPARISON")
    logger.info("=" * 60)
    logger.info("  YOLOv7 ONNX: %.4f seconds | Detections: %d", t_yolov7, yolov7_count)
    if models:
        logger.info("  YOLOv11x:    %.4f seconds | Detections: %d", t_yolo11, yolo11_count)
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
