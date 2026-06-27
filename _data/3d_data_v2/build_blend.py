import bpy
import sys
import os
import json
import base64
import math
import numpy as np

def _read_varint(data: bytes, offset: int) -> tuple[int, int]:
    val = 0
    shift = 0
    while True:
        b = data[offset]
        offset += 1
        val |= (b & 0x7F) << shift
        if not (b & 0x80):
            break
        shift += 7
    return val, offset

def unpack_vertices(packed: bytes) -> np.ndarray:
    count = len(packed) // 3
    if count == 0:
        return np.zeros((0, 3), dtype=np.uint8)

    data = np.frombuffer(packed, dtype=np.uint8)
    x_stream = data[0:count]
    y_stream = data[count : 2 * count]
    z_stream = data[2 * count : 3 * count]

    x = np.cumsum(x_stream, dtype=np.uint8)
    y = np.cumsum(y_stream, dtype=np.uint8)
    z = np.cumsum(z_stream, dtype=np.uint8)

    return np.stack([x, y, z], axis=1)

def unpack_tex_coords(packed: bytes, vertex_count: int) -> tuple[np.ndarray, float, float]:
    if len(packed) < 4:
        return np.zeros((vertex_count, 2), dtype=np.uint16), 1, 1

    import struct
    u_mod = 1 + struct.unpack_from("<H", packed, 0)[0]
    v_mod = 1 + struct.unpack_from("<H", packed, 2)[0]
    data = packed[4:]

    uvs = np.zeros((vertex_count, 2), dtype=np.int32)
    u = 0
    v = 0
    for i in range(vertex_count):
        u = (u + data[vertex_count * 0 + i] + (data[vertex_count * 2 + i] << 8)) % u_mod
        v = (v + data[vertex_count * 1 + i] + (data[vertex_count * 3 + i] << 8)) % v_mod
        uvs[i, 0] = u
        uvs[i, 1] = v

    return uvs.astype(np.uint16), u_mod, v_mod

def unpack_indices_to_strip(packed: bytes) -> np.ndarray:
    offset = 0
    strip_len, offset = _read_varint(packed, offset)

    strip = np.zeros(strip_len, dtype=np.int32)
    zeros = 0
    b = 0
    c = 0
    for i in range(strip_len):
        val, offset = _read_varint(packed, offset)
        a = b
        b = c
        c = zeros - val
        strip[i] = c
        if val == 0:
            zeros += 1

    return strip.astype(np.uint32)

def triangulate_strip(strip: np.ndarray, w_mask=None, masked_octants=None) -> np.ndarray:
    triangles = []
    for i in range(len(strip) - 2):
        a = strip[i]
        b = strip[i + 1]
        c = strip[i + 2]
        if a == b or a == c or b == c:
            continue
        if w_mask is not None and masked_octants:
            if (w_mask[a] in masked_octants or w_mask[b] in masked_octants
                    or w_mask[c] in masked_octants):
                continue
        if i & 1:
            triangles.extend([a, c, b])
        else:
            triangles.extend([a, b, c])

    if not triangles:
        return np.array([], dtype=np.uint32)
    return np.array(triangles, dtype=np.uint32)

def unpack_octant_mask_and_layer_bounds(
    packed: bytes,
    indices: np.ndarray,
    vertex_count: int,
) -> tuple[np.ndarray, list[int]]:
    w_mask = np.zeros(vertex_count, dtype=np.uint8)
    layer_bounds = [0] * 10

    if not packed:
        layer_bounds = [len(indices)] * 10
        return w_mask, layer_bounds

    offset = 0
    n_entries, offset = _read_varint(packed, offset)

    idx_i = 0
    k = 0
    m = 0

    for i in range(n_entries):
        if i % 8 == 0:
            if m < 10:
                layer_bounds[m] = k
                m += 1

        v, offset = _read_varint(packed, offset)
        for _ in range(v):
            if idx_i < len(indices):
                vtx_i = indices[idx_i]
                idx_i += 1
                if vtx_i < vertex_count:
                    w_mask[vtx_i] = i & 7
            k += 1

    while m < 10:
        layer_bounds[m] = k
        m += 1

    return w_mask, layer_bounds

def unpack_for_normals(for_normals_bytes: bytes) -> np.ndarray | None:
    if not for_normals_bytes or len(for_normals_bytes) < 3:
        return None

    import struct
    data = for_normals_bytes
    count = struct.unpack_from("<H", data, 0)[0]
    if count * 2 != len(data) - 3:
        return None

    s = data[2]
    data = data[3:]

    def f1(v, l):
        if l <= 4:
            return (v << l) + (v & ((1 << l) - 1))
        if l <= 6:
            r = 8 - l
            val = v << l
            val2 = val >> r
            return val + val2 + (val2 >> r) + (val2 >> r >> r)
        return -(v & 1)

    def f2(c):
        cr = round(c)
        return max(0, min(255, cr))

    output = np.zeros((count, 3), dtype=np.uint8)
    for i in range(count):
        a_val = f1(data[0 + i], s) / 255.0
        f_val = f1(data[count + i], s) / 255.0

        b = a_val
        c = f_val
        g = b + c
        h = b - c
        sign = 1

        if not (0.5 <= g <= 1.5 and -0.5 <= h <= 0.5):
            sign = -1
            if g <= 0.5:
                b = 0.5 - f_val
                c = 0.5 - a_val
            elif g >= 1.5:
                b = 1.5 - f_val
                c = 1.5 - a_val
            elif h <= -0.5:
                b = f_val - 0.5
                c = a_val + 0.5
            else:
                b = f_val + 0.5
                c = a_val - 0.5
            g = b + c
            h = b - c

        a = min(min(2 * g - 1, 3 - 2 * g), min(2 * h + 1, 1 - 2 * h)) * sign
        b = 2 * b - 1
        c = 2 * c - 1
        mag = math.sqrt(a * a + b * b + c * c)
        if mag > 0:
            m = 127 / mag
        else:
            m = 0

        output[i, 0] = f2(m * a + 127)
        output[i, 1] = f2(m * b + 127)
        output[i, 2] = f2(m * c + 127)

    return output

def unpack_normals(
    mesh_normals: bytes,
    for_normals: np.ndarray | None,
    vertex_count: int,
) -> np.ndarray:
    if mesh_normals and for_normals is not None and len(mesh_normals) > 0:
        count = len(mesh_normals) // 2
        normals = np.zeros((count, 3), dtype=np.float32)
        data = mesh_normals
        for i in range(count):
            j = data[i] + (data[count + i] << 8)
            if j < len(for_normals):
                normals[i, 0] = (for_normals[j, 0] - 127.0) / 127.0
                normals[i, 1] = (for_normals[j, 1] - 127.0) / 127.0
                normals[i, 2] = (for_normals[j, 2] - 127.0) / 127.0
            else:
                normals[i] = [0, 0, 1]
    else:
        normals = np.full((vertex_count, 3), [0, 0, 1], dtype=np.float32)

    norms = np.linalg.norm(normals, axis=1, keepdims=True)
    norms = np.where(norms > 0, norms, 1.0)
    normals = normals / norms

    return normals

def apply_matrix(vertices_u8: np.ndarray, matrix: list[float]) -> np.ndarray:
    n = len(vertices_u8)
    if n == 0:
        return np.zeros((0, 3), dtype=np.float64)

    M = np.array(matrix, dtype=np.float64).reshape(4, 4).T

    pts = np.ones((n, 4), dtype=np.float64)
    pts[:, 0] = vertices_u8[:, 0].astype(np.float64)
    pts[:, 1] = vertices_u8[:, 1].astype(np.float64)
    pts[:, 2] = vertices_u8[:, 2].astype(np.float64)

    result = pts @ M.T
    return result[:, :3]

def transform_normals(normals: np.ndarray, matrix: list[float]) -> np.ndarray:
    if len(normals) == 0:
        return normals

    M = np.array(matrix, dtype=np.float64).reshape(4, 4).T
    rot = M[:3, :3]

    result = normals.astype(np.float64) @ rot.T
    norms = np.linalg.norm(result, axis=1, keepdims=True)
    norms = np.where(norms > 0, norms, 1.0)
    return (result / norms).astype(np.float32)

def get_enu_rotation_matrix(ref_point: np.ndarray) -> np.ndarray:
    """
    Computes rotation matrix from ECEF space to local ENU tangent plane at ref_point.
    """
    rx, ry, rz = ref_point
    L = math.sqrt(rx*rx + ry*ry + rz*rz)
    if L == 0:
        return np.eye(3)
    u = np.array([rx/L, ry/L, rz/L])
    
    xy_len = math.sqrt(rx*rx + ry*ry)
    if xy_len > 0:
        e = np.array([-ry/xy_len, rx/xy_len, 0.0])
    else:
        e = np.array([1.0, 0.0, 0.0])
        
    n = np.cross(u, e)
    
    # Rows are e, n, u. This maps ECEF (relative to ref_point) to ENU.
    R = np.stack([e, n, u], axis=0)
    return R

def clear_scene():
    """Wipe the current scene so the next node imports into a clean slate."""
    bpy.ops.wm.read_factory_settings(use_empty=True)


def build_one(json_path: str, out_blend_path: str, ref_point: np.ndarray):
    """Build a single node's .blend + .glb from its decoded NodeData JSON cache."""
    # Load JSON data
    with open(json_path, "r", encoding="utf-8") as f:
        node_data = json.load(f)

    meshes_list = node_data.get("meshes", [])
    ma = node_data.get("matrix_globe_from_mesh", [])
    for_normals_bytes = base64.b64decode(node_data.get("for_normals", ""))
    for_normals = unpack_for_normals(for_normals_bytes)

    for mesh_idx, mesh_json in enumerate(meshes_list):
        # 1. Unpack vertices
        vertices_bytes = base64.b64decode(mesh_json.get("vertices", ""))
        raw_verts = unpack_vertices(vertices_bytes)
        vertex_count = len(raw_verts)

        if vertex_count == 0:
            continue

        # 2. Unpack UVs
        tex_coords_bytes = base64.b64decode(mesh_json.get("texture_coordinates", ""))
        raw_uvs, u_mod, v_mod = unpack_tex_coords(tex_coords_bytes, vertex_count)

        # 3. Unpack indices
        indices_bytes = base64.b64decode(mesh_json.get("indices", ""))
        raw_strip = unpack_indices_to_strip(indices_bytes)

        # 4. Unpack octant mask (used only for layer bounds, not for filtering)
        layer_data_bytes = base64.b64decode(mesh_json.get("layer_and_octant_counts", ""))
        w_mask, layer_bounds = unpack_octant_mask_and_layer_bounds(
            layer_data_bytes, raw_strip, vertex_count
        )

        # 5. Triangulate (NO octant masking - tiles are kept whole so coarse LODs
        # are not carved up by the octants that have their own finer tiles).
        # Keep only the triangles inside the renderable bounds (layer_bounds[3]).
        max_idx = min(layer_bounds[3], len(raw_strip))
        truncated_strip = raw_strip[:max_idx]
        raw_indices = triangulate_strip(truncated_strip)

        if len(raw_indices) == 0:
            continue

        # 6. Apply matrix and subtract reference point
        transformed_verts = apply_matrix(raw_verts, ma)
        transformed_verts -= ref_point

        # 7. Unpack and transform normals
        normals_bytes = base64.b64decode(mesh_json.get("normals", ""))
        raw_normals = unpack_normals(normals_bytes, for_normals, vertex_count)
        transformed_normals = transform_normals(raw_normals, ma)

        # Rotate positions and normals from ECEF to local ENU tangent plane at reference point
        R = get_enu_rotation_matrix(ref_point)
        # To match Blender's Z-up coordinate system where +Z is Up, +X is East, +Y is North
        # Row 0: East, Row 1: North, Row 2: Up
        R_blend = np.stack([R[0], R[1], R[2]], axis=0)
        
        transformed_verts = transformed_verts @ R_blend.T
        transformed_normals = transformed_normals @ R_blend.T

        # 8. Compute UVs
        uv_offset_and_scale = mesh_json.get("uv_offset_and_scale", [])
        if len(uv_offset_and_scale) == 4:
            # Replicate JS decoding logic for explicitly provided uv_offset_and_scale
            uv_offset = (uv_offset_and_scale[0], uv_offset_and_scale[1] - 1.0 / uv_offset_and_scale[3])
            uv_scale = (uv_offset_and_scale[2], -uv_offset_and_scale[3])
        else:
            uv_offset = (0.5, 0.5 - v_mod if v_mod > 0 else 0.5)
            uv_scale = (1.0 / u_mod if u_mod > 0 else 1.0, -1.0 / v_mod if v_mod > 0 else -1.0)

        uvs_float = np.zeros((vertex_count, 2), dtype=np.float32)
        uvs_float[:, 0] = (raw_uvs[:, 0].astype(np.float32) + uv_offset[0]) * uv_scale[0]
        uvs_float[:, 1] = (raw_uvs[:, 1].astype(np.float32) + uv_offset[1]) * uv_scale[1]

        # 9. Create Blender Mesh
        mesh_name = f"mesh_{mesh_idx}"
        mesh_data = bpy.data.meshes.new(name=mesh_name)
        file_id = os.path.splitext(os.path.basename(out_blend_path))[0]
        obj = bpy.data.objects.new(name=file_id, object_data=mesh_data)
        bpy.context.collection.objects.link(obj)

        faces = raw_indices.reshape(-1, 3).tolist()
        vertices_list = transformed_verts.tolist()

        mesh_data.from_pydata(vertices_list, [], faces)
        mesh_data.update()

        # 10. Assign custom split normals
        if len(transformed_normals) > 0:
            vertex_normals = transformed_normals.tolist()
            mesh_data.normals_split_custom_set_from_vertices(vertex_normals)

        # 11. Assign UV map coordinates
        uv_loop_map = mesh_data.uv_layers.new(name="UVMap")
        uv_data = uv_loop_map.data
        for loop_idx, loop in enumerate(mesh_data.loops):
            v_idx = loop.vertex_index
            uv_data[loop_idx].uv = (uvs_float[v_idx, 0], uvs_float[v_idx, 1])

        # 12. Create and assign Material / Texture
        texture_list = mesh_json.get("texture", [])
        if len(texture_list) > 0:
            tex_json = texture_list[0]
            data_list = tex_json.get("data", [])
            if len(data_list) > 0:
                tex_bytes = base64.b64decode(data_list[0])
                
                # Write temp file to load texture image into Blender
                temp_img_name = f"temp_tex_{mesh_idx}.jpg"
                with open(temp_img_name, "wb") as f_img:
                    f_img.write(tex_bytes)

                try:
                    image = bpy.data.images.load(filepath=os.path.abspath(temp_img_name))
                    image.pack()
                except Exception as ex:
                    print(f"Failed to load/pack image: {ex}")
                    image = None
                finally:
                    if os.path.exists(temp_img_name):
                        os.remove(temp_img_name)

                if image:
                    material = bpy.data.materials.new(name=f"mat_{mesh_idx}")
                    material.use_nodes = True
                    nodes = material.node_tree.nodes
                    links = material.node_tree.links
                    nodes.clear()

                    node_output = nodes.new(type="ShaderNodeOutputMaterial")
                    node_bsdf = nodes.new(type="ShaderNodeBsdfPrincipled")
                    node_tex = nodes.new(type="ShaderNodeTexImage")

                    node_tex.image = image
                    links.new(node_tex.outputs["Color"], node_bsdf.inputs["Base Color"])
                    links.new(node_bsdf.outputs["BSDF"], node_output.inputs["Surface"])

                    mesh_data.materials.append(material)

    # Save to blend file
    os.makedirs(os.path.dirname(out_blend_path), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=os.path.abspath(out_blend_path))

    # Export to GLB
    out_glb_path = os.path.splitext(out_blend_path)[0] + ".glb"
    bpy.ops.export_scene.gltf(
        filepath=os.path.abspath(out_glb_path),
        export_format='GLB',
        use_selection=False,
    )
    assert os.path.exists(out_glb_path), f"GLB export produced no file: {out_glb_path}"


def main():
    """Process a batch of nodes described by a single JSON file in one Blender run."""
    try:
        args_idx = sys.argv.index("--")
        args = sys.argv[args_idx + 1:]
    except ValueError:
        args = []

    if len(args) < 1:
        print("Usage: blender -b -P build_blend.py -- <batch_json_path>")
        sys.exit(1)

    batch_path = args[0]
    with open(batch_path, "r", encoding="utf-8") as f:
        batch = json.load(f)

    ref_point = np.array(batch["ref_point"], dtype=np.float64)
    nodes = batch.get("nodes", [])

    built = 0
    failed = 0
    for node in nodes:
        octant = node.get("octant_path", "?")
        # Delete everything from the scene before building the next node.
        clear_scene()
        try:
            build_one(node["json_path"], node["blend_path"], ref_point)
            built += 1
            print(f"BUILD_OK {octant}")
        except Exception as e:
            failed += 1
            print(f"BUILD_FAIL {octant}: {e}")

    print(f"Batch complete: {built} built, {failed} failed (of {len(nodes)})")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"FATAL build_blend batch error: {e}")
        sys.exit(1)
