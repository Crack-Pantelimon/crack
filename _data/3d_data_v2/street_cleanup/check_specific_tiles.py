import sys
import os
import json
import time
from pathlib import Path
import cv2
import numpy as np
import subprocess

ROOT_DIR = Path("/home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2")
sys.path.insert(0, str(ROOT_DIR))
from street_cleanup.road_tiles import glb_path_for_octant

TILES = [
    "30436272704361714633",
    "30436272704361725431",
    "30436272704361707630"
]

ONNX_PATH = ROOT_DIR / "street_cleanup" / "yolov7-m_itcvd_qgis.onnx"

def main():
    print("Checking specific tiles:")
    # Setup specs
    specs = []
    for t_id in TILES:
        glb_path = glb_path_for_octant(t_id)
        last_three = t_id[-3:]
        jpg_path = ROOT_DIR / "street_cleanup" / "renders" / last_three / f"{t_id}.jpg"
        meta_path = ROOT_DIR / "street_cleanup" / "renders" / last_three / f"{t_id}.json"
        
        specs.append({
            "octant_path": t_id,
            "glb_path": str(glb_path),
            "jpg_path": str(jpg_path),
            "meta_path": str(meta_path),
            "resolution": [128, 128],
            "lat_lon_bbox": {
                "lat_north": 0.0,  # placeholder
                "lat_south": 0.0,
                "lon_east": 0.0,
                "lon_west": 0.0
            }
        })
        
    # Write batch
    batch_json = "/tmp/specific_tiles_batch.json"
    with open(batch_json, "w") as f:
        json.dump({"tiles": specs}, f)
        
    # Render using Blender
    print("Rendering tiles using Blender...")
    cmd = [
        "blender",
        "-b",
        "-P",
        str(ROOT_DIR / "street_cleanup" / "render_top_down.py"),
        "--",
        batch_json
    ]
    subprocess.run(cmd, check=True)
    
    # Load YOLOv7
    print("Loading YOLOv7 ONNX model...")
    net = cv2.dnn.readNetFromONNX(str(ONNX_PATH))
    
    for spec in specs:
        t_id = spec["octant_path"]
        jpg_p = Path(spec["jpg_path"])
        if not jpg_p.exists():
            print(f"Tile {t_id}: Render failed (no image created)")
            continue
            
        img = cv2.imread(str(jpg_p))
        if img is None:
            print(f"Tile {t_id}: Failed to load rendered image")
            continue
            
        h_img, w_img, _ = img.shape
        blob = cv2.dnn.blobFromImage(img, 1.0 / 255.0, (640, 640), (0, 0, 0), swapRB=True, crop=False)
        net.setInput(blob)
        preds = net.forward()[0]
        
        raw_dets = []
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
                
        # NMS
        boxes = [[d["bbox"][0], d["bbox"][1], d["bbox"][2]-d["bbox"][0], d["bbox"][3]-d["bbox"][1]] for d in raw_dets]
        scores = [d["confidence"] for d in raw_dets]
        indices = cv2.dnn.NMSBoxes(boxes, scores, score_threshold=0.01, nms_threshold=0.4)
        
        print(f"\nTile {t_id}:")
        if len(indices) == 0:
            print("  No vehicles detected.")
        else:
            if isinstance(indices, np.ndarray):
                indices = indices.flatten()
            for idx in indices:
                det = raw_dets[idx]
                x1, y1, x2, y2 = det["bbox"]
                print(f"  - car (confidence: {det['confidence']:.2f}) | bbox: [{x1}, {y1}, {x2}, {y2}]")
                
    if os.path.exists(batch_json):
        os.remove(batch_json)

if __name__ == "__main__":
    main()
