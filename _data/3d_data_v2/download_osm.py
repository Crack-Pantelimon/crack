# /// script
# dependencies = [
#     "requests[socks]",
#     "osm2geojson",
# ]
# ///

"""
OpenStreetMap Downloader Script

Downloads all OSM features (roads, buildings, water, etc.) for the bounding box
defined in data_in/zone-bbox.txt, converts them to GeoJSON format, and writes
them into separate files in data_osm/ directory.
"""

import os
import sys
import time
import json
import shutil
import logging
from pathlib import Path
import requests

# Import bbox parsing from local octree helper
from octree import parse_bbox

# Setup Logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    handlers=[logging.StreamHandler(sys.stdout)]
)
logger = logging.getLogger("download_osm")

# Configuration paths
BBOX_FILE = Path("data_in/zone-bbox.txt")
OUTPUT_DIR = Path("data_osm")
TEMP_DIR = Path("data_osm.tmp")

# Public Overpass API Interpreter endpoints
OVERPASS_ENDPOINTS = [
    "https://overpass-api.de/api/interpreter",
    "https://lz4.overpass-api.de/api/interpreter",
]

# Delay between requests to be polite to public Overpass servers
COOLDOWN_DELAY = 3.0

# Detailed categories representing all possible OSM layers/features
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
                            break # Try next endpoint
                    
                    elif response.status_code == 429:
                        sleep_time = base_delay * (2 ** (attempt - 1))
                        logger.warning(f"Rate limited (429) by Overpass API using {method_name}. Retrying in {sleep_time}s...")
                        time.sleep(sleep_time)
                        method_failed = True
                        break # Break method loop to retry next attempt
                    
                    elif response.status_code in (502, 503, 504):
                        sleep_time = base_delay * (attempt)
                        logger.warning(f"Server error ({response.status_code}) using {method_name}. Retrying in {sleep_time}s...")
                        time.sleep(sleep_time)
                        method_failed = True
                        break # Break method loop to retry next attempt
                        
                    else:
                        logger.warning(f"HTTP Error {response.status_code} using {method_name}: {response.text[:200]}")
                        # Fallback to the next method (e.g. proxy) if direct failed
                        continue
                        
                except requests.RequestException as e:
                    logger.warning(f"Network error on {endpoint} ({method_name}): {e}")
                    # Fallback to the next method (e.g. proxy) if direct failed
                    continue
            
            if not method_failed:
                sleep_time = base_delay * attempt
                logger.warning(f"All methods failed with network errors/blocks. Sleeping for {sleep_time}s before next attempt...")
                time.sleep(sleep_time)
                
        logger.warning(f"Endpoint {endpoint} failed or rate-limited. Trying next endpoint...")
        
    return None

def main():
    logger.info("Initializing OpenStreetMap downloader...")
    
    # 1. Parse bounding box coordinates
    if not BBOX_FILE.exists():
        logger.error(f"Bounding box file {BBOX_FILE} not found!")
        sys.exit(1)
        
    bbox = parse_bbox(str(BBOX_FILE))
    bbox_str = f"{bbox.south},{bbox.west},{bbox.north},{bbox.east}"
    logger.info(f"Bounding box: S={bbox.south}, W={bbox.west}, N={bbox.north}, E={bbox.east}")
    
    # Ensure directories exist
    OUTPUT_DIR.mkdir(exist_ok=True, parents=True)
    TEMP_DIR.mkdir(exist_ok=True, parents=True)
    
    # Load configuration parameters (User-Agent and SOCKS proxy) if available
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

    # Count categories and identify which ones to process
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
        logger.info("All layers are already downloaded. Nothing to do!")
        sys.exit(0)
        
    # Track performance for ETA
    active_download_times = []
    downloaded_in_this_run = 0
    remaining_count = len(categories_to_download)
    
    for idx, category_name in enumerate(categories_to_download):
        progress_str = f"[{idx + 1}/{remaining_count}]"
        logger.info(f"{progress_str} Preparing download for layer: '{category_name}'")
        
        # Calculate and display ETA
        if downloaded_in_this_run > 0:
            avg_time = sum(active_download_times) / downloaded_in_this_run
            est_remaining_time = avg_time * (remaining_count - idx)
            logger.info(f"ETA for completion: {format_eta(est_remaining_time)} (avg {avg_time:.1f}s/layer)")
        else:
            logger.info("ETA: Calculating after first download...")
            
        query_part = OSM_CATEGORIES[category_name]
        
        # Cooldown delay before request to prevent hitting rate-limits
        if idx > 0:
            logger.info(f"Sleeping for {COOLDOWN_DELAY}s (polite cooldown)...")
            time.sleep(COOLDOWN_DELAY)
            
        start_time = time.time()
        
        # Download raw OSM data
        osm_data = download_category_with_retry(query_part, bbox_str, proxies, headers)
        
        if osm_data is None:
            logger.error(f"Failed to download data for layer '{category_name}' after all attempts.")
            continue
            
        # Parse and convert to GeoJSON
        try:
            import osm2geojson
            logger.info("Converting OSM data to GeoJSON...")
            geojson_data = osm2geojson.json2geojson(osm_data)
            
            features_count = len(geojson_data.get("features", []))
            logger.info(f"Successfully converted. Found {features_count} features.")
            
            # Write to .tmp file next to final dir (inside TEMP_DIR)
            temp_path = TEMP_DIR / f"{category_name}.geojson"
            with open(temp_path, "w", encoding="utf-8") as f:
                json.dump(geojson_data, f, indent=2)
                
            # Move/Rename to final location
            final_path = OUTPUT_DIR / f"{category_name}.geojson"
            shutil.move(str(temp_path), str(final_path))
            logger.info(f"Saved layer '{category_name}' to {final_path}")
            
            # Track execution time
            elapsed = time.time() - start_time
            active_download_times.append(elapsed)
            downloaded_in_this_run += 1
            
        except Exception as e:
            logger.error(f"Error processing/saving layer '{category_name}': {e}")
            
    # Cleanup temp directory if empty
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

if __name__ == "__main__":
    main()
