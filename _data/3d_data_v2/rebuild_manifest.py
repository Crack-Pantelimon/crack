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
import logging
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

import pyarrow as pa
import pyarrow.parquet as pq
from pygltflib import GLTF2

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
    Extract stats from a single .glb file using pygltflib.
    """
    gltf = GLTF2().load(glb_path)

    vertex_count = 0
    triangle_count = 0
    xyz_min = [float("inf"), float("inf"), float("inf")]
    xyz_max = [float("-inf"), float("-inf"), float("-inf")]

    for mesh in gltf.meshes:
        for primitive in mesh.primitives:
            pos_accessor_idx = primitive.attributes.POSITION
            if pos_accessor_idx is not None:
                accessor = gltf.accessors[pos_accessor_idx]
                vertex_count += accessor.count
                if accessor.min is not None and accessor.max is not None:
                    for i in range(3):
                        if accessor.min[i] < xyz_min[i]:
                            xyz_min[i] = accessor.min[i]
                        if accessor.max[i] > xyz_max[i]:
                            xyz_max[i] = accessor.max[i]

            if primitive.indices is not None:
                index_accessor = gltf.accessors[primitive.indices]
                triangle_count += index_accessor.count // 3
            elif pos_accessor_idx is not None:
                triangle_count += accessor.count // 3

    if vertex_count == 0:
        xyz_min = [0.0, 0.0, 0.0]
        xyz_max = [0.0, 0.0, 0.0]

    return {
        "vertex_count": vertex_count,
        "triangle_count": triangle_count,
        "mesh_count": len(gltf.meshes),
        "xyz_min": [float(v) for v in xyz_min],
        "xyz_max": [float(v) for v in xyz_max],
    }


def build_row(glb_path: str) -> dict:
    """Assemble a single manifest row for one .glb file."""
    p = Path(glb_path)
    octant_path = p.stem
    depth = len(octant_path)

    bbox = octant_path_to_bbox(octant_path)
    stats = get_glb_stats(glb_path)

    file_size_bytes = p.stat().st_size
    xyz_min = stats["xyz_min"]
    xyz_max = stats["xyz_max"]

    return {
        "octant_path": octant_path,
        "depth": depth,
        "glb_path": str(p),
        "file_size_bytes": int(file_size_bytes),
        "vertex_count": int(stats["vertex_count"]),
        "triangle_count": int(stats["triangle_count"]),
        "mesh_count": int(stats["mesh_count"]),
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
    glb_files = sorted(glob.glob(os.path.join(OUTPUT_DIR, "*", "*", "*.glb")))
    logger.info(f"Found {len(glb_files)} .glb files under {OUTPUT_DIR}/")

    if not glb_files:
        logger.warning("No .glb files found; nothing to write.")
        return

    # Load existing manifest if it exists
    rows: list[dict] = []
    existing_octants = set()
    if os.path.exists(MANIFEST_PATH):
        try:
            table = pq.read_table(MANIFEST_PATH)
            rows = table.to_pylist()
            existing_octants = {r["octant_path"] for r in rows}
            logger.info(f"Loaded {len(existing_octants)} existing records from {MANIFEST_PATH}")
        except Exception as e:
            logger.warning(f"Could not read existing manifest {MANIFEST_PATH}: {e}. Starting fresh.")
            rows = []

    glb_files_to_process = []
    for g in glb_files:
        octant_path = Path(g).stem
        if octant_path not in existing_octants:
            glb_files_to_process.append(g)

    logger.info(f"Need to process {len(glb_files_to_process)} new / remaining .glb files")

    if not glb_files_to_process:
        logger.info("All files are already processed in the manifest.")
        return

    BATCH_SIZE = 1024 * 4
    # Chunk the remaining files into batches of 32
    batches = [glb_files_to_process[i:i + BATCH_SIZE] for i in range(0, len(glb_files_to_process), BATCH_SIZE)]
    failed = 0

    for batch_idx, batch in enumerate(batches, start=1):
        batch_rows = []
        with ThreadPoolExecutor(max_workers=MAX_WORKERS) as executor:
            futures = {executor.submit(build_row, g): g for g in batch}
            for future in as_completed(futures):
                glb = futures[future]
                try:
                    row = future.result()
                    batch_rows.append(row)
                except Exception as e:
                    logger.error(f"Failed for {glb}: {e}")
                    failed += 1

        if batch_rows:
            rows.extend(batch_rows)
            # Sort full list by depth and octant path before writing
            rows.sort(key=lambda r: (r["depth"], r["octant_path"]))

            # Write full manifest to disk
            table = pa.Table.from_pylist(rows)
            pq.write_table(table, MANIFEST_PATH)

            # Sync file to disk
            try:
                fd = os.open(MANIFEST_PATH, os.O_RDONLY)
                try:
                    os.fsync(fd)
                finally:
                    os.close(fd)
            except Exception as e:
                logger.warning(f"Failed to sync {MANIFEST_PATH}: {e}")

            logger.info(f"Batch {batch_idx}/{len(batches)}: Processed {len(batch_rows)} rows (total {len(rows)} rows written)")

    logger.info("=" * 60)
    logger.info(f"Wrote {MANIFEST_PATH} ({len(rows)} rows, {failed} failed)")
    logger.info("=" * 60)


if __name__ == "__main__":
    main()
