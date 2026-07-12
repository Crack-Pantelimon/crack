"""Headless verification of per-mesh materials in a blend output."""
import sys

import bpy
import numpy as np


def _find_base_color_image(mat):
    if not mat or not mat.use_nodes or not mat.node_tree:
        return None, None
    for node in mat.node_tree.nodes:
        if node.type == "TEX_IMAGE" and node.image is not None:
            return node, node.image
    return None, None


def check_blend(blend_path: str) -> None:
    bpy.ops.wm.open_mainfile(filepath=blend_path)
    terrain_objs = [
        o
        for o in bpy.data.objects
        if o.type == "MESH" and o.data.uv_layers and o.data.materials
    ]
    print(f"terrain meshes: {len(terrain_objs)}")
    errors = []

    for i, mesh_obj in enumerate(terrain_objs):
        mat_names = {m.name for m in mesh_obj.data.materials if m}
        for expected in (
            f"cars_{i}",
            f"ground_{i}",
            f"air_{i}",
            f"car_painted_with_ground_{i}",
        ):
            if expected not in bpy.data.materials:
                errors.append(f"mesh {i}: missing material {expected}")

        painted_name = f"car_painted_with_ground_{i}"
        if painted_name not in bpy.data.materials:
            continue
        painted_mat = bpy.data.materials[painted_name]
        painted_slot = next(
            (j for j, m in enumerate(mesh_obj.data.materials) if m == painted_mat),
            None,
        )
        if painted_slot is None:
            errors.append(f"mesh {i}: painted material not in mesh slots")
            continue

        poly_slots = {p.material_index for p in mesh_obj.data.polygons}
        if poly_slots != {painted_slot}:
            errors.append(
                f"mesh {i}: polygons use slots {poly_slots}, expected {{{painted_slot}}}"
            )

        orig_mat = mesh_obj.data.materials[0]
        _, orig_img = _find_base_color_image(orig_mat)
        _, painted_img = _find_base_color_image(painted_mat)
        if orig_img is None or painted_img is None:
            errors.append(f"mesh {i}: missing base-color images")
            continue

        w, h = orig_img.size
        orig_px = np.empty(w * h * 4, dtype=np.float32)
        painted_px = np.empty(w * h * 4, dtype=np.float32)
        orig_img.pixels.foreach_get(orig_px)
        painted_img.pixels.foreach_get(painted_px)
        orig_rgba = orig_px.reshape(h, w, 4)
        painted_rgba = painted_px.reshape(h, w, 4)

        air_mat = bpy.data.materials.get(f"air_{i}")
        if air_mat:
            _, air_img = _find_base_color_image(air_mat)
            if air_img is not None:
                asz_w, asz_h = air_img.size
                air_px = np.empty(asz_w * asz_h * 4, dtype=np.float32)
                air_img.pixels.foreach_get(air_px)
                air_mask = air_px.reshape(asz_h, asz_w, 4)[..., 0] > 0.5
                if air_mask.any():
                    src_h, src_w = air_mask.shape
                    yi = np.clip(
                        np.round(np.linspace(0, src_h - 1, h)).astype(int),
                        0,
                        src_h - 1,
                    )
                    xi = np.clip(
                        np.round(np.linspace(0, src_w - 1, w)).astype(int),
                        0,
                        src_w - 1,
                    )
                    air_rs = air_mask[np.ix_(yi, xi)]
                    air_coords = np.argwhere(air_rs)
                    if len(air_coords) > 0:
                        for y, x in air_coords[:3]:
                            if np.allclose(orig_rgba[y, x, :3], painted_rgba[y, x, :3]):
                                errors.append(
                                    f"mesh {i}: air texel ({y},{x}) unchanged"
                                )
                    non_air = ~air_rs
                    if non_air.any():
                        y, x = np.argwhere(non_air)[0]
                        if not np.allclose(
                            orig_rgba[y, x, :3], painted_rgba[y, x, :3], atol=1e-6
                        ):
                            errors.append(
                                f"mesh {i}: non-air texel ({y},{x}) changed unexpectedly"
                            )

        print(f"mesh {i} ({mesh_obj.name}): materials ok, slot={painted_slot}")

    if errors:
        for e in errors:
            print(f"FAIL: {e}")
        sys.exit(1)
    print("CHECK_OK")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: blender -b blend.blend -P _check_blend.py -- <blend_path>")
        sys.exit(1)
    check_blend(sys.argv[-1])
