# /// script
# dependencies = [
#     "opencv-python-headless",
#     "numpy",
# ]
# ///

"""YOLOv8-OBB satellite vehicle detector (ONNX), oriented bounding boxes.

Pretrained on DOTAv1 (15 classes); we keep only "large vehicle" (9) and
"small vehicle" (10). Unlike the old YOLOv7 detector this reports each car's
rotation, so parked-at-an-angle cars get a tightly wrapped quad instead of an
axis-aligned box.
"""

from __future__ import annotations

import math
from pathlib import Path

import cv2
import numpy as np

DEFAULT_ONNX = Path(__file__).parent / "yolo_models/yolov8n_obb_dota.onnx"
INPUT_SIZE = 640
VEHICLE_CLASS_IDS = {9: "large vehicle", 10: "small vehicle"}


def load_net(onnx_path: Path | str = DEFAULT_ONNX) -> cv2.dnn.Net:
    path = Path(onnx_path)
    if not path.is_file():
        raise FileNotFoundError(f"ONNX model not found: {path}")
    return cv2.dnn.readNetFromONNX(str(path))


def _rotated_corners(cx: float, cy: float, w: float, h: float, angle_rad: float) -> list[list[float]]:
    """Corners of a rotated rect, in the same TL/TR/BR/BL winding as an axis-aligned box at angle 0."""
    cos_a, sin_a = math.cos(angle_rad), math.sin(angle_rad)
    local = [(-w / 2, -h / 2), (w / 2, -h / 2), (w / 2, h / 2), (-w / 2, h / 2)]
    return [[cx + lx * cos_a - ly * sin_a, cy + lx * sin_a + ly * cos_a] for lx, ly in local]


def detect_cars(
    net: cv2.dnn.Net,
    image_bgr: np.ndarray,
    *,
    conf: float = 0.05,
    nms: float = 0.4,
) -> list[dict]:
    """Return vehicle detections as rotated pixel quads in the source image.

    conf defaults lower than a typical YOLO run: DOTA vehicles are far larger
    in their native training crops than our ~20-30px cars, so confidences run
    low even for clear true positives.
    """
    h_img, w_img = image_bgr.shape[:2]
    blob = cv2.dnn.blobFromImage(
        image_bgr, 1.0 / 255.0, (INPUT_SIZE, INPUT_SIZE), (0, 0, 0), swapRB=True, crop=False
    )
    net.setInput(blob)
    output = net.forward()[0]  # (20, 8400): [x, y, w, h, cls0..cls14, angle]
    preds = output.T  # (8400, 20)

    class_scores = preds[:, 4:19]
    class_ids = np.argmax(class_scores, axis=1)
    scores = class_scores[np.arange(len(preds)), class_ids]

    raw_detections: list[dict] = []
    for i in np.where(scores >= conf)[0]:
        class_id = int(class_ids[i])
        if class_id not in VEHICLE_CLASS_IDS:
            continue

        x_c, y_c, w, h = (float(v) for v in preds[i, 0:4])
        angle_rad = float(preds[i, 19])
        x_c = (x_c / INPUT_SIZE) * w_img
        y_c = (y_c / INPUT_SIZE) * h_img
        w = (w / INPUT_SIZE) * w_img
        h = (h / INPUT_SIZE) * h_img

        raw_detections.append(
            {
                "center": (x_c, y_c),
                "size": (w, h),
                "angle_deg": math.degrees(angle_rad),
                "confidence": float(scores[i]),
                "class_name": VEHICLE_CLASS_IDS[class_id],
            }
        )

    if not raw_detections:
        return []

    rotated_rects = [(d["center"], d["size"], d["angle_deg"]) for d in raw_detections]
    conf_scores = [d["confidence"] for d in raw_detections]
    indices = cv2.dnn.NMSBoxesRotated(rotated_rects, conf_scores, score_threshold=0.01, nms_threshold=nms)
    if len(indices) == 0:
        return []
    if isinstance(indices, np.ndarray):
        indices = indices.flatten()

    results = []
    for idx in indices:
        d = raw_detections[int(idx)]
        cx, cy = d["center"]
        w, h = d["size"]
        corners = _rotated_corners(cx, cy, w, h, math.radians(d["angle_deg"]))
        xs = [c[0] for c in corners]
        ys = [c[1] for c in corners]
        results.append(
            {
                "corners_pixel": corners,
                "bbox_pixel": [int(min(xs)), int(min(ys)), int(max(xs)), int(max(ys))],
                "angle_deg": d["angle_deg"],
                "confidence": d["confidence"],
                "class_name": d["class_name"],
            }
        )
    return results
