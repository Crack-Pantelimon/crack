# 3D Data V2

This folder contains the Python pipeline for fetching, decoding, and converting Google Earth's NodeData protobufs into Blender (`.blend`) files.

## Setup and Running

This project uses `uv` for dependency management. Dependencies (like `numpy`, `protobuf`, `pillow`, etc.) are defined in `pyproject.toml`.

### Running the Main Script

To run the main pipeline properly without hitting `ModuleNotFoundError` (such as `np not found`):

```bash
# Option 1: Run directly with uv
uv run main.py

# Option 2: Run arbitrary scripts using uv's python
uv run python debug_positions.py
```

*Note: Make sure you are inside the `_data/3d_data_v2` directory when running these commands so `uv` automatically picks up the `pyproject.toml` file.*

### Rebuilding the Manifest

`main.py` only downloads and exports `.blend` / `.glb` / `.jpg` artifacts — it does **not** write a manifest. The manifest is recomputed on demand from the `.glb` files on disk:

```bash
uv run rebuild_manifest.py
```

This globs `data_out/*/*.glb`, derives the octree id and lat/lon bbox from the path (via `octree.py`), and runs `glb_stats.py` in Blender on each `.glb` to extract the xyz bbox, vertex count, and triangle count. The result is written to `data_out/manifest.parquet` (Parquet, via `pyarrow`).

If `uv` is not in your PATH, you can usually find it at `~/.local/bin/uv` or `~/.cargo/bin/uv`:
```bash
~/.local/bin/uv run main.py
```

### Running from another context

If you need to run a script from a parent directory, you can specify the directory to `uv`:
```bash
uv run --directory _data/3d_data_v2 python _data/3d_data_v2/main.py
```

## Known Issues Addressed

- **Half Black Maps (LOD Truncation):** Previously, the triangle strips were truncated to `layer_bounds[3]`, which caused missing geometry and holes. The maps are now decoded fully.
- **Diagonal Maps / ECEF Orientation:** Bevy requires raw ECEF coordinate space (which is tilted/diagonal relative to world axes). We no longer artificially rotate the meshes to be ENU-flat.
- **Texture Banding / UV Bug:** When explicit `uv_offset_and_scale` is provided in the protobuf, the V-coordinate is now properly flipped according to the reference JS implementation, preventing texture wrapping/banding issues.
