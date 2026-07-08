"""
Blender script: top-down orthographic renders of map tiles.

Loads one GLB per render, frames the mesh with an orthographic camera looking
straight down, and writes a JPG plus sidecar JSON metadata.

Run via:
    blender -b -P street_cleanup/render_top_down.py -- <batch_json_path>
"""

from __future__ import annotations

import json
import os
import sys

import bpy
from mathutils import Vector

RENDER_SIZE = 128
ORTHO_PADDING = 1.05
_GPU_CONFIGURED = False


def enable_gpu_rendering() -> list[str]:
    """Enable GPU compute devices for Blender rendering."""
    enabled: list[str] = []
    prefs = bpy.context.preferences

    try:
        cycles_prefs = prefs.addons["cycles"].preferences
    except KeyError:
        print("GPU_RENDER: Cycles addon not available")
        return enabled

    for backend in ("OPTIX", "CUDA", "HIP", "ONEAPI", "METAL"):
        try:
            cycles_prefs.compute_device_type = backend
            cycles_prefs.get_devices()
            if any(device.type != "CPU" for device in cycles_prefs.devices):
                break
        except Exception:
            continue

    for device in cycles_prefs.devices:
        device.use = device.type != "CPU"
        if device.use:
            enabled.append(f"{device.type}:{device.name}")

    if enabled:
        print(f"GPU_RENDER enabled: {', '.join(enabled)}")
    else:
        print("GPU_RENDER: no GPU compute devices found, using Blender defaults")

    return enabled


def pick_render_engine() -> str:
    for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE"):
        try:
            bpy.context.scene.render.engine = engine
            if bpy.context.scene.render.engine == engine:
                return engine
        except Exception:
            continue
    return "BLENDER_EEVEE"


def ensure_gpu_rendering() -> None:
    global _GPU_CONFIGURED
    if _GPU_CONFIGURED:
        return
    enable_gpu_rendering()
    _GPU_CONFIGURED = True


def clear_scene() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)


def convert_materials_to_emission() -> None:
    for mat in bpy.data.materials:
        if not mat.use_nodes:
            continue
        nodes = mat.node_tree.nodes
        links = mat.node_tree.links

        tex_node = None
        output_node = None
        for node in nodes:
            if node.type == "TEX_IMAGE":
                tex_node = node
            elif node.type == "OUTPUT_MATERIAL":
                output_node = node

        if tex_node is None or output_node is None:
            continue

        for node in list(nodes):
            if node not in (tex_node, output_node):
                nodes.remove(node)

        emit_node = nodes.new(type="ShaderNodeEmission")
        links.new(tex_node.outputs["Color"], emit_node.inputs["Color"])
        links.new(emit_node.outputs["Emission"], output_node.inputs["Surface"])


def compute_mesh_bbox() -> dict | None:
    min_corner = Vector((float("inf"), float("inf"), float("inf")))
    max_corner = Vector((float("-inf"), float("-inf"), float("-inf")))
    found = False

    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        found = True
        for corner in obj.bound_box:
            world_corner = obj.matrix_world @ Vector(corner)
            min_corner.x = min(min_corner.x, world_corner.x)
            min_corner.y = min(min_corner.y, world_corner.y)
            min_corner.z = min(min_corner.z, world_corner.z)
            max_corner.x = max(max_corner.x, world_corner.x)
            max_corner.y = max(max_corner.y, world_corner.y)
            max_corner.z = max(max_corner.z, world_corner.z)

    if not found:
        return None

    center = (min_corner + max_corner) / 2.0
    size = max_corner - min_corner
    return {
        "min": [float(min_corner.x), float(min_corner.y), float(min_corner.z)],
        "max": [float(max_corner.x), float(max_corner.y), float(max_corner.z)],
        "center": [float(center.x), float(center.y), float(center.z)],
        "size": [float(size.x), float(size.y), float(size.z)],
    }


def resolve_resolution(tile: dict) -> tuple[int, int]:
    size = tile.get("resolution", RENDER_SIZE)
    if isinstance(size, (list, tuple)) and len(size) >= 2:
        return int(size[0]), int(size[1])
    return int(size), int(size)


def setup_render_settings(scene: bpy.types.Scene, *, width: int, height: int) -> None:
    ensure_gpu_rendering()
    scene.render.engine = pick_render_engine()
    scene.render.image_settings.file_format = "JPEG"
    scene.render.resolution_x = width
    scene.render.resolution_y = height
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = False

    if not scene.world:
        scene.world = bpy.data.worlds.new("World")
    scene.world.use_nodes = True
    bg_node = scene.world.node_tree.nodes.get("Background")
    if bg_node:
        bg_node.inputs["Color"].default_value = (0.7, 0.8, 0.9, 1.0)


def render_tile(tile: dict) -> bool:
    glb_path = tile["glb_path"]
    out_jpg = tile["jpg_path"]
    out_json = tile["meta_path"]

    width, height = resolve_resolution(tile)

    clear_scene()
    setup_render_settings(scene := bpy.context.scene, width=width, height=height)

    try:
        bpy.ops.import_scene.gltf(filepath=os.path.abspath(glb_path))
    except Exception as exc:
        print(f"RENDER_FAIL {tile.get('octant_path', glb_path)}: import failed: {exc}")
        return False

    convert_materials_to_emission()
    bbox = compute_mesh_bbox()
    if bbox is None:
        print(f"RENDER_FAIL {tile.get('octant_path', glb_path)}: no mesh geometry")
        return False

    horizontal_extent = max(bbox["size"][0], bbox["size"][1])
    if horizontal_extent <= 0:
        horizontal_extent = 1.0
    ortho_scale = horizontal_extent * ORTHO_PADDING

    cam_data = bpy.data.cameras.new(name="TopDownCam")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = ortho_scale
    cam_data.clip_start = 0.1
    cam_data.clip_end = max(bbox["size"][2], 1.0) + 500.0

    cam_obj = bpy.data.objects.new(name="TopDownCam", object_data=cam_data)
    bpy.context.collection.objects.link(cam_obj)
    scene.camera = cam_obj

    cam_height = bbox["max"][2] + max(horizontal_extent, 10.0)
    cam_obj.location = (bbox["center"][0], bbox["center"][1], cam_height)
    cam_obj.rotation_euler = (0.0, 0.0, 0.0)

    os.makedirs(os.path.dirname(os.path.abspath(out_jpg)), exist_ok=True)
    os.makedirs(os.path.dirname(os.path.abspath(out_json)), exist_ok=True)

    scene.render.filepath = os.path.abspath(out_jpg)
    bpy.ops.render.render(write_still=True)

    meta = {
        "octant_path": tile.get("octant_path"),
        "glb_path": glb_path,
        "resolution": [width, height],
        "ortho_scale": ortho_scale,
        "camera_location": list(cam_obj.location),
        "bbox_xyz": {
            "min": bbox["min"],
            "max": bbox["max"],
        },
        "lat_lon_bbox": tile.get("lat_lon_bbox"),
    }
    with open(out_json, "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=2)

    print(f"RENDER_OK {tile.get('octant_path', glb_path)}")
    return True


def main() -> None:
    try:
        args_idx = sys.argv.index("--")
        args = sys.argv[args_idx + 1 :]
    except ValueError:
        args = []

    if len(args) < 1:
        print("Usage: blender -b -P render_top_down.py -- <batch_json_path>")
        sys.exit(1)

    with open(args[0], "r", encoding="utf-8") as f:
        batch = json.load(f)

    tiles = batch.get("tiles", [])
    if not tiles:
        print("No tiles to render")
        sys.exit(0)

    rendered = 0
    failed = 0
    for tile in tiles:
        try:
            if render_tile(tile):
                rendered += 1
            else:
                failed += 1
        except Exception as exc:
            failed += 1
            print(f"RENDER_FAIL {tile.get('octant_path', '?')}: {exc}")

    print(f"Batch complete: {rendered} rendered, {failed} failed (of {len(tiles)})")


if __name__ == "__main__":
    main()
