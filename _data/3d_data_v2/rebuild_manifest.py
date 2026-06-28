"""
Rebuild the tile manifest from exported .glb files.

This is the only script that knows anything about the manifest. It:
  1. Globs every `<OUTPUT_DIR>/*/*.glb` file on disk.
  2. Derives the octree id (octant path) and file paths from each glb.
  3. Computes the lat/lon bbox via pure octree code conversion.
  4. Computes the xyz bbox + vertex/triangle counts by running a Blender script
     (glb_stats.py) on each glb.
  5. Writes everything to a Parquet manifest using pyarrow.

The manifest can be regenerated at any time straight from the .glb files on disk,
so it never needs to be produced by the download/export pipeline (main.py).

Run with:
    uv run rebuild_manifest.py
"""

import os
import glob
import json
import logging
import subprocess
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

import pyarrow as pa
import pyarrow.parquet as pq

from octree import octant_path_to_bbox

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("rebuild_manifest")

OUTPUT_DIR = "data_out"
MANIFEST_PATH = Path(OUTPUT_DIR) / "manifest.parquet"
MAX_WORKERS = 6


def get_glb_stats(glb_path: str) -> dict:
    """
    Run the Blender stats helper on a single .glb and return the parsed stats.

    Blender exits 0 even when the embedded script raises, so we detect failure by
    looking for the GLB_STATS marker (and surface tracebacks/errors otherwise).
    """
    cmd = ["blender", "-b", "-P", "glb_stats.py", "--", os.path.abspath(glb_path)]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    output = (proc.stdout or "") + (proc.stderr or "")

    for line in (proc.stdout or "").splitlines():
        if line.startswith("GLB_STATS:"):
            return json.loads(line[len("GLB_STATS:"):])

    raise RuntimeError(
        f"glb_stats.py produced no stats for {glb_path} (returncode={proc.returncode}).\n"
        f"---- blender output ----\n{output.strip()}\n------------------------"
    )


def build_row(glb_path: str) -> dict:
    """Assemble a single manifest row for one .glb file."""
    p = Path(glb_path)
    octant_path = p.stem
    depth = len(octant_path)

    bbox = octant_path_to_bbox(octant_path)
    stats = get_glb_stats(glb_path)

    file_size_bytes = p.stat().st_size
    # jpg_path = p.with_suffix(".jpg")
    xyz_min = stats["xyz_min"]
    xyz_max = stats["xyz_max"]

    return {
        "octant_path": octant_path,
        "depth": depth,
        "glb_path": str(p),
        # "jpg_path": str(jpg_path) if jpg_path.exists() else "",
        "file_size_bytes": int(file_size_bytes),
        "vertex_count": int(stats["vertex_count"]),
        "triangle_count": int(stats["triangle_count"]),
        # lat/lon bbox (degrees) from octree id conversion
        "lat_north": float(bbox.north),
        "lat_south": float(bbox.south),
        "lon_west": float(bbox.west),
        "lon_east": float(bbox.east),
        # xyz bbox (local ENU frame relative to export reference point) from the glb
        "x_min": float(xyz_min[0]),
        "y_min": float(xyz_min[1]),
        "z_min": float(xyz_min[2]),
        "x_max": float(xyz_max[0]),
        "y_max": float(xyz_max[1]),
        "z_max": float(xyz_max[2]),
    }


def main():
    glb_files = sorted(glob.glob(os.path.join(OUTPUT_DIR, "*", "*.glb")))
    logger.info(f"Found {len(glb_files)} .glb files under {OUTPUT_DIR}/")

    if not glb_files:
        logger.warning("No .glb files found; nothing to write.")
        return

    rows: list[dict] = []
    failed = 0

    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
        futures = {executor.submit(build_row, g): g for g in glb_files}
        for i, future in enumerate(as_completed(futures), start=1):
            glb = futures[future]
            try:
                row = future.result()
                rows.append(row)
                logger.info(
                    f"[{i}/{len(glb_files)}] {row['octant_path']}: "
                    f"{row['vertex_count']} verts, {row['triangle_count']} tris"
                )
            except Exception as e:
                logger.error(f"[{i}/{len(glb_files)}] Failed for {glb}: {e}")
                failed += 1

    rows.sort(key=lambda r: (r["depth"], r["octant_path"]))

    table = pa.Table.from_pylist(rows)
    pq.write_table(table, MANIFEST_PATH)

    logger.info("=" * 60)
    logger.info(f"Wrote {MANIFEST_PATH} ({len(rows)} rows, {failed} failed)")
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
