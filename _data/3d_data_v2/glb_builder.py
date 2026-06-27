"""
GLB file builder for decoded Google Earth meshes.

Constructs GLB binary files using pygltflib from DecodedMesh objects.
Handles vertex positions, normals, UVs, indices, and JPG textures.
"""

import io
import struct
import numpy as np
from PIL import Image
import pygltflib

from mesh_decoder import DecodedMesh


def build_glb(
    meshes: list[DecodedMesh],
    octant_path: str,
    reference_point: np.ndarray | None = None,
) -> bytes:
    """
    Build a GLB file from decoded meshes.

    If reference_point is provided, vertex positions are offset by subtracting it
    (to keep coordinates near-origin for game engine use).

    Returns the GLB file content as bytes.
    """
    if not meshes:
        return b""

    gltf = pygltflib.GLTF2(
        asset=pygltflib.Asset(version="2.0", generator="earth-tile-exporter"),
        scene=0,
        scenes=[pygltflib.Scene(nodes=list(range(len(meshes))))],
        nodes=[],
        meshes=[],
        accessors=[],
        bufferViews=[],
        buffers=[],
        materials=[],
        textures=[],
        images=[],
        samplers=[],
    )

    # Single buffer for all binary data
    buffer_data = bytearray()

    for mesh_idx, dm in enumerate(meshes):
        if len(dm.positions) == 0 or len(dm.indices) == 0:
            continue

        # Offset and rotate positions/normals to local ENU tangent plane
        positions = dm.positions.copy()
        normals = dm.normals.copy()
        if reference_point is not None:
            positions -= reference_point
            
            ref_x, ref_y, ref_z = reference_point
            ref_len = np.linalg.norm(reference_point)
            if ref_len > 0:
                # Up vector
                u_x = ref_x / ref_len
                u_y = ref_y / ref_len
                u_z = ref_z / ref_len
                
                # East vector
                east_len = np.sqrt(ref_x**2 + ref_y**2)
                if east_len > 0:
                    e_x = -ref_y / east_len
                    e_y = ref_x / east_len
                    e_z = 0.0
                else:
                    e_x = 1.0
                    e_y = 0.0
                    e_z = 0.0
                
                # North vector (U x E)
                n_x = u_y * e_z - u_z * e_y
                n_y = u_z * e_x - u_x * e_z
                n_z = u_x * e_y - u_y * e_x
                
                # Rotate from ECEF to local tangent plane in GLTF convention (Y-up):
                # Row 0 (X) = East
                # Row 1 (Y) = Up
                # Row 2 (Z) = South (-North)
                R = np.array([
                    [e_x, e_y, e_z],
                    [u_x, u_y, u_z],
                    [-n_x, -n_y, -n_z]
                ], dtype=np.float64)
                
                positions = positions @ R.T
                normals = normals @ R.T

        positions_f32 = positions.astype(np.float32)
        normals_f32 = normals.astype(np.float32)
        uvs_f32 = dm.uvs.astype(np.float32)

        # Determine index type
        max_index = int(dm.indices.max())
        if max_index < 65536:
            indices_data = dm.indices.astype(np.uint16)
            index_component_type = pygltflib.UNSIGNED_SHORT
        else:
            indices_data = dm.indices.astype(np.uint32)
            index_component_type = pygltflib.UNSIGNED_INT

        # -- Material and texture --
        material_idx = len(gltf.materials)
        tex_idx = len(gltf.textures)
        img_idx = len(gltf.images)

        # Encode texture as PNG for GLB embedding
        tex_bytes = _prepare_texture(dm)

        if tex_bytes:
            # Image buffer view
            img_bv_start = len(buffer_data)
            buffer_data.extend(tex_bytes)
            _pad_to_4(buffer_data)
            img_bv_idx = len(gltf.bufferViews)
            gltf.bufferViews.append(
                pygltflib.BufferView(
                    buffer=0,
                    byteOffset=img_bv_start,
                    byteLength=len(tex_bytes),
                )
            )

            gltf.images.append(
                pygltflib.Image(
                    bufferView=img_bv_idx,
                    mimeType="image/png",
                )
            )

            if not gltf.samplers:
                gltf.samplers.append(
                    pygltflib.Sampler(
                        magFilter=pygltflib.LINEAR,
                        minFilter=pygltflib.LINEAR_MIPMAP_LINEAR,
                        wrapS=pygltflib.CLAMP_TO_EDGE,
                        wrapT=pygltflib.CLAMP_TO_EDGE,
                    )
                )

            gltf.textures.append(
                pygltflib.Texture(
                    sampler=0,
                    source=img_idx,
                )
            )

            gltf.materials.append(
                pygltflib.Material(
                    pbrMetallicRoughness=pygltflib.PbrMetallicRoughness(
                        baseColorTexture=pygltflib.TextureInfo(index=tex_idx),
                        metallicFactor=0.0,
                        roughnessFactor=1.0,
                    ),
                    doubleSided=True,
                )
            )
        else:
            gltf.materials.append(
                pygltflib.Material(
                    pbrMetallicRoughness=pygltflib.PbrMetallicRoughness(
                        metallicFactor=0.0,
                        roughnessFactor=1.0,
                    ),
                    doubleSided=True,
                )
            )

        # -- Indices buffer view + accessor --
        idx_bytes = indices_data.tobytes()
        idx_bv_start = len(buffer_data)
        buffer_data.extend(idx_bytes)
        _pad_to_4(buffer_data)
        idx_bv_idx = len(gltf.bufferViews)
        gltf.bufferViews.append(
            pygltflib.BufferView(
                buffer=0,
                byteOffset=idx_bv_start,
                byteLength=len(idx_bytes),
                target=pygltflib.ELEMENT_ARRAY_BUFFER,
            )
        )

        idx_accessor_idx = len(gltf.accessors)
        gltf.accessors.append(
            pygltflib.Accessor(
                bufferView=idx_bv_idx,
                byteOffset=0,
                componentType=index_component_type,
                count=len(indices_data),
                type=pygltflib.SCALAR,
                max=[int(indices_data.max())],
                min=[int(indices_data.min())],
            )
        )

        # -- Position buffer view + accessor --
        pos_bytes = positions_f32.tobytes()
        pos_bv_start = len(buffer_data)
        buffer_data.extend(pos_bytes)
        _pad_to_4(buffer_data)
        pos_bv_idx = len(gltf.bufferViews)
        gltf.bufferViews.append(
            pygltflib.BufferView(
                buffer=0,
                byteOffset=pos_bv_start,
                byteLength=len(pos_bytes),
                target=pygltflib.ARRAY_BUFFER,
            )
        )

        pos_accessor_idx = len(gltf.accessors)
        gltf.accessors.append(
            pygltflib.Accessor(
                bufferView=pos_bv_idx,
                byteOffset=0,
                componentType=pygltflib.FLOAT,
                count=len(positions_f32),
                type=pygltflib.VEC3,
                max=positions_f32.max(axis=0).tolist(),
                min=positions_f32.min(axis=0).tolist(),
            )
        )

        # -- Normal buffer view + accessor --
        norm_bytes = normals_f32.tobytes()
        norm_bv_start = len(buffer_data)
        buffer_data.extend(norm_bytes)
        _pad_to_4(buffer_data)
        norm_bv_idx = len(gltf.bufferViews)
        gltf.bufferViews.append(
            pygltflib.BufferView(
                buffer=0,
                byteOffset=norm_bv_start,
                byteLength=len(norm_bytes),
                target=pygltflib.ARRAY_BUFFER,
            )
        )

        norm_accessor_idx = len(gltf.accessors)
        gltf.accessors.append(
            pygltflib.Accessor(
                bufferView=norm_bv_idx,
                byteOffset=0,
                componentType=pygltflib.FLOAT,
                count=len(normals_f32),
                type=pygltflib.VEC3,
            )
        )

        # -- UV buffer view + accessor --
        uv_bytes = uvs_f32.tobytes()
        uv_bv_start = len(buffer_data)
        buffer_data.extend(uv_bytes)
        _pad_to_4(buffer_data)
        uv_bv_idx = len(gltf.bufferViews)
        gltf.bufferViews.append(
            pygltflib.BufferView(
                buffer=0,
                byteOffset=uv_bv_start,
                byteLength=len(uv_bytes),
                target=pygltflib.ARRAY_BUFFER,
            )
        )

        uv_accessor_idx = len(gltf.accessors)
        gltf.accessors.append(
            pygltflib.Accessor(
                bufferView=uv_bv_idx,
                byteOffset=0,
                componentType=pygltflib.FLOAT,
                count=len(uvs_f32),
                type=pygltflib.VEC2,
            )
        )

        # -- Mesh primitive --
        primitive = pygltflib.Primitive(
            attributes=pygltflib.Attributes(
                POSITION=pos_accessor_idx,
                NORMAL=norm_accessor_idx,
                TEXCOORD_0=uv_accessor_idx,
            ),
            indices=idx_accessor_idx,
            material=material_idx,
        )

        gltf_mesh_idx = len(gltf.meshes)
        gltf.meshes.append(
            pygltflib.Mesh(
                primitives=[primitive],
                name=f"tile_{octant_path}_mesh{mesh_idx}",
            )
        )

        gltf.nodes.append(
            pygltflib.Node(
                mesh=gltf_mesh_idx,
                name=f"node_{octant_path}_mesh{mesh_idx}",
            )
        )

    # Set buffer
    gltf.buffers.append(
        pygltflib.Buffer(byteLength=len(buffer_data))
    )

    # Set binary blob
    gltf.set_binary_blob(bytes(buffer_data))

    # Serialize to GLB bytes
    glb_bytes = b"".join(gltf.save_to_bytes())
    return glb_bytes


def _pad_to_4(data: bytearray):
    """Pad bytearray to 4-byte alignment."""
    while len(data) % 4 != 0:
        data.append(0)


def _prepare_texture(dm: DecodedMesh) -> bytes | None:
    """
    Prepare texture data for GLB embedding.
    Converts JPG to PNG. Returns PNG bytes or None.
    """
    if not dm.texture_data:
        return None

    try:
        if dm.texture_format == 1:  # JPG
            img = Image.open(io.BytesIO(dm.texture_data))
            buf = io.BytesIO()
            img.save(buf, format="PNG")
            return buf.getvalue()
        else:
            # For CRN-DXT1 or other formats, we'd need decompression
            # For now, skip unsupported formats
            return None
    except Exception as e:
        print(f"Warning: Failed to decode texture: {e}")
        return None
