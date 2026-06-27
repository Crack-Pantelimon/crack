"""
Blender helper: extract geometry stats from a .glb file.

Run headless via:
    blender -b -P glb_stats.py -- <glb_path>

Imports the GLB, then prints a single line to stdout of the form:
    GLB_STATS:{"vertex_count": ..., "triangle_count": ..., "xyz_min": [...], "xyz_max": [...]}

The xyz bbox is in the GLB's own coordinate frame (the local ENU frame relative
to the export reference point used by build_blend.py). rebuild_manifest.py parses
the marker line to assemble the Parquet manifest.
"""

import bpy
import sys
import os
import json


def glb_stats(glb_path: str) -> dict:
    # Start from an empty scene so only the imported GLB contributes geometry.
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.abspath(glb_path))

    vertex_count = 0
    triangle_count = 0
    xyz_min = [float("inf"), float("inf"), float("inf")]
    xyz_max = [float("-inf"), float("-inf"), float("-inf")]

    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        matrix = obj.matrix_world
        mesh = obj.data
        mesh.calc_loop_triangles()
        triangle_count += len(mesh.loop_triangles)
        vertex_count += len(mesh.vertices)
        for vertex in mesh.vertices:
            world_pos = matrix @ vertex.co
            for i in range(3):
                if world_pos[i] < xyz_min[i]:
                    xyz_min[i] = world_pos[i]
                if world_pos[i] > xyz_max[i]:
                    xyz_max[i] = world_pos[i]

    if vertex_count == 0:
        # No geometry: emit zeroed bbox so downstream code stays numeric.
        xyz_min = [0.0, 0.0, 0.0]
        xyz_max = [0.0, 0.0, 0.0]

    return {
        "vertex_count": vertex_count,
        "triangle_count": triangle_count,
        "xyz_min": [float(v) for v in xyz_min],
        "xyz_max": [float(v) for v in xyz_max],
    }


if __name__ == "__main__":
    try:
        args_idx = sys.argv.index("--")
        args = sys.argv[args_idx + 1:]
    except ValueError:
        args = []

    if len(args) < 1:
        print("Usage: blender -b -P glb_stats.py -- <glb_path>")
        sys.exit(1)

    stats = glb_stats(args[0])
    print("GLB_STATS:" + json.dumps(stats))
