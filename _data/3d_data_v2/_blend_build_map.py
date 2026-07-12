"""
Blender batch script: drape OSM road centerlines and car markers onto photogrammetry GLBs.

Invoked as:
    blender -b -P _blend_build_map.py -- <batch.json>
"""

import json
import os
import sys
from math import acos, degrees

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

APEX_OFFSET = 3.0
COLLIDER_SCALE = 1.2
MASK_SIZE = 512
CAR_GRAY = 0.35
CLASSIFY_MASK_SIZE = 512
NORMAL_ANGLE_THRESHOLD_DEG = 30.0


def clear_scene():
    """Wipe the current scene so the next tile imports into a clean slate."""
    bpy.ops.wm.read_factory_settings(use_empty=True)


def measure_terrain_bbox() -> dict:
    """
    Compute axis-aligned bounds of all mesh objects in Blender coords (ENU).
    Returns east/north/up min-max lists.
    """
    east_min = north_min = up_min = float("inf")
    east_max = north_max = up_max = float("-inf")
    found = False

    for obj in bpy.data.objects:
        if obj.type != "MESH":
            continue
        found = True
        matrix = obj.matrix_world
        for corner in obj.bound_box:
            world = matrix @ Vector(corner)
            east_min = min(east_min, world.x)
            east_max = max(east_max, world.x)
            north_min = min(north_min, world.y)
            north_max = max(north_max, world.y)
            up_min = min(up_min, world.z)
            up_max = max(up_max, world.z)

    if not found:
        raise RuntimeError("No mesh objects found after GLB import")

    return {
        "east": [east_min, east_max],
        "north": [north_min, north_max],
        "up": [up_min, up_max],
    }


def latlon_to_xy(lon: float, lat: float, latlon_bbox: dict, terrain_bbox: dict) -> tuple[float, float]:
    """Map lat/lon to Blender (east, north) via bilinear extrapolation."""
    west = latlon_bbox["west"]
    east_lon = latlon_bbox["east"]
    south = latlon_bbox["south"]
    north = latlon_bbox["north"]

    fx = (lon - west) / (east_lon - west)
    fy = (lat - south) / (north - south)

    east_min, east_max = terrain_bbox["east"]
    north_min, north_max = terrain_bbox["north"]

    x = east_min + fx * (east_max - east_min)
    y = north_min + fy * (north_max - north_min)
    return x, y


def build_terrain_bvh(terrain_obj: bpy.types.Object | None) -> BVHTree | None:
    """
    Build a world-space BVH of `terrain_obj` once, up front. All ray casts in this
    script query this single static tree instead of calling into Blender's per-object
    ray_cast (which, called tens of thousands of times across the per-pixel classify
    loop, is both far slower and has been observed to crash Blender outright). Building
    once here also means road/car objects created later can never be hit by a ray,
    since they were never part of the tree to begin with.
    """
    if terrain_obj is None:
        return None
    mesh = terrain_obj.data
    mesh.calc_loop_triangles()
    n = len(mesh.vertices)
    co = np.empty(n * 3, dtype=np.float64)
    mesh.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    mw = np.array(terrain_obj.matrix_world)
    world = (np.hstack([co, np.ones((n, 1))]) @ mw.T)[:, :3]
    verts = [Vector(v) for v in world]
    tris = [tuple(lt.vertices) for lt in mesh.loop_triangles]
    if not tris:
        return None
    return BVHTree.FromPolygons(verts, tris, all_triangles=True)


def raycast_hit(
    x: float, y: float, top: float, bvh: BVHTree | None
) -> tuple[float, Vector] | None:
    """Cast downward from above the terrain bbox; return (hit z, hit normal) or None."""
    if bvh is None:
        return None
    loc, normal, _index, _dist = bvh.ray_cast(
        Vector((x, y, top + 1.0)), Vector((0.0, 0.0, -1.0))
    )
    if loc is None:
        return None
    return loc.z, normal.normalized()


def raycast_height(x: float, y: float, top: float, bvh: BVHTree | None) -> float | None:
    """Cast downward from above the terrain bbox; return hit z or None."""
    hit = raycast_hit(x, y, top, bvh)
    return hit[0] if hit else None


def resolve_heights(heights: list[float | None]) -> list[float] | None:
    """
    Fill ray-cast misses from nearest chain neighbor with a hit.
    Returns None if the whole road has zero hits.
    """
    n = len(heights)
    if n == 0:
        return None

    hits = [h is not None for h in heights]
    if not any(hits):
        return None

    resolved = list(heights)
    for i in range(n):
        if resolved[i] is not None:
            continue
        for dist in range(1, n):
            for j in (i - dist, i + dist):
                if 0 <= j < n and resolved[j] is not None:
                    resolved[i] = resolved[j]
                    break
            if resolved[i] is not None:
                break
    return resolved


def get_or_create_collection(name: str) -> bpy.types.Collection:
    coll = bpy.data.collections.get(name)
    if coll is None:
        coll = bpy.data.collections.new(name)
        bpy.context.scene.collection.children.link(coll)
    return coll


def create_road_object(
    feature_id,
    coords_xy: list[tuple[float, float]],
    heights: list[float],
) -> bpy.types.Object:
    """Create a mesh polyline named road_<feature_id> in the roads collection."""
    verts = [(x, y, z) for (x, y), z in zip(coords_xy, heights)]
    edges = [(i, i + 1) for i in range(len(verts) - 1)]

    mesh = bpy.data.meshes.new(name=f"road_{feature_id}_mesh")
    mesh.from_pydata(verts, edges, [])
    mesh.update()

    obj = bpy.data.objects.new(name=f"road_{feature_id}", object_data=mesh)
    roads_coll = get_or_create_collection("roads")
    roads_coll.objects.link(obj)
    return obj


def resolve_corner_heights(raw_zs: list[float | None], top: float) -> list[float]:
    """Fill corner ray misses with the average z of the corners that hit (else top)."""
    hits = [z for z in raw_zs if z is not None]
    avg = sum(hits) / len(hits) if hits else top
    return [z if z is not None else avg for z in raw_zs]


def convex_hull_2d(points: list[tuple[float, float]]) -> list[tuple[float, float]]:
    """Andrew's monotone chain; returns CCW hull (>=3 unique points) or [] otherwise."""
    pts = sorted(set((round(x, 6), round(y, 6)) for x, y in points))
    if len(pts) < 3:
        return pts

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower = []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    upper = []
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return lower[:-1] + upper[:-1]


def build_collider_mesh(
    corners_latlon: list[list[float]],
    center_latlon: list[float],
    latlon_bbox: dict,
    terrain_bbox: dict,
    top: float,
    bvh: BVHTree | None,
) -> dict:
    """
    Build a box collider molded onto the terrain. Returns a dict with verts, faces,
    hull_xy, and the ground reference (normal + max height) sampled at the corners
    plus the roof height, used later to classify car-vs-ground pixels.
    """
    base_xy: list[tuple[float, float]] = []
    raw_z: list[float | None] = []
    raw_normal: list[Vector | None] = []
    for lat, lon in corners_latlon:
        x, y = latlon_to_xy(lon, lat, latlon_bbox, terrain_bbox)
        base_xy.append((x, y))
        hit = raycast_hit(x, y, top, bvh)
        raw_z.append(hit[0] if hit else None)
        raw_normal.append(hit[1] if hit else None)
    ground_z = resolve_corner_heights(raw_z, top)

    ground_hits_z = [z for z in raw_z if z is not None]
    ground_max_z = max(ground_hits_z) if ground_hits_z else top
    ground_normals = [n for n in raw_normal if n is not None]
    if ground_normals:
        ground_normal = Vector((0.0, 0.0, 0.0))
        for n in ground_normals:
            ground_normal += n
        ground_normal.normalize()
    else:
        ground_normal = Vector((0.0, 0.0, 1.0))

    cx, cy = latlon_to_xy(center_latlon[1], center_latlon[0], latlon_bbox, terrain_bbox)
    center_z = raycast_height(cx, cy, top, bvh)
    if center_z is None:
        center_z = sum(ground_z) / len(ground_z)
    apex_z = center_z + APEX_OFFSET
    min_ground = min(ground_z)

    bm = bmesh.new()
    bottom = [bm.verts.new((base_xy[i][0], base_xy[i][1], ground_z[i])) for i in range(4)]
    topv = [bm.verts.new((base_xy[i][0], base_xy[i][1], apex_z)) for i in range(4)]
    bm.faces.new(tuple(reversed(bottom)))
    bm.faces.new(tuple(topv))
    for i in range(4):
        j = (i + 1) % 4
        bm.faces.new((bottom[i], bottom[j], topv[j], topv[i]))

    bmesh.ops.subdivide_edges(
        bm, edges=bm.edges[:], cuts=3, use_grid_fill=True
    )
    bm.verts.ensure_lookup_table()

    # Mold the top face down onto the terrain, column by column (each XY position
    # shared by a ground vertex and a top vertex, plus any side-wall levels between
    # them). Raycasting only the top-of-column vertex and re-interpolating the rest
    # keeps side-wall vertices attached to the new (lowered) top instead of the old
    # flat apex — otherwise they linger at their pre-mold height and create a rim
    # around the border where it looks "unlowered" relative to the interior.
    columns: dict[tuple[float, float], list] = {}
    for v in bm.verts:
        columns.setdefault((round(v.co.x, 4), round(v.co.y, 4)), []).append(v)
    for verts_in_col in columns.values():
        verts_in_col.sort(key=lambda v: v.co.z)

    top_hits: list[float | None] = [
        raycast_height(verts_in_col[-1].co.x, verts_in_col[-1].co.y, top, bvh)
        for verts_in_col in columns.values()
    ]
    resolved_top = resolve_corner_heights(top_hits, (min_ground + apex_z) / 2.0)

    for verts_in_col, present in zip(columns.values(), resolved_top):
        new_top_z = (apex_z + present) / 2.0
        ground_z_col = verts_in_col[0].co.z
        n = len(verts_in_col)
        for i, v in enumerate(verts_in_col):
            frac = i / (n - 1) if n > 1 else 1.0
            v.co.z = ground_z_col + frac * (new_top_z - ground_z_col)

    # Enlarge 20% about the centroid.
    cog = Vector((0.0, 0.0, 0.0))
    for v in bm.verts:
        cog += v.co
    cog /= len(bm.verts)
    for v in bm.verts:
        v.co = cog + COLLIDER_SCALE * (v.co - cog)

    bm.verts.index_update()
    verts = [tuple(v.co) for v in bm.verts]
    faces = [[v.index for v in f.verts] for f in bm.faces]
    hull = convex_hull_2d([(v[0], v[1]) for v in verts])
    bm.free()
    return {
        "verts": verts,
        "faces": faces,
        "hull": hull,
        "ground_normal": ground_normal,
        "ground_max_z": ground_max_z,
        "roof_z": apex_z,
    }


def create_car_object(
    car_index: int,
    verts: list[tuple[float, float, float]],
    faces: list[list[int]],
) -> bpy.types.Object:
    """Link a pre-built collider mesh into the cars collection."""
    mesh = bpy.data.meshes.new(name=f"car_{car_index}_mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()

    obj = bpy.data.objects.new(name=f"car_{car_index}", object_data=mesh)
    get_or_create_collection("cars").objects.link(obj)
    return obj


def _point_in_hull(px, py, hull: np.ndarray):
    """Vectorized point-in-convex-CCW-polygon test."""
    inside = np.ones(px.shape, dtype=bool)
    m = len(hull)
    for i in range(m):
        x1, y1 = hull[i]
        x2, y2 = hull[(i + 1) % m]
        cross = (x2 - x1) * (py - y1) - (y2 - y1) * (px - x1)
        inside &= cross >= -1e-9
    return inside


def paint_car_mask(
    terrain_obj: bpy.types.Object,
    hulls: list[list[tuple[float, float]]],
    image_name: str,
) -> bpy.types.Image | None:
    """
    Rasterize car footprints into a black mask image using the terrain's own UV unwrap.
    A texel is painted white when its world XY (barycentric-interpolated from the tile
    triangle) falls inside any car footprint hull.
    """
    mesh = terrain_obj.data
    if not mesh.uv_layers or not mesh.materials:
        return None

    hull_arrays = [np.asarray(h, dtype=np.float64) for h in hulls if len(h) >= 3]
    if not hull_arrays:
        return None
    hull_bboxes = [
        (h[:, 0].min(), h[:, 0].max(), h[:, 1].min(), h[:, 1].max()) for h in hull_arrays
    ]

    uv_layer = mesh.uv_layers.active or mesh.uv_layers[0]
    uv_data = uv_layer.data
    mw = np.array(terrain_obj.matrix_world)

    n = len(mesh.vertices)
    co = np.empty(n * 3, dtype=np.float64)
    mesh.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    world = (np.hstack([co, np.ones((n, 1))]) @ mw.T)[:, :3]

    mesh.calc_loop_triangles()
    size = MASK_SIZE
    mask = np.zeros((size, size), dtype=np.float32)

    for lt in mesh.loop_triangles:
        vs = lt.vertices
        ls = lt.loops
        w0, w1, w2 = world[vs[0]], world[vs[1]], world[vs[2]]
        u0 = uv_data[ls[0]].uv
        u1 = uv_data[ls[1]].uv
        u2 = uv_data[ls[2]].uv
        p0 = (u0[0] * size, u0[1] * size)
        p1 = (u1[0] * size, u1[1] * size)
        p2 = (u2[0] * size, u2[1] * size)

        minx = max(int(np.floor(min(p0[0], p1[0], p2[0]))), 0)
        maxx = min(int(np.ceil(max(p0[0], p1[0], p2[0]))), size - 1)
        miny = max(int(np.floor(min(p0[1], p1[1], p2[1]))), 0)
        maxy = min(int(np.ceil(max(p0[1], p1[1], p2[1]))), size - 1)
        if minx > maxx or miny > maxy:
            continue

        denom = (p1[1] - p2[1]) * (p0[0] - p2[0]) + (p2[0] - p1[0]) * (p0[1] - p2[1])
        if abs(denom) < 1e-12:
            continue

        gx, gy = np.meshgrid(
            np.arange(minx, maxx + 1) + 0.5, np.arange(miny, maxy + 1) + 0.5
        )
        a = ((p1[1] - p2[1]) * (gx - p2[0]) + (p2[0] - p1[0]) * (gy - p2[1])) / denom
        b = ((p2[1] - p0[1]) * (gx - p2[0]) + (p0[0] - p2[0]) * (gy - p2[1])) / denom
        c = 1.0 - a - b
        inside = (a >= 0) & (b >= 0) & (c >= 0)
        if not inside.any():
            continue

        wx = a * w0[0] + b * w1[0] + c * w2[0]
        wy = a * w0[1] + b * w1[1] + c * w2[1]
        painted = np.zeros_like(inside)
        for hull, bb in zip(hull_arrays, hull_bboxes):
            sel = inside & (wx >= bb[0]) & (wx <= bb[1]) & (wy >= bb[2]) & (wy <= bb[3])
            if not sel.any():
                continue
            painted |= sel & _point_in_hull(wx, wy, hull)
        if painted.any():
            mask[miny : maxy + 1, minx : maxx + 1][painted] = 1.0

    if not mask.any():
        return None

    return _make_mask_image(image_name, mask)


def attach_mask_material(
    terrain_obj: bpy.types.Object, mask_img: bpy.types.Image, mat_name: str
) -> None:
    """
    Append an inspection-only material slot showing a raw mask texture.
    Appending a material slot without reassigning any polygon's material_index
    leaves every face rendering with slot 0 (the original tile material) — this
    slot exists purely so the mask can be opened and viewed by hand.
    """
    mat = bpy.data.materials.new(name=mat_name)
    if not mat.use_nodes:
        mat.use_nodes = True
    nt = mat.node_tree
    for node in list(nt.nodes):
        nt.nodes.remove(node)

    tex_node = nt.nodes.new("ShaderNodeTexImage")
    tex_node.image = mask_img
    tex_node.label = mask_img.name

    emit_node = nt.nodes.new("ShaderNodeEmission")
    output_node = nt.nodes.new("ShaderNodeOutputMaterial")
    nt.links.new(tex_node.outputs["Color"], emit_node.inputs["Color"])
    nt.links.new(emit_node.outputs["Emission"], output_node.inputs["Surface"])

    terrain_obj.data.materials.append(mat)


def _make_mask_image(name: str, mask: np.ndarray) -> bpy.types.Image:
    size = mask.shape[0]
    existing = bpy.data.images.get(name)
    if existing is not None:
        bpy.data.images.remove(existing)
    img = bpy.data.images.new(name, width=size, height=size, alpha=False)
    img.colorspace_settings.name = "Non-Color"
    rgba = np.zeros((size, size, 4), dtype=np.float32)
    rgba[..., 0] = mask
    rgba[..., 1] = mask
    rgba[..., 2] = mask
    rgba[..., 3] = 1.0
    img.pixels.foreach_set(rgba.reshape(-1))
    img.update()
    img.pack()
    return img


def paint_car_ground_air_masks(
    terrain_obj: bpy.types.Object,
    car_refs: list[dict],
    image_prefix: str,
    bvh: BVHTree | None,
    top: float,
) -> tuple[bpy.types.Image | None, bpy.types.Image | None]:
    """
    Split the car silhouette into a "_car_ground" mask (ground around the car) and
    a "_car_air" mask (the car body itself), by raycasting every pixel of a small
    (CLASSIFY_MASK_SIZE) working mask and comparing the hit normal/height against
    each car's ground reference sampled at its bbox corners (see build_collider_mesh):

    - normal_component: 1.0 if the hit normal differs from the ground normal by more
      than NORMAL_ANGLE_THRESHOLD_DEG, else 0.0.
    - height_component: 0.0 at the highest ground corner, 1.0 at the midpoint between
      that height and the car roof (apex_z), linear in between.
    - combined = clamp(normal_component + height_component, 0, 1) is the "_car_air"
      value for that pixel; "_car_ground" is its complement.

    Only pixels that fall inside a car hull get raycast, so the ray count stays small
    even though the mesh itself may have many more triangles than mask pixels.
    """
    mesh = terrain_obj.data
    if not mesh.uv_layers or not mesh.materials:
        return None, None

    valid_refs = [ref for ref in car_refs if len(ref["hull"]) >= 3]
    hull_arrays = [np.asarray(ref["hull"], dtype=np.float64) for ref in valid_refs]
    if not hull_arrays:
        return None, None
    hull_bboxes = [
        (h[:, 0].min(), h[:, 0].max(), h[:, 1].min(), h[:, 1].max()) for h in hull_arrays
    ]

    uv_layer = mesh.uv_layers.active or mesh.uv_layers[0]
    uv_data = uv_layer.data
    mw = np.array(terrain_obj.matrix_world)

    n = len(mesh.vertices)
    co = np.empty(n * 3, dtype=np.float64)
    mesh.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    world = (np.hstack([co, np.ones((n, 1))]) @ mw.T)[:, :3]

    mesh.calc_loop_triangles()
    size = CLASSIFY_MASK_SIZE
    ground_mask = np.zeros((size, size), dtype=np.float32)
    air_mask = np.zeros((size, size), dtype=np.float32)

    for lt in mesh.loop_triangles:
        vs = lt.vertices
        ls = lt.loops
        w0, w1, w2 = world[vs[0]], world[vs[1]], world[vs[2]]
        u0 = uv_data[ls[0]].uv
        u1 = uv_data[ls[1]].uv
        u2 = uv_data[ls[2]].uv
        p0 = (u0[0] * size, u0[1] * size)
        p1 = (u1[0] * size, u1[1] * size)
        p2 = (u2[0] * size, u2[1] * size)

        minx = max(int(np.floor(min(p0[0], p1[0], p2[0]))), 0)
        maxx = min(int(np.ceil(max(p0[0], p1[0], p2[0]))), size - 1)
        miny = max(int(np.floor(min(p0[1], p1[1], p2[1]))), 0)
        maxy = min(int(np.ceil(max(p0[1], p1[1], p2[1]))), size - 1)
        if minx > maxx or miny > maxy:
            continue

        denom = (p1[1] - p2[1]) * (p0[0] - p2[0]) + (p2[0] - p1[0]) * (p0[1] - p2[1])
        if abs(denom) < 1e-12:
            continue

        gx, gy = np.meshgrid(
            np.arange(minx, maxx + 1) + 0.5, np.arange(miny, maxy + 1) + 0.5
        )
        a = ((p1[1] - p2[1]) * (gx - p2[0]) + (p2[0] - p1[0]) * (gy - p2[1])) / denom
        b = ((p2[1] - p0[1]) * (gx - p2[0]) + (p0[0] - p2[0]) * (gy - p2[1])) / denom
        c = 1.0 - a - b
        inside = (a >= 0) & (b >= 0) & (c >= 0)
        if not inside.any():
            continue

        wx = a * w0[0] + b * w1[0] + c * w2[0]
        wy = a * w0[1] + b * w1[1] + c * w2[1]

        for ref, hull, bb in zip(valid_refs, hull_arrays, hull_bboxes):
            sel = inside & (wx >= bb[0]) & (wx <= bb[1]) & (wy >= bb[2]) & (wy <= bb[3])
            if not sel.any():
                continue
            sel = sel & _point_in_hull(wx, wy, hull)
            if not sel.any():
                continue

            ground_normal = ref["ground_normal"]
            ground_max = ref["ground_max_z"]
            roof = ref["roof_z"]
            midpoint = ground_max + 0.5 * (roof - ground_max)

            for py, px in zip(*np.nonzero(sel)):
                hit = raycast_hit(wx[py, px], wy[py, px], top, bvh)
                if hit is None:
                    continue
                z, normal = hit

                cos_angle = max(-1.0, min(1.0, normal.dot(ground_normal)))
                angle_deg = degrees(acos(cos_angle))
                normal_component = 1.0 if angle_deg > NORMAL_ANGLE_THRESHOLD_DEG else 0.0

                if midpoint > ground_max:
                    height_component = max(
                        0.0, min(1.0, (z - ground_max) / (midpoint - ground_max))
                    )
                else:
                    height_component = 1.0 if z > ground_max else 0.0

                combined = min(1.0, normal_component + height_component)
                fy, fx = miny + py, minx + px
                air_mask[fy, fx] = max(air_mask[fy, fx], combined)
                ground_mask[fy, fx] = max(ground_mask[fy, fx], 1.0 - combined)

    if not air_mask.any() and not ground_mask.any():
        return None, None

    ground_img = _make_mask_image(f"{image_prefix}_car_ground", ground_mask)
    air_img = _make_mask_image(f"{image_prefix}_car_air", air_mask)
    return ground_img, air_img


def log(msg: str) -> None:
    print(msg, flush=True)


def process_item(item: dict) -> None:
    octtree_path = item["octtree_path"]
    glb_path = os.path.abspath(item["glb_path"])
    out_blend_path = os.path.abspath(item["out_blend_path"])
    latlon_bbox = item["latlon_bbox"]
    roads = item.get("roads", [])

    log(f"[{octtree_path}] clear_scene + import {glb_path}")
    clear_scene()
    bpy.ops.import_scene.gltf(filepath=glb_path)

    terrain_obj = next(
        (
            o
            for o in bpy.data.objects
            if o.type == "MESH" and o.data.uv_layers and o.data.materials
        ),
        None,
    )
    log(f"[{octtree_path}] terrain_obj={terrain_obj.name if terrain_obj else None}")

    terrain_bbox = measure_terrain_bbox()
    top = terrain_bbox["up"][1]

    log(f"[{octtree_path}] building terrain BVH")
    bvh = build_terrain_bvh(terrain_obj)
    log(f"[{octtree_path}] terrain BVH ready")

    log(f"[{octtree_path}] building {len(roads)} road(s)")
    roads_created = 0
    for road_entry in roads:
        feature = road_entry["feature"]
        geometry = feature.get("geometry", {})
        if geometry.get("type") != "LineString":
            continue

        coords = geometry.get("coordinates", [])
        if len(coords) < 2:
            continue

        props = feature.get("properties", {})
        feature_id = props.get("id", roads_created)

        xy_coords = []
        raw_heights = []
        for lon, lat in coords:
            x, y = latlon_to_xy(lon, lat, latlon_bbox, terrain_bbox)
            xy_coords.append((x, y))
            raw_heights.append(raycast_height(x, y, top, bvh))

        resolved = resolve_heights(raw_heights)
        if resolved is None:
            continue

        create_road_object(feature_id, xy_coords, resolved)
        roads_created += 1
    log(f"[{octtree_path}] roads created: {roads_created}")

    cars_created = 0
    cars_json_path = item.get("cars_json")
    if cars_json_path and os.path.isfile(cars_json_path):
        with open(cars_json_path, encoding="utf-8") as f:
            cars_data = json.load(f)

        # Build all collider meshes first, then link them. All ray casts (here and in
        # paint_car_ground_air_masks below) are object-scoped to terrain_obj, so linking
        # the colliders into the scene beforehand no longer risks self-occlusion.
        cars_list = cars_data.get("cars", [])
        log(f"[{octtree_path}] building {len(cars_list)} car collider(s)")
        collider_specs = []
        for i, car in enumerate(cars_list):
            corners = car.get("corners_latlon")
            center = car.get("center_latlon")
            if not corners or len(corners) != 4 or not center:
                continue
            log(f"[{octtree_path}] collider {i}/{len(cars_list)}: build_collider_mesh")
            collider_specs.append(
                build_collider_mesh(
                    corners, center, latlon_bbox, terrain_bbox, top, bvh
                )
            )
        log(f"[{octtree_path}] collider meshes built: {len(collider_specs)}")

        hulls = []
        for spec in collider_specs:
            create_car_object(cars_created, spec["verts"], spec["faces"])
            hulls.append(spec["hull"])
            cars_created += 1
        log(f"[{octtree_path}] car objects linked: {cars_created}")

        if hulls and terrain_obj is not None:
            log(f"[{octtree_path}] paint_car_mask")
            mask_img = paint_car_mask(
                terrain_obj, hulls, f"{octtree_path}_cars"
            )
            if mask_img is not None:
                attach_mask_material(
                    terrain_obj, mask_img, f"{terrain_obj.name}_cars_mat"
                )

            log(f"[{octtree_path}] paint_car_ground_air_masks")
            ground_img, air_img = paint_car_ground_air_masks(
                terrain_obj, collider_specs, octtree_path, bvh, top
            )
            log(f"[{octtree_path}] ground_air masks done")
            if ground_img is not None:
                attach_mask_material(
                    terrain_obj, ground_img, f"{terrain_obj.name}_car_ground"
                )
            if air_img is not None:
                attach_mask_material(
                    terrain_obj, air_img, f"{terrain_obj.name}_car_air"
                )

    log(f"[{octtree_path}] saving {out_blend_path}")
    os.makedirs(os.path.dirname(out_blend_path), exist_ok=True)
    bpy.ops.wm.save_as_mainfile(filepath=out_blend_path)


def main():
    try:
        args_idx = sys.argv.index("--")
        args = sys.argv[args_idx + 1 :]
    except ValueError:
        args = []

    if len(args) < 1:
        print("Usage: blender -b -P _blend_build_map.py -- <batch_json_path>")
        sys.exit(1)

    batch_path = args[0]
    with open(batch_path, "r", encoding="utf-8") as f:
        batch = json.load(f)

    items = batch.get("items", [])
    ok = 0
    failed = 0

    for item in items:
        tile = item.get("octtree_path", "?")
        try:
            process_item(item)
            ok += 1
            print(f"BUILD_OK {tile}")
        except Exception as e:
            failed += 1
            print(f"BUILD_FAIL {tile}: {e}")

    print(f"Batch complete: {ok} ok, {failed} failed (of {len(items)})")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"FATAL _blend_build_map batch error: {e}")
        sys.exit(1)
