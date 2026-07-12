"""
Blender script: top-down orthographic render of a POST-PROCESSED .blend tile.

Unlike _blend_render_topdown.py (which renders the raw GLB), this opens an
already-built .blend (roads draped, cars cut out of the terrain) and renders it
straight down against a black background, so any holes left in the ground by the
car-removal cut show up as black. The car "wrapper" collider objects are made
visible as a bright wireframe cage — visible enough to locate each car, but open
enough that the ground (or a hole) underneath still shows through.

Writes <blend_stem>_postrender.jpg next to each blend.

Run via:
    blender -b -P _blend_render_postprocess.py -- <blend_path> [<blend_path> ...]
"""

from __future__ import annotations

import os
import sys

import bpy
from mathutils import Vector

RENDER_SIZE = 512
ORTHO_PADDING = 1.05
CAGE_COLOR = (1.0, 0.15, 0.05, 1.0)  # bright red-orange wireframe


def pick_render_engine() -> str:
    for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE"):
        try:
            bpy.context.scene.render.engine = engine
            if bpy.context.scene.render.engine == engine:
                return engine
        except Exception:
            continue
    return "BLENDER_EEVEE"


def convert_materials_to_emission() -> None:
    """Flatten every textured material to an unlit emission of its base color image."""
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


def make_cage_material() -> bpy.types.Material:
    """
    A translucent red-orange tint for the car wrappers: emission mixed with a
    transparent BSDF, so the wrapper box is clearly visible from above while the
    ground (or a hole) underneath still shows through it. A solid material would hide
    exactly the fill we want to inspect; a wireframe modifier spikes on the molded
    collider geometry — a tint avoids both.
    """
    mat = bpy.data.materials.new("car_cage")
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    for node in list(nodes):
        nodes.remove(node)
    out = nodes.new(type="ShaderNodeOutputMaterial")
    mix = nodes.new(type="ShaderNodeMixShader")
    trans = nodes.new(type="ShaderNodeBsdfTransparent")
    emit = nodes.new(type="ShaderNodeEmission")
    emit.inputs["Color"].default_value = CAGE_COLOR
    mix.inputs["Fac"].default_value = 0.5  # 50% tint, 50% see-through
    links.new(trans.outputs[0], mix.inputs[1])
    links.new(emit.outputs["Emission"], mix.inputs[2])
    links.new(mix.outputs[0], out.inputs["Surface"])
    for attr, val in (("surface_render_method", "BLENDED"), ("blend_method", "BLEND")):
        try:
            setattr(mat, attr, val)
        except (AttributeError, TypeError):
            pass
    mat.use_backface_culling = False
    return mat


def show_car_wrappers_as_cage() -> None:
    """
    Tint every object in the 'cars' collection translucent red so the wrapper is
    visible without occluding the ground/holes beneath it.
    """
    cars_coll = bpy.data.collections.get("cars")
    if cars_coll is None:
        return
    cage_mat = make_cage_material()
    for obj in cars_coll.objects:
        if obj.type != "MESH":
            continue
        obj.hide_render = False
        obj.hide_viewport = False
        obj.data.materials.clear()
        obj.data.materials.append(cage_mat)


def compute_mesh_bbox(objects) -> dict | None:
    min_corner = Vector((float("inf"),) * 3)
    max_corner = Vector((float("-inf"),) * 3)
    found = False
    for obj in objects:
        if obj.type != "MESH":
            continue
        found = True
        for corner in obj.bound_box:
            wc = obj.matrix_world @ Vector(corner)
            min_corner.x = min(min_corner.x, wc.x)
            min_corner.y = min(min_corner.y, wc.y)
            min_corner.z = min(min_corner.z, wc.z)
            max_corner.x = max(max_corner.x, wc.x)
            max_corner.y = max(max_corner.y, wc.y)
            max_corner.z = max(max_corner.z, wc.z)
    if not found:
        return None
    center = (min_corner + max_corner) / 2.0
    size = max_corner - min_corner
    return {
        "min": min_corner,
        "max": max_corner,
        "center": center,
        "size": size,
    }


def setup_world_black(scene: bpy.types.Scene) -> None:
    if not scene.world:
        scene.world = bpy.data.worlds.new("World")
    scene.world.use_nodes = True
    bg = scene.world.node_tree.nodes.get("Background")
    if bg:
        bg.inputs["Color"].default_value = (0.0, 0.0, 0.0, 1.0)


def render_blend(blend_path: str) -> bool:
    bpy.ops.wm.open_mainfile(filepath=os.path.abspath(blend_path))
    scene = bpy.context.scene

    scene.render.engine = pick_render_engine()
    scene.render.image_settings.file_format = "JPEG"
    scene.render.resolution_x = RENDER_SIZE
    scene.render.resolution_y = RENDER_SIZE
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = False
    if hasattr(scene, "eevee"):
        for attr in ("use_shadows", "use_gtao", "use_ssr", "use_bloom"):
            try:
                setattr(scene.eevee, attr, False)
            except Exception:
                pass
    setup_world_black(scene)

    convert_materials_to_emission()

    # Frame the camera on the terrain only, so the wrapper cages don't blow out the
    # bounds; then reveal the wrappers as cages on top.
    terrain_objs = [
        o
        for o in scene.objects
        if o.type == "MESH" and o.data.uv_layers and o.data.materials
    ]
    bbox = compute_mesh_bbox(terrain_objs or scene.objects)
    if bbox is None:
        print(f"POSTRENDER_FAIL {blend_path}: no mesh geometry")
        return False

    show_car_wrappers_as_cage()

    horizontal_extent = max(bbox["size"].x, bbox["size"].y) or 1.0
    ortho_scale = horizontal_extent * ORTHO_PADDING

    cam_data = bpy.data.cameras.new(name="TopDownCam")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = ortho_scale
    cam_data.clip_start = 0.1
    cam_data.clip_end = max(bbox["size"].z, 1.0) + 500.0

    cam_obj = bpy.data.objects.new(name="TopDownCam", object_data=cam_data)
    scene.collection.objects.link(cam_obj)
    scene.camera = cam_obj
    cam_height = bbox["max"].z + max(horizontal_extent, 10.0)
    cam_obj.location = (bbox["center"].x, bbox["center"].y, cam_height)
    cam_obj.rotation_euler = (0.0, 0.0, 0.0)

    out_jpg = os.path.splitext(os.path.abspath(blend_path))[0] + "_postrender.jpg"
    os.makedirs(os.path.dirname(out_jpg), exist_ok=True)
    scene.render.filepath = out_jpg
    bpy.ops.render.render(write_still=True)
    print(f"POSTRENDER_OK {blend_path} -> {out_jpg}")
    return True


def main() -> None:
    try:
        args = sys.argv[sys.argv.index("--") + 1 :]
    except ValueError:
        args = []
    if not args:
        print("Usage: blender -b -P _blend_render_postprocess.py -- <blend> [<blend> ...]")
        sys.exit(1)

    ok = 0
    for blend_path in args:
        try:
            if render_blend(blend_path):
                ok += 1
        except Exception as exc:
            print(f"POSTRENDER_FAIL {blend_path}: {exc}")
    print(f"Postrender complete: {ok} ok (of {len(args)})")


if __name__ == "__main__":
    main()
