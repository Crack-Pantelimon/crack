# /// script
# dependencies = [
#     "requests[socks]",
#     "osm2geojson",
#     "pyarrow",
# ]
# ///

"""
OpenStreetMap Downloader Script

Stage 1: Downloads all OSM features (roads, buildings, water, etc.) for the
bounding box defined in data_in/zone-bbox.txt, converts them to GeoJSON format,
and writes them into separate files in data_osm/original/.

Stage 2: Cross-references every OSM feature against the photogrammetry octree
(from data_out/manifest.parquet) and emits features.parquet and
octtree_features.parquet under data_osm/.
"""

import os
import sys
import time
import json
import shutil
import logging
from pathlib import Path
from collections import defaultdict

import pyarrow as pa
import pyarrow.parquet as pq
import requests

from octree import parse_bbox

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)]
)
logger = logging.getLogger("download_osm")

BBOX_FILE = Path("data_in/zone-bbox.txt")
OUTPUT_DIR = Path("data_osm/original")
DATA_OSM_DIR = Path("data_osm")
TEMP_DIR = Path("data_osm.tmp")
MANIFEST_PATH = Path("data_out/manifest.parquet")
FEATURES_PATH = DATA_OSM_DIR / "features.parquet"
OCTTREE_FEATURES_PATH = DATA_OSM_DIR / "octtree_features.parquet"

MAX_TILE_DEPTH = 20

OVERPASS_ENDPOINTS = [
    "https://overpass-api.de/api/interpreter",
    "https://lz4.overpass-api.de/api/interpreter",
]

COOLDOWN_DELAY = 3.0

OSM_CATEGORIES = {
    "buildings": 'nwr["building"]',
    "roads": 'nwr["highway"]',
    "waterways": 'nwr["waterway"]',
    "natural": 'nwr["natural"]',
    "landuse": 'nwr["landuse"]',
    "leisure": 'nwr["leisure"]',
    "railways": 'nwr["railway"]',
    "aeroways": 'nwr["aeroway"]',
    "amenities": 'nwr["amenity"]',
    "shops": 'nwr["shop"]',
    "tourism": 'nwr["tourism"]',
    "historic": 'nwr["historic"]',
    "boundaries": 'nwr["boundary"]',
    "man_made": 'nwr["man_made"]',
    "offices": 'nwr["office"]',
    "public_transport": 'nwr["public_transport"]',
    "power": 'nwr["power"]',
    "barriers": 'nwr["barrier"]',
    "places": 'nwr["place"]',
    "routes": 'nwr["route"]',
    "emergency": 'nwr["emergency"]',
    "military": 'nwr["military"]',
    "telecom": 'nwr["telecom"]',
    "craft": 'nwr["craft"]',
    "geological": 'nwr["geological"]',
    "aerialways": 'nwr["aerialway"]',
}


def format_eta(seconds: float) -> str:
    """Format seconds into a human-readable ETA string."""
    if seconds < 60:
        return f"{int(seconds)}s"
    minutes = int(seconds // 60)
    secs = int(seconds % 60)
    if minutes < 60:
        return f"{minutes}m {secs}s"
    hours = int(minutes // 60)
    mins = int(minutes % 60)
    return f"{hours}h {mins}m {secs}s"


def download_category_with_retry(query_part: str, bbox_str: str, proxies: dict, headers: dict) -> dict | None:
    """Download OSM data for a query part from public Overpass API with retries and failover."""
    query = f"""[out:json][timeout:180];
(
  {query_part}({bbox_str});
);
out geom;"""

    for endpoint in OVERPASS_ENDPOINTS:
        max_attempts = 5
        base_delay = 5.0

        for attempt in range(1, max_attempts + 1):
            try_methods = [("direct", {})]
            if proxies:
                try_methods.append(("proxy", proxies))

            method_failed = False
            for method_name, proxy_cfg in try_methods:
                try:
                    logger.info(f"Connecting to {endpoint} ({method_name}) (Attempt {attempt}/{max_attempts})...")
                    response = requests.post(
                        endpoint,
                        data={"data": query},
                        headers=headers,
                        proxies=proxy_cfg,
                        timeout=200
                    )

                    if response.status_code == 200:
                        try:
                            return response.json()
                        except json.JSONDecodeError:
                            logger.error(f"Failed to parse JSON response from {endpoint}")
                            break

                    elif response.status_code == 429:
                        sleep_time = base_delay * (2 ** (attempt - 1))
                        logger.warning(f"Rate limited (429) by Overpass API using {method_name}. Retrying in {sleep_time}s...")
                        time.sleep(sleep_time)
                        method_failed = True
                        break

                    elif response.status_code in (502, 503, 504):
                        sleep_time = base_delay * (attempt)
                        logger.warning(f"Server error ({response.status_code}) using {method_name}. Retrying in {sleep_time}s...")
                        time.sleep(sleep_time)
                        method_failed = True
                        break

                    else:
                        logger.warning(f"HTTP Error {response.status_code} using {method_name}: {response.text[:200]}")
                        continue

                except requests.RequestException as e:
                    logger.warning(f"Network error on {endpoint} ({method_name}): {e}")
                    continue

            if not method_failed:
                sleep_time = base_delay * attempt
                logger.warning(f"All methods failed with network errors/blocks. Sleeping for {sleep_time}s before next attempt...")
                time.sleep(sleep_time)

        logger.warning(f"Endpoint {endpoint} failed or rate-limited. Trying next endpoint...")

    return None


def download_all():
    """Stage 1: download OSM layers into data_osm/original/."""
    logger.info("Initializing OpenStreetMap downloader...")

    if not BBOX_FILE.exists():
        logger.error(f"Bounding box file {BBOX_FILE} not found!")
        sys.exit(1)

    bbox = parse_bbox(str(BBOX_FILE))
    bbox_str = f"{bbox.south},{bbox.west},{bbox.north},{bbox.east}"
    logger.info(f"Bounding box: S={bbox.south}, W={bbox.west}, N={bbox.north}, E={bbox.east}")

    OUTPUT_DIR.mkdir(exist_ok=True, parents=True)
    TEMP_DIR.mkdir(exist_ok=True, parents=True)

    user_agent = "3DDataV2OSMImporter/1.0 (contact@example.com)"
    proxies = {}

    try:
        import config
        if hasattr(config, "SOCKS_PROXY") and config.SOCKS_PROXY:
            proxies = {
                "http": config.SOCKS_PROXY,
                "https": config.SOCKS_PROXY
            }
            logger.info(f"Using SOCKS proxy: {config.SOCKS_PROXY}")
    except ImportError:
        logger.warning("Could not import config.py. Using direct connection.")

    headers = {
        "User-Agent": user_agent,
        "Accept": "application/json"
    }

    total_categories = len(OSM_CATEGORIES)
    categories_to_download = []
    skipped_count = 0

    for category_name in OSM_CATEGORIES:
        final_path = OUTPUT_DIR / f"{category_name}.geojson"
        if final_path.exists():
            skipped_count += 1
        else:
            categories_to_download.append(category_name)

    logger.info(f"Total categories: {total_categories}. Already downloaded: {skipped_count}. To download: {len(categories_to_download)}.")

    if not categories_to_download:
        logger.info("All layers are already downloaded. Skipping stage 1.")
        return

    active_download_times = []
    downloaded_in_this_run = 0
    remaining_count = len(categories_to_download)

    for idx, category_name in enumerate(categories_to_download):
        progress_str = f"[{idx + 1}/{remaining_count}]"
        logger.info(f"{progress_str} Preparing download for layer: '{category_name}'")

        if downloaded_in_this_run > 0:
            avg_time = sum(active_download_times) / downloaded_in_this_run
            est_remaining_time = avg_time * (remaining_count - idx)
            logger.info(f"ETA for completion: {format_eta(est_remaining_time)} (avg {avg_time:.1f}s/layer)")
        else:
            logger.info("ETA: Calculating after first download...")

        query_part = OSM_CATEGORIES[category_name]

        if idx > 0:
            logger.info(f"Sleeping for {COOLDOWN_DELAY}s (polite cooldown)...")
            time.sleep(COOLDOWN_DELAY)

        start_time = time.time()

        osm_data = download_category_with_retry(query_part, bbox_str, proxies, headers)

        if osm_data is None:
            logger.error(f"Failed to download data for layer '{category_name}' after all attempts.")
            continue

        try:
            import osm2geojson
            logger.info("Converting OSM data to GeoJSON...")
            geojson_data = osm2geojson.json2geojson(osm_data)

            features_count = len(geojson_data.get("features", []))
            logger.info(f"Successfully converted. Found {features_count} features.")

            temp_path = TEMP_DIR / f"{category_name}.geojson"
            with open(temp_path, "w", encoding="utf-8") as f:
                json.dump(geojson_data, f, indent=2)

            final_path = OUTPUT_DIR / f"{category_name}.geojson"
            shutil.move(str(temp_path), str(final_path))
            logger.info(f"Saved layer '{category_name}' to {final_path}")

            elapsed = time.time() - start_time
            active_download_times.append(elapsed)
            downloaded_in_this_run += 1

        except Exception as e:
            logger.error(f"Error processing/saving layer '{category_name}': {e}")

    try:
        if TEMP_DIR.exists() and not os.listdir(TEMP_DIR):
            TEMP_DIR.rmdir()
    except Exception as e:
        logger.warning(f"Could not remove temporary directory: {e}")

    logger.info("=" * 60)
    logger.info("DOWNLOAD COMPLETED!")
    logger.info(f"  Processed: {downloaded_in_this_run} layers")
    logger.info(f"  Skipped: {skipped_count} layers")
    logger.info(f"  Output directory: {OUTPUT_DIR}")
    logger.info("=" * 60)


def load_octree_index():
    """Load octree tile bboxes and parent/child relationships from the manifest."""
    if not MANIFEST_PATH.exists():
        logger.error(f"Manifest not found at {MANIFEST_PATH}")
        sys.exit(1)

    table = pq.read_table(
        MANIFEST_PATH,
        columns=["octant_path", "lat_north", "lat_south", "lon_west", "lon_east"],
    )

    bboxes: dict[str, tuple[float, float, float, float]] = {}
    children: dict[str, list[str]] = defaultdict(list)

    for row in table.to_pylist():
        path = row["octant_path"]
        if len(path) > MAX_TILE_DEPTH:
            continue
        bboxes[path] = (
            row["lat_north"],
            row["lat_south"],
            row["lon_west"],
            row["lon_east"],
        )

    for path in bboxes:
        parent = path[:-1]
        if parent in bboxes:
            children[parent].append(path)

    roots = [p for p in bboxes if p[:-1] not in bboxes]

    logger.info(f"Loaded octree index: {len(bboxes)} tiles, {len(roots)} root(s)")
    return bboxes, children, roots


def _flatten_coords(coords):
    """Recursively yield (lon, lat) from nested GeoJSON coordinate arrays."""
    if not coords:
        return
    if isinstance(coords[0], (int, float)):
        yield coords[0], coords[1]
    else:
        for item in coords:
            yield from _flatten_coords(item)


def iter_lonlat(geometry):
    """Yield (lon, lat) from any GeoJSON geometry."""
    if geometry is None:
        return
    if geometry.get("type") == "GeometryCollection":
        for sub_geom in geometry.get("geometries", []):
            yield from iter_lonlat(sub_geom)
    else:
        yield from _flatten_coords(geometry.get("coordinates"))


def collect_octant_paths(points, path, bboxes, children):
    """Recursively assign points to octant paths using half-open bbox containment."""
    n, s, w, e = bboxes[path]
    inside = [(lon, lat) for lon, lat in points if s <= lat < n and w <= lon < e]
    if not inside:
        return []

    result = [path]
    for child in children.get(path, ()):
        result.extend(collect_octant_paths(inside, child, bboxes, children))
    return result


def assign_feature_paths(geometry, bboxes, children, roots):
    """Return the set of octant paths that contain any point of the feature."""
    points = list(iter_lonlat(geometry))
    if not points:
        return set()

    paths: set[str] = set()
    for root in roots:
        paths.update(collect_octant_paths(points, root, bboxes, children))
    return paths


def build_feature_manifests():
    """Stage 2: build features.parquet and octtree_features.parquet."""
    logger.info("Building feature manifests (stage 2)...")

    bboxes, children, roots = load_octree_index()

    feature_rows: list[tuple] = []
    octtree_rows: list[tuple] = []

    geojson_files = sorted(OUTPUT_DIR.glob("*.geojson"))
    if not geojson_files:
        logger.error(f"No GeoJSON files found in {OUTPUT_DIR}")
        sys.exit(1)

    for geojson_path in geojson_files:
        collection_name = geojson_path.stem
        with open(geojson_path, encoding="utf-8") as f:
            data = json.load(f)

        for feature in data.get("features", []):
            props = feature.get("properties", {})
            feature_type = props.get("type")
            feature_id = props.get("id")
            geometry = feature.get("geometry")
            geometry_type = geometry.get("type") if geometry else None
            feature_json = json.dumps(feature, ensure_ascii=False)

            feature_rows.append((
                collection_name,
                feature_type,
                feature_id,
                geometry_type,
                feature_json,
            ))

            for path in assign_feature_paths(geometry, bboxes, children, roots):
                octtree_rows.append((
                    path,
                    collection_name,
                    feature_type,
                    feature_id,
                ))

    feature_rows.sort(key=lambda r: (r[0], r[1], r[2], r[3]))
    octtree_rows.sort(key=lambda r: (r[0], r[1], r[2], r[3]))

    DATA_OSM_DIR.mkdir(exist_ok=True, parents=True)

    features_table = pa.Table.from_pydict({
        "collection_name": [r[0] for r in feature_rows],
        "feature_type": [r[1] for r in feature_rows],
        "feature_id": [r[2] for r in feature_rows],
        "geometry_type": [r[3] for r in feature_rows],
        "feature_json": [r[4] for r in feature_rows],
    })
    pq.write_table(features_table, FEATURES_PATH)

    octtree_table = pa.Table.from_pydict({
        "octtree_path": [r[0] for r in octtree_rows],
        "collection_name": [r[1] for r in octtree_rows],
        "feature_type": [r[2] for r in octtree_rows],
        "feature_id": [r[3] for r in octtree_rows],
    })
    pq.write_table(octtree_table, OCTTREE_FEATURES_PATH)

    logger.info("=" * 60)
    logger.info("FEATURE MANIFESTS COMPLETED!")
    logger.info(f"  Features: {len(feature_rows)} rows -> {FEATURES_PATH}")
    logger.info(f"  Octtree assignments: {len(octtree_rows)} rows -> {OCTTREE_FEATURES_PATH}")
    logger.info("=" * 60)


def main():
    download_all()
    build_feature_manifests()


if __name__ == "__main__":
    main()
