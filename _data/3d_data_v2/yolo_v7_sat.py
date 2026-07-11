"""YOLOv7 satellite vehicle detector (ONNX)."""

from __future__ import annotations

from pathlib import Path

import cv2
import numpy as np

DEFAULT_ONNX = Path(__file__).parent / "yolo_models/yolov7-m_itcvd_qgis.onnx"
INPUT_SIZE = 640


def load_net(onnx_path: Path | str = DEFAULT_ONNX) -> cv2.dnn.Net:
    path = Path(onnx_path)
    if not path.is_file():
        raise FileNotFoundError(f"ONNX model not found: {path}")
    return cv2.dnn.readNetFromONNX(str(path))


def detect_cars(
    net: cv2.dnn.Net,
    image_bgr: np.ndarray,
    *,
    conf: float = 0.20,
    nms: float = 0.4,
) -> list[dict]:
    """Return car detections as pixel bboxes in the source image."""
    h_img, w_img = image_bgr.shape[:2]
    blob = cv2.dnn.blobFromImage(
        image_bgr, 1.0 / 255.0, (INPUT_SIZE, INPUT_SIZE), (0, 0, 0), swapRB=True, crop=False
    )
    net.setInput(blob)
    outputs = net.forward()
    predictions = outputs[0]

    raw_detections: list[dict] = []
    for pred in predictions:
        obj_conf = pred[4]
        class_prob = pred[5]
        confidence = float(obj_conf * class_prob)
        if confidence < conf:
            continue

        x_c, y_c, w, h = pred[0:4]
        x_c = (x_c / INPUT_SIZE) * w_img
        y_c = (y_c / INPUT_SIZE) * h_img
        w = (w / INPUT_SIZE) * w_img
        h = (h / INPUT_SIZE) * h_img
        x1 = int(x_c - w / 2)
        y1 = int(y_c - h / 2)
        x2 = int(x_c + w / 2)
        y2 = int(y_c + h / 2)
        raw_detections.append(
            {
                "bbox_pixel": [x1, y1, x2, y2],
                "confidence": confidence,
            }
        )

    if not raw_detections:
        return []

    boxes = [
        [
            d["bbox_pixel"][0],
            d["bbox_pixel"][1],
            d["bbox_pixel"][2] - d["bbox_pixel"][0],
            d["bbox_pixel"][3] - d["bbox_pixel"][1],
        ]
        for d in raw_detections
    ]
    scores = [d["confidence"] for d in raw_detections]
    indices = cv2.dnn.NMSBoxes(boxes, scores, score_threshold=0.01, nms_threshold=nms)
    if len(indices) == 0:
        return []

    if isinstance(indices, np.ndarray):
        indices = indices.flatten()
    return [raw_detections[int(idx)] for idx in indices]
