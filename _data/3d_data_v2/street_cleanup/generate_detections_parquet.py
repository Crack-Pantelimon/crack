import sys
from pathlib import Path
import json
import os
import time
import subprocess
from concurrent.futures import ThreadPoolExecutor, as_completed
import logging
import cv2
import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq

# Set up logging
logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("generate_parquet")

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from street_cleanup.coord_utils import latlon_to_enu
from street_cleanup.road_tiles import list_road_d20_tiles, glb_path_for_octant

# Paths
ROOT_DIR = Path("/home/vasile/.gemini/antigravity/scratch/crack/_data/3d_data_v2")
ONNX_PATH = ROOT_DIR / "street_cleanup" / "yolov7-m_itcvd_qgis.onnx"
NATURAL_GEOJSON = ROOT_DIR / "data_osm" / "natural.geojson"
OUTPUT_PARQUET = ROOT_DIR / "detections.parquet"
RENDERS_DIR = ROOT_DIR / "street_cleanup" / "renders"

BLENDER_BATCH_SIZE = 32
MAX_WORKERS = 12

def run_blender_batch(batch: list[dict]) -> tuple[int, int]:
    batch_json_path = f"/tmp/blender_batch_{batch[0]['octant_path']}.json"
    with open(batch_json_path, "w") as f:
        json.dump({"tiles": batch}, f)

    cmd = [
        "blender",
        "-b",
        "-P",
        str(ROOT_DIR / "street_cleanup" / "render_top_down.py"),
        "--",
        batch_json_path
    ]
    
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, check=True)
        # Parse output for success/fail logs
        ok_count = 0
        fail_count = 0
        for line in proc.stdout.splitlines():
            if "RENDER_FAIL" in line:
                fail_count += 1
            elif "Saved:" in line:
                ok_count += 1
        # If no specific log counts but subprocess was successful
        if ok_count == 0 and fail_count == 0:
            ok_count = len(batch)
        return ok_count, fail_count
    except subprocess.CalledProcessError as err:
        logger.error(f"Blender batch execution failed: {err.stderr}")
        return 0, len(batch)
    finally:
        if os.path.exists(batch_json_path):
            os.remove(batch_json_path)

def load_all_trees() -> list[tuple[float, float]]:
    if not NATURAL_GEOJSON.exists():
        logger.warning(f"OSM trees file not found: {NATURAL_GEOJSON}")
        return []
    with open(NATURAL_GEOJSON, "r", encoding="utf-8") as f:
        data = json.load(f)
    trees = []
    for feat in data.get("features", []):
        tags = feat.get("properties", {}).get("tags", {})
        if tags.get("natural") == "tree":
            geom = feat.get("geometry", {})
            if geom.get("type") == "Point":
                lon, lat = geom["coordinates"]
                trees.append((lat, lon))
    logger.info(f"Loaded {len(trees)} trees from OSM natural.geojson.")
    return trees

def latlon_to_pixel(lat: float, lon: float, width: int, height: int, lat_lon_bbox: dict) -> tuple[float, float]:
    lon_west = lat_lon_bbox["lon_west"]
    lon_east = lat_lon_bbox["lon_east"]
    lat_north = lat_lon_bbox["lat_north"]
    lat_south = lat_lon_bbox["lat_south"]
    
    u = (lon - lon_west) / (lon_east - lon_west) if lon_east != lon_west else 0.5
    v = (lat_north - lat) / (lat_north - lat_south) if lat_north != lat_south else 0.5
    
    px = u * width
    py = v * height
    return px, py

def run_yolov7_onnx(image: np.ndarray, net, conf_threshold=0.20) -> list[dict]:
    h_img, w_img, _ = image.shape
    blob = cv2.dnn.blobFromImage(image, 1.0 / 255.0, (640, 640), (0, 0, 0), swapRB=True, crop=False)
    net.setInput(blob)
    outputs = net.forward()
    predictions = outputs[0]
    raw_detections = []
    for pred in predictions:
        obj_conf = pred[4]
        class_prob = pred[5]
        confidence = obj_conf * class_prob
        if confidence >= conf_threshold:
            x_c, y_c, w, h = pred[0:4]
            x_c = (x_c / 640.0) * w_img
            y_c = (y_c / 640.0) * h_img
            w = (w / 640.0) * w_img
            h = (h / 640.0) * h_img
            x1 = int(x_c - w / 2)
            y1 = int(y_c - h / 2)
            x2 = int(x_c + w / 2)
            y2 = int(y_c + h / 2)
            raw_detections.append({
                "bbox": [x1, y1, x2, y2],
                "confidence": float(confidence)
            })
    # NMS
    boxes = [[d["bbox"][0], d["bbox"][1], d["bbox"][2]-d["bbox"][0], d["bbox"][3]-d["bbox"][1]] for d in raw_detections]
    scores = [d["confidence"] for d in raw_detections]
    indices = cv2.dnn.NMSBoxes(boxes, scores, score_threshold=0.01, nms_threshold=0.4)
    if len(indices) > 0:
        if isinstance(indices, np.ndarray):
            indices = indices.flatten()
        return [raw_detections[idx] for idx in indices]
    return []

def main():
    logger.info("=" * 60)
    logger.info("Global Map Detections Generator")
    logger.info("=" * 60)

    # 1. Load trees
    trees = load_all_trees()

    # 2. Get list of existing road tiles
    road_tiles = list_road_d20_tiles(only_existing=True)
    logger.info(f"Total existing road tiles: {len(road_tiles)}")

    # 3. Filter to render specs
    render_specs = []
    for t in road_tiles:
        last_three = t.octant_path[-3:]
        jpg_path = RENDERS_DIR / last_three / f"{t.octant_path}.jpg"
        meta_path = RENDERS_DIR / last_three / f"{t.octant_path}.json"
        
        render_specs.append({
            "octant_path": t.octant_path,
            "glb_path": str(t.glb_path),
            "jpg_path": str(jpg_path),
            "meta_path": str(meta_path),
            "resolution": [128, 128],
            "lat_lon_bbox": {
                "lat_north": t.lat_north,
                "lat_south": t.lat_south,
                "lon_east": t.lon_east,
                "lon_west": t.lon_west,
            }
        })

    # 4. Render missing tiles
    todo_renders = []
    for spec in render_specs:
        jpg_p = Path(spec["jpg_path"])
        meta_p = Path(spec["meta_path"])
        if not (jpg_p.exists() and meta_p.exists()):
            todo_renders.append(spec)

    logger.info(f"Already rendered: {len(render_specs) - len(todo_renders)}")
    logger.info(f"Remaining to render: {len(todo_renders)}")

    if todo_renders:
        batches = [todo_renders[i : i + BLENDER_BATCH_SIZE] for i in range(0, len(todo_renders), BLENDER_BATCH_SIZE)]
        logger.info(f"Rendering remaining tiles in {len(batches)} parallel batches...")
        total_ok = 0
        total_fail = 0
        t0 = time.time()
        with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
            futures = {executor.submit(run_blender_batch, batch): batch for batch in batches}
            for i, future in enumerate(as_completed(futures), start=1):
                ok, fail = future.result()
                total_ok += ok
                total_fail += fail
                logger.info(f"Blender Progress: {i}/{len(batches)} batches | Rendered OK: {total_ok}, Failed: {total_fail}")
                sys.stdout.flush()
        logger.info(f"Blender rendering completed in {time.time() - t0:.1f}s.")
        sys.stdout.flush()

    # 5. Load YOLOv7 model
    if not ONNX_PATH.exists():
        logger.error(f"YOLOv7 ONNX model weights not found at: {ONNX_PATH}")
        sys.exit(1)
    logger.info(f"Loading YOLOv7 ONNX model from: {ONNX_PATH.name}")
    
    # Run sequential inference & OSM projections to populate columns
    parquet_data = {
        "octtree_id": [],
        "resolution_x": [],
        "resolution_y": [],
        "bbox_x0": [],
        "bbox_x1": [],
        "bbox_y0": [],
        "bbox_y1": [],
        "class": []
    }

    net = cv2.dnn.readNetFromONNX(str(ONNX_PATH))
    t0_inf = time.time()
    
    # Process tiles
    logger.info("Running detection pipeline on all renders...")
    for idx, spec in enumerate(render_specs, start=1):
        jpg_p = Path(spec["jpg_path"])
        meta_p = Path(spec["meta_path"])
        
        if not (jpg_p.exists() and meta_p.exists()):
            continue
            
        with open(meta_p, "r", encoding="utf-8") as f:
            meta = json.load(f)
            
        width, height = meta["resolution"]
        lat_lon_bbox = meta["lat_lon_bbox"]
        octant_path = spec["octant_path"]

        # Run YOLO detection for cars
        img = cv2.imread(str(jpg_p))
        if img is None:
            continue
            
        car_dets = run_yolov7_onnx(img, net, conf_threshold=0.20)
        for det in car_dets:
            x1, y1, x2, y2 = det["bbox"]
            parquet_data["octtree_id"].append(octant_path)
            parquet_data["resolution_x"].append(width)
            parquet_data["resolution_y"].append(height)
            parquet_data["bbox_x0"].append(float(x1))
            parquet_data["bbox_x1"].append(float(x2))
            parquet_data["bbox_y0"].append(float(y1))
            parquet_data["bbox_y1"].append(float(y2))
            parquet_data["class"].append("car")

        # Project trees onto this tile
        for lat, lon in trees:
            px, py = latlon_to_pixel(lat, lon, width, height, lat_lon_bbox)
            if 0.0 <= px <= float(width) and 0.0 <= py <= float(height):
                # Calculate pixel bbox for tree (4m diameter ~ 60x60 pixels in 1024px zoom)
                bx0 = max(0.0, px - 3.75)
                bx1 = min(float(width), px + 3.75)
                by0 = max(0.0, py - 3.75)
                by1 = min(float(height), py + 3.75)
                
                parquet_data["octtree_id"].append(octant_path)
                parquet_data["resolution_x"].append(width)
                parquet_data["resolution_y"].append(height)
                parquet_data["bbox_x0"].append(float(bx0))
                parquet_data["bbox_x1"].append(float(bx1))
                parquet_data["bbox_y0"].append(float(by0))
                parquet_data["bbox_y1"].append(float(by1))
                parquet_data["class"].append("tree")

        if idx % 50 == 0 or idx == len(render_specs):
            logger.info(f"Processed detections for {idx}/{len(render_specs)} tiles...")
            sys.stdout.flush()

    logger.info(f"Detection processing completed in {time.time() - t0_inf:.1f}s.")
    sys.stdout.flush()

    # 6. Save as Parquet file
    logger.info(f"Writing Parquet table with {len(parquet_data['octtree_id'])} detections to {OUTPUT_PARQUET}...")
    sys.stdout.flush()
    table = pa.Table.from_pydict(parquet_data)
    pq.write_table(table, str(OUTPUT_PARQUET))
    logger.info("Parquet table generated successfully.")
    sys.stdout.flush()

if __name__ == "__main__":
    main()
