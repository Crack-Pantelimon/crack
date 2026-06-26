"""
Mesh decoder for Google Earth NodeData protobuf.

Ports the C++ unpacking algorithms from rocktree_decoder.h to Python.
Handles vertices, texture coordinates, indices, normals, and octant masks.
"""

import struct
import math
import numpy as np

import rocktree_pb2 as pb


def _read_varint(data: bytes, offset: int) -> tuple[int, int]:
    """Read a variable-length integer from data starting at offset.
    Returns (value, new_offset)."""
    c = 0
    d = 1
    while True:
        if offset >= len(data):
            raise ValueError("Varint extends past end of data")
        e = data[offset]
        offset += 1
        c += (e & 0x7F) * d
        d <<= 7
        if not (e & 0x80):
            break
    return c, offset


def unpack_vertices(packed: bytes) -> np.ndarray:
    """
    Unpack vertices from Mesh.vertices field.
    XYZ delta-decoded from 3 interleaved byte streams.
    Each vertex is 3 uint8 values (0-255).

    Returns array of shape (count, 3) dtype uint8.
    """
    count = len(packed) // 3
    if count == 0:
        return np.zeros((0, 3), dtype=np.uint8)

    data = np.frombuffer(packed, dtype=np.uint8)
    x_stream = data[0:count]
    y_stream = data[count : 2 * count]
    z_stream = data[2 * count : 3 * count]

    # Delta decode with uint8 wrapping
    x = np.cumsum(x_stream, dtype=np.uint8)
    y = np.cumsum(y_stream, dtype=np.uint8)
    z = np.cumsum(z_stream, dtype=np.uint8)

    return np.stack([x, y, z], axis=1)


def unpack_tex_coords(packed: bytes, vertex_count: int) -> tuple[np.ndarray, float, float]:
    """
    Unpack texture coordinates from Mesh.texture_coordinates field.
    Delta-decoded UV coordinates.

    Returns (uvs array of shape (count, 2) dtype uint16, u_mod, v_mod).
    """
    if len(packed) < 4:
        return np.zeros((vertex_count, 2), dtype=np.uint16), 1, 1

    u_mod = 1 + struct.unpack_from("<H", packed, 0)[0]
    v_mod = 1 + struct.unpack_from("<H", packed, 2)[0]
    data = packed[4:]

    assert len(data) == vertex_count * 4, (
        f"tex_coords data size mismatch: {len(data)} != {vertex_count * 4}"
    )

    uvs = np.zeros((vertex_count, 2), dtype=np.int32)
    u = 0
    v = 0
    for i in range(vertex_count):
        u = (u + data[vertex_count * 0 + i] + (data[vertex_count * 2 + i] << 8)) % u_mod
        v = (v + data[vertex_count * 1 + i] + (data[vertex_count * 3 + i] << 8)) % v_mod
        uvs[i, 0] = u
        uvs[i, 1] = v

    return uvs.astype(np.uint16), u_mod, v_mod


def unpack_indices(packed: bytes) -> np.ndarray:
    """
    Unpack triangle indices from Mesh.indices field.
    Decodes varint-encoded triangle strip, then converts to individual triangles.

    Returns array of shape (n_triangles * 3,) dtype uint32.
    """
    offset = 0
    strip_len, offset = _read_varint(packed, offset)

    # Decode triangle strip
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

    # Convert triangle strip to individual triangles
    triangles = []
    for i in range(strip_len - 2):
        a = strip[i]
        b_val = strip[i + 1]
        c_val = strip[i + 2]
        if a == b_val or a == c_val or b_val == c_val:
            continue  # degenerate triangle
        if i & 1:
            triangles.extend([a, c_val, b_val])
        else:
            triangles.extend([a, b_val, c_val])

    if not triangles:
        return np.array([], dtype=np.uint32)
    return np.array(triangles, dtype=np.uint32)


def unpack_octant_mask_and_layer_bounds(
    packed: bytes,
    indices: np.ndarray,
    vertex_count: int,
) -> tuple[np.ndarray, list[int]]:
    """
    Unpack octant mask (W value per vertex) and layer bounds.
    Returns (w_mask array of shape (vertex_count,) dtype uint8, layer_bounds list).
    """
    w_mask = np.zeros(vertex_count, dtype=np.uint8)
    layer_bounds = [0] * 10

    if not packed:
        # No octant data — all indices are valid
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
    """
    Unpack the for_normals field from NodeData.
    Octahedral normal encoding.

    Returns array of shape (count, 3) dtype uint8, or None if no data.
    """
    if not for_normals_bytes or len(for_normals_bytes) < 3:
        return None

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
    """
    Unpack normals for a mesh using mesh_normals indices and for_normals table.
    Returns array of shape (vertex_count, 3) dtype float32, normalized.
    """
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
        # Default normals
        normals = np.full((vertex_count, 3), [0, 0, 1], dtype=np.float32)

    # Normalize
    norms = np.linalg.norm(normals, axis=1, keepdims=True)
    norms = np.where(norms > 0, norms, 1.0)
    normals = normals / norms

    return normals


def apply_matrix(
    vertices_u8: np.ndarray,
    matrix: list[float],
) -> np.ndarray:
    """
    Transform local 8-bit vertex coordinates to world (ECEF) coordinates
    using the 4x4 matrix_globe_from_mesh.

    The matrix is stored as 16 doubles in column-major order.

    Returns array of shape (n, 3) dtype float64.
    """
    n = len(vertices_u8)
    if n == 0:
        return np.zeros((0, 3), dtype=np.float64)

    # Build 4x4 column-major matrix
    M = np.array(matrix, dtype=np.float64).reshape(4, 4).T  # column-major → row-major

    # Homogeneous coordinates: [x, y, z, 1]
    pts = np.ones((n, 4), dtype=np.float64)
    pts[:, 0] = vertices_u8[:, 0].astype(np.float64)
    pts[:, 1] = vertices_u8[:, 1].astype(np.float64)
    pts[:, 2] = vertices_u8[:, 2].astype(np.float64)

    # Transform
    result = pts @ M.T  # (n, 4) @ (4, 4).T = (n, 4)
    return result[:, :3]


def transform_normals(
    normals: np.ndarray,
    matrix: list[float],
) -> np.ndarray:
    """
    Transform normals using the 3x3 rotation part of the matrix.
    (Normals use the inverse-transpose, but since the matrix is
    orthogonal for rotation, we can just use the upper-left 3x3.)
    """
    if len(normals) == 0:
        return normals

    M = np.array(matrix, dtype=np.float64).reshape(4, 4).T
    rot = M[:3, :3]

    result = normals.astype(np.float64) @ rot.T
    # Re-normalize
    norms = np.linalg.norm(result, axis=1, keepdims=True)
    norms = np.where(norms > 0, norms, 1.0)
    return (result / norms).astype(np.float32)


class DecodedMesh:
    """A fully decoded mesh ready for GLB conversion."""

    def __init__(self):
        self.positions: np.ndarray = np.array([])  # (n, 3) float64 ECEF
        self.normals: np.ndarray = np.array([])  # (n, 3) float32
        self.uvs: np.ndarray = np.array([])  # (n, 2) float32
        self.indices: np.ndarray = np.array([])  # (m,) uint32
        self.texture_data: bytes = b""  # raw JPG bytes
        self.texture_width: int = 256
        self.texture_height: int = 256
        self.texture_format: int = 1  # 1=JPG, 6=CRN_DXT1


def decode_node(node_data: pb.NodeData) -> list[DecodedMesh]:
    """
    Fully decode all meshes from a NodeData protobuf message.
    Returns a list of DecodedMesh objects.
    """
    matrix = list(node_data.matrix_globe_from_mesh)
    if len(matrix) != 16:
        raise ValueError(f"Expected 16 matrix values, got {len(matrix)}")

    # Unpack shared normals table
    for_normals = None
    if node_data.HasField("for_normals") and len(node_data.for_normals) > 0:
        for_normals = unpack_for_normals(node_data.for_normals)

    meshes = []
    for mesh_pb in node_data.meshes:
        dm = DecodedMesh()

        # 1. Unpack vertices (uint8 XYZ)
        raw_verts = unpack_vertices(mesh_pb.vertices)
        vertex_count = len(raw_verts)

        if vertex_count == 0:
            continue

        # 2. Unpack texture coordinates
        tex_coords_raw = mesh_pb.texture_coordinates if mesh_pb.HasField("texture_coordinates") else b""
        raw_uvs, u_mod, v_mod = unpack_tex_coords(tex_coords_raw, vertex_count)

        # 3. Unpack indices
        raw_indices = unpack_indices(mesh_pb.indices)

        # 4. Unpack octant mask and layer bounds
        layer_data = mesh_pb.layer_and_octant_counts if mesh_pb.HasField("layer_and_octant_counts") else b""
        _, layer_bounds = unpack_octant_mask_and_layer_bounds(
            layer_data, raw_indices, vertex_count
        )

        # Truncate indices to renderable geometry (layer_bounds[3])
        max_idx = min(layer_bounds[3], len(raw_indices))
        raw_indices = raw_indices[:max_idx]

        if len(raw_indices) == 0:
            continue

        # 5. Apply UV offset and scale
        if len(mesh_pb.uv_offset_and_scale) == 4:
            uv_offset = (mesh_pb.uv_offset_and_scale[0], mesh_pb.uv_offset_and_scale[1])
            uv_scale = (mesh_pb.uv_offset_and_scale[2], mesh_pb.uv_offset_and_scale[3])
        else:
            uv_offset = (0.5, 0.5 - 1.0 / v_mod if v_mod > 0 else 0.5)
            uv_scale = (1.0 / u_mod if u_mod > 0 else 1.0, -1.0 / v_mod if v_mod > 0 else -1.0)

        uvs_float = np.zeros((vertex_count, 2), dtype=np.float32)
        uvs_float[:, 0] = (raw_uvs[:, 0].astype(np.float32) + uv_offset[0]) * uv_scale[0]
        uvs_float[:, 1] = (raw_uvs[:, 1].astype(np.float32) + uv_offset[1]) * uv_scale[1]
        dm.uvs = uvs_float

        # 6. Unpack normals
        mesh_normals = mesh_pb.normals if mesh_pb.HasField("normals") else b""
        normals = unpack_normals(mesh_normals, for_normals, vertex_count)

        # 7. Transform vertices and normals to world coordinates
        dm.positions = apply_matrix(raw_verts, matrix)
        dm.normals = transform_normals(normals, matrix)
        dm.indices = raw_indices

        # 8. Extract texture
        if len(mesh_pb.texture) > 0:
            tex = mesh_pb.texture[0]
            if len(tex.data) > 0:
                dm.texture_data = tex.data[0]
            dm.texture_format = tex.format if tex.HasField("format") else 1
            dm.texture_width = tex.width if tex.HasField("width") else 256
            dm.texture_height = tex.height if tex.HasField("height") else 256

        meshes.append(dm)

    return meshes
