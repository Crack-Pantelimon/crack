"""
Manifest writer for tile export results.
"""

import json
from pathlib import Path


def write_manifest(
    tiles: list[dict],
    bbox: dict,
    level: int,
    reference_point: list[float],
    output_dir: str,
):
    """
    Write the manifest JSON file to the output directory.

    Args:
        tiles: List of tile metadata dicts
        bbox: Bounding box dict with north/south/west/east
        level: Octree level used
        reference_point: ECEF reference point [x, y, z] subtracted from all positions
        output_dir: Directory to write manifest.json to
    """
    manifest = {
        "bbox": bbox,
        "octree_level": level,
        "tile_count": len(tiles),
        "reference_point_ecef": reference_point,
        "tiles": tiles,
    }

    manifest_path = Path(output_dir) / "manifest.json"
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"Wrote manifest: {manifest_path} ({len(tiles)} tiles)")
