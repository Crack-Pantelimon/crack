"""
Blender batch script: drape OSM road centerlines and car markers onto photogrammetry GLBs.

Invoked as:
    blender -b -P _blend_build_map.py -- <batch.json>
"""

import json
import os
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

APEX_OFFSET = 3.0
# The car collider hull is built at COLLIDER_SCALE (120%) of the car footprint. To cut
# the car bump out of the ground we clone that hull and shrink it to CUT_SCALE of its
# size, i.e. 0.92 * 1.2 = 1.104 (~110% of the original car), leaving a thin ring of
# ground uncut so the fill triangles blend into the surrounding terrain.
COLLIDER_SCALE = 1.2
CUT_SCALE = 0.92
GROUND_PALETTE_SIZE = 100


def clear_scene():
    """Wipe the current scene so the next tile imports into a clean slate."""
    bpy.ops.wm.read_factory_settings(use_empty=True)


def weld_terrain_mesh(obj: bpy.types.Object, dist: float = 1e-4) -> None:
    """
    Merge coincident vertices in a terrain mesh. Photogrammetry GLBs arrive as a
    "triangle soup": adjacent triangles share the exact same corner positions but use
    *separate* vertices, so the surface has no topological connectivity and every edge
    reads as a boundary. That makes hole/rim detection after a cut meaningless. Welding
    by a tiny distance stitches the soup into a connected surface (positions are already
    identical, so the shape is unchanged) while per-loop UVs are preserved, so the knife
    cut leaves a single clean boundary loop we can actually fill.
    """
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=dist)
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()


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


def build_terrain_bvh(terrain_objs: list[bpy.types.Object] | None) -> BVHTree | None:
    """
    Build a world-space BVH over all terrain meshes once, up front. All ray casts in
    this script query this single static tree instead of calling into Blender's
    per-object ray_cast (which, called tens of thousands of times, is both far slower
    and has been observed to crash Blender outright). Building once here also means
    road/car objects created later can never be hit by a ray, since they were never
    part of the tree to begin with.
    """
    if not terrain_objs:
        return None
    verts: list[Vector] = []
    tris: list[tuple[int, int, int]] = []
    offset = 0
    for terrain_obj in terrain_objs:
        mesh = terrain_obj.data
        mesh.calc_loop_triangles()
        n = len(mesh.vertices)
        co = np.empty(n * 3, dtype=np.float64)
        mesh.vertices.foreach_get("co", co)
        co = co.reshape(n, 3)
        mw = np.array(terrain_obj.matrix_world)
        world = (np.hstack([co, np.ones((n, 1))]) @ mw.T)[:, :3]
        verts.extend(Vector(v) for v in world)
        for lt in mesh.loop_triangles:
            tris.append(tuple(v + offset for v in lt.vertices))
        offset += n
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


def build_collider_mesh(
    corners_latlon: list[list[float]],
    center_latlon: list[float],
    latlon_bbox: dict,
    terrain_bbox: dict,
    top: float,
    bvh: BVHTree | None,
) -> dict:
    """
    Build a closed box collider molded onto the terrain, enlarged COLLIDER_SCALE about
    its centroid. Returns {verts, faces}; the hull is linked into the scene as a car
    marker, and its XY footprint (shrunk to CUT_SCALE) drives the knife that cuts the
    car bump out of the ground (see cut_car_from_terrain / _build_footprint_prism).
    """
    base_xy: list[tuple[float, float]] = []
    raw_z: list[float | None] = []
    for lat, lon in corners_latlon:
        x, y = latlon_to_xy(lon, lat, latlon_bbox, terrain_bbox)
        base_xy.append((x, y))
        raw_z.append(raycast_height(x, y, top, bvh))
    ground_z = resolve_corner_heights(raw_z, top)

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

    # Enlarge about the centroid.
    cog = Vector((0.0, 0.0, 0.0))
    for v in bm.verts:
        cog += v.co
    cog /= len(bm.verts)
    for v in bm.verts:
        v.co = cog + COLLIDER_SCALE * (v.co - cog)

    bm.verts.index_update()
    verts = [tuple(v.co) for v in bm.verts]
    faces = [[v.index for v in f.verts] for f in bm.faces]
    bm.free()
    return {"verts": verts, "faces": faces}


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


def _hsv_saturation(rgb: np.ndarray) -> np.ndarray:
    """Vectorized HSV saturation: s = (max - min) / max, 0 where max == 0."""
    mx = rgb.max(axis=-1)
    mn = rgb.min(axis=-1)
    with np.errstate(divide="ignore", invalid="ignore"):
        return np.where(mx > 0, (mx - mn) / mx, 0.0)


def _sample_ground_palette(rgb: np.ndarray) -> np.ndarray:
    """
    Pick up to GROUND_PALETTE_SIZE random ground-like colors (mid-luminance,
    low-saturation) from a flat (N,3) array of texture pixels. Filtering out the very
    dark (photogrammetry texture borders / shadows are pure black, whose saturation is
    0 and would otherwise dominate a naive lowest-saturation pick) and the very bright
    (lane markings) leaves asphalt/concrete tones. Falls back progressively so a palette
    is always returned when any pixels exist.
    """
    lum = rgb.mean(axis=1)
    sat = _hsv_saturation(rgb)
    for lo, hi, smax in ((0.12, 0.75, 0.30), (0.08, 0.85, 0.45), (0.0, 1.0, 1.0)):
        cand = rgb[(lum > lo) & (lum < hi) & (sat < smax)]
        if len(cand) >= 1:
            break
    if len(cand) == 0:
        return np.empty((0, 3), dtype=np.float32)
    n_keep = min(GROUND_PALETTE_SIZE, len(cand))
    idx = np.random.choice(len(cand), size=n_keep, replace=False)
    return cand[idx].astype(np.float32).copy()


def _find_base_color_image(
    mat: bpy.types.Material,
) -> tuple[bpy.types.ShaderNodeTexImage | None, bpy.types.Image | None]:
    """Return the first TEX_IMAGE node and its image in `mat`, or (None, None)."""
    if not mat or not mat.use_nodes or not mat.node_tree:
        return None, None
    for node in mat.node_tree.nodes:
        if node.type == "TEX_IMAGE" and node.image is not None:
            return node, node.image
    return None, None


def _make_color_image(
    name: str, rgba: np.ndarray, colorspace: str
) -> bpy.types.Image:
    h, w = rgba.shape[:2]
    existing = bpy.data.images.get(name)
    if existing is not None:
        bpy.data.images.remove(existing)
    img = bpy.data.images.new(name, width=w, height=h, alpha=True)
    img.colorspace_settings.name = colorspace
    img.pixels.foreach_set(rgba.reshape(-1))
    img.update()
    img.pack()
    return img


def build_fill_material(
    mesh: bpy.types.Object, index: int
) -> tuple[int, int] | None:
    """
    Build a dedicated material for the ground-fill triangles and append it to the mesh
    (the terrain's own faces keep their original material and, crucially, their original
    texture — untouched).

    The material's texture is a tiny 1×N *palette* strip: N ground-like colors sampled
    from the terrain (mid-luminance, low-saturation — asphalt/concrete, skipping the
    pure-black texture borders and bright lane markings). Each fill triangle is later
    pointed, via its UVs, at one random texel of this strip, so it renders as a flat
    random ground tone. Painting a separate strip instead of rasterizing into the shared
    terrain atlas is what keeps the cut from corrupting the rest of the ground: fill UVs
    that straddled atlas seams used to overwrite unrelated texels and flatten distant
    faces. Returns (palette_size, fill_slot) or None.
    """
    if not mesh.data.materials:
        return None
    orig_mat = mesh.data.materials[0]
    _, image = _find_base_color_image(orig_mat)
    if image is None:
        log(f"[{mesh.name}] no base-color image, skipping fill material")
        return None

    w, h = image.size
    flat = np.empty(w * h * 4, dtype=np.float32)
    image.pixels.foreach_get(flat)
    rgba = flat.reshape(h, w, 4)
    colors = _sample_ground_palette(rgba[..., :3].reshape(-1, 3))
    if colors.shape[0] == 0:
        log(f"[{mesh.name}] no ground palette colors, skipping fill material")
        return None

    n = colors.shape[0]
    palette = np.ones((1, n, 4), dtype=np.float32)
    palette[0, :, :3] = colors
    palette_img = _make_color_image(
        f"car_painted_with_ground_tex_{index}",
        palette,
        image.colorspace_settings.name,
    )

    fill_mat = orig_mat.copy()
    fill_mat.name = f"car_painted_with_ground_{index}"
    ptex, _ = _find_base_color_image(fill_mat)
    if ptex is not None:
        ptex.image = palette_img
        ptex.interpolation = "Closest"  # sample one flat texel, no cross-color blend
        ptex.extension = "EXTEND"

    mesh.data.materials.append(fill_mat)
    fill_slot = len(mesh.data.materials) - 1
    return n, fill_slot


def _palette_uv(n: int) -> tuple[float, float]:
    """A UV pointing at the centre of one random texel of the 1×N palette strip."""
    j = np.random.randint(0, n)
    return ((j + 0.5) / n, 0.5)


def _convex_hull_2d(points: np.ndarray) -> list[tuple[float, float]]:
    """Andrew's monotone chain; returns a CCW hull (>=3 unique points) or [] otherwise."""
    pts = sorted(set((round(float(x), 5), round(float(y), 5)) for x, y in points))
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


def _point_in_convex(x: float, y: float, poly: list[tuple[float, float]]) -> bool:
    """Point-in-CCW-convex-polygon test (edges treated as inside)."""
    m = len(poly)
    for i in range(m):
        x1, y1 = poly[i]
        x2, y2 = poly[(i + 1) % m]
        if (x2 - x1) * (y - y1) - (y2 - y1) * (x - x1) < 0.0:
            return False
    return True


def _separate_boundary_loops(edges: list) -> list[tuple[list, bool]]:
    """Chain a set of BMEdges into (vertex_chain, closed) loops/open paths."""
    adj: dict = {}
    for e in edges:
        a, b = e.verts[0], e.verts[1]
        adj.setdefault(a, []).append((b, e))
        adj.setdefault(b, []).append((a, e))

    used: set = set()
    remaining = set(edges)
    chains: list[tuple[list, bool]] = []

    def unused(v):
        return [(o, e) for (o, e) in adj[v] if e not in used]

    while remaining:
        start = next((v for v in adj if len(unused(v)) == 1), None)
        if start is None:
            start = next(iter(remaining)).verts[0]
        chain = [start]
        cur = start
        closed = False
        while True:
            inc = unused(cur)
            if not inc:
                break
            o, e = inc[0]
            used.add(e)
            remaining.discard(e)
            if o is start:
                closed = True
                break
            chain.append(o)
            cur = o
        chains.append((chain, closed))
    return chains


def _fan_fill_loops(bm, rim: list) -> list:
    """Fallback fill: fan each boundary loop from a fresh centroid vertex."""
    faces: list = []
    for chain, closed in _separate_boundary_loops(rim):
        if len(chain) < 3:
            continue
        center_co = Vector((0.0, 0.0, 0.0))
        for v in chain:
            center_co += v.co
        center_co /= len(chain)
        cvert = bm.verts.new(center_co)
        if closed:
            pairs = [(chain[i], chain[(i + 1) % len(chain)]) for i in range(len(chain))]
        else:
            pairs = [(chain[i], chain[i + 1]) for i in range(len(chain) - 1)]
        for a, b in pairs:
            try:
                faces.append(bm.faces.new((a, b, cvert)))
            except ValueError:
                continue
    return faces


def _fill_loops(bm, rim: list) -> list:
    """
    Triangulate the region(s) bounded by the rim edges. Prefer bmesh.ops.triangle_fill
    (even, well-shaped triangles from the boundary verts only); if it fills nothing
    (e.g. a badly non-planar loop it refuses), fall back to a centroid fan so the hole
    is always closed.
    """
    try:
        res = bmesh.ops.triangle_fill(
            bm, use_beauty=True, use_dissolve=False, edges=list(rim)
        )
    except RuntimeError:
        res = None
    faces = (
        [g for g in res["geom"] if isinstance(g, bmesh.types.BMFace)] if res else []
    )
    if not faces:
        faces = _fan_fill_loops(bm, rim)
    return faces


def _build_footprint_prism(
    car_obj: bpy.types.Object,
    z0: float,
    z1: float,
    mark_mat: bpy.types.Material,
) -> tuple[bpy.types.Object | None, list[tuple[float, float]]]:
    """
    Build the cutter for one car: take the car collider's XY footprint, shrink it to
    CUT_SCALE about its centroid, and extrude it into a vertical prism spanning
    [z0, z1] (the full terrain z-range plus a margin). A full-height prism — rather
    than the finite-height collider box — is what makes the knife cut a clean, closed
    ring around the footprint regardless of how the terrain undulates through it.
    Returns (cutter_object, footprint_polygon) or (None, []) if the footprint is
    degenerate. Verts are world-space (car colliders have identity matrix_world).
    """
    n = len(car_obj.data.vertices)
    co = np.empty(n * 3, dtype=np.float64)
    car_obj.data.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    cog_xy = co[:, :2].mean(axis=0)
    shrunk_xy = cog_xy + CUT_SCALE * (co[:, :2] - cog_xy)
    poly = _convex_hull_2d(shrunk_xy)
    if len(poly) < 3:
        return None, []

    m = len(poly)
    verts = [(x, y, z0) for x, y in poly] + [(x, y, z1) for x, y in poly]
    faces = [[i, (i + 1) % m, (i + 1) % m + m, i + m] for i in range(m)]

    mesh = bpy.data.meshes.new(f"{car_obj.name}_cutter_mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    mesh.materials.append(mark_mat)
    for poly_f in mesh.polygons:
        poly_f.material_index = 0

    clone = bpy.data.objects.new(f"{car_obj.name}_cutter", mesh)
    bpy.context.scene.collection.objects.link(clone)
    return clone, poly


def cut_car_from_terrain(
    mesh_obj: bpy.types.Object,
    car_obj: bpy.types.Object,
    z_range: tuple[float, float],
    mark_mat: bpy.types.Material,
    n_colors: int,
    fill_slot: int,
) -> tuple[int, int, list]:
    """
    Cut one car's footprint volume out of the terrain and rebuild flat ground:
      1. Knife the terrain with a vertical prism built from the shrunk car footprint
         (mesh.intersect), tagging the prism faces with mark_mat.
      2. Delete the prism faces and every terrain face whose centroid falls inside the
         footprint — leaving a clean, knife-cut hole.
      3. Triangulate each boundary loop of the hole, assign the fill material, and point
         each new triangle's UVs at a random texel of the ground palette strip.
    Returns (rim_edges, fill_faces, footprint_poly).
    """
    z0, z1 = z_range
    clone, poly = _build_footprint_prism(car_obj, z0, z1, mark_mat)
    if clone is None:
        return 0, 0, []

    # Knife the prism into the terrain: join, then intersect the (selected) prism faces
    # against the (unselected) terrain faces.
    bpy.ops.object.select_all(action="DESELECT")
    clone.select_set(True)
    mesh_obj.select_set(True)
    bpy.context.view_layer.objects.active = mesh_obj
    bpy.ops.object.join()

    cutter_slot = next(
        (j for j, m in enumerate(mesh_obj.data.materials) if m is mark_mat), None
    )
    if cutter_slot is None:
        return 0, 0, []

    bpy.ops.object.mode_set(mode="EDIT")
    be = bmesh.from_edit_mesh(mesh_obj.data)
    for f in be.faces:
        f.select_set(f.material_index == cutter_slot)
    bmesh.update_edit_mesh(mesh_obj.data)
    try:
        bpy.ops.mesh.intersect(
            mode="SELECT_UNSELECT", separate_mode="NONE", solver="EXACT"
        )
    except RuntimeError as e:
        log(f"[{mesh_obj.name}] knife failed: {e}")
    bpy.ops.object.mode_set(mode="OBJECT")

    rim, fill = _rebuild_hole(mesh_obj, cutter_slot, poly, n_colors, fill_slot)
    return rim, fill, poly


def _rebuild_hole(
    mesh_obj: bpy.types.Object,
    cutter_slot: int,
    poly: list[tuple[float, float]],
    n_colors: int,
    fill_slot: int,
) -> tuple[int, int]:
    """Delete cutter + inside-footprint faces, triangulate the hole, palette-map the fill."""
    bm = bmesh.new()
    bm.from_mesh(mesh_obj.data)
    bm.faces.ensure_lookup_table()
    uv_lay = bm.loops.layers.uv.active
    if uv_lay is None and bm.loops.layers.uv:
        uv_lay = bm.loops.layers.uv[0]
    mw = mesh_obj.matrix_world

    del_set = set()
    for f in bm.faces:
        if f.material_index == cutter_slot:
            del_set.add(f)
            continue
        c = mw @ f.calc_center_median()
        if _point_in_convex(c.x, c.y, poly):
            del_set.add(f)

    # The hole rim: edges that keep exactly one neighbour after the delete (their other
    # neighbour(s) are removed). Collected before the delete so the loops stay intact.
    rim = []
    for e in bm.edges:
        lf = e.link_faces
        n_del = sum(1 for f in lf if f in del_set)
        if n_del >= 1 and (len(lf) - n_del) == 1:
            rim.append(e)

    bmesh.ops.delete(bm, geom=list(del_set), context="FACES")
    rim = [e for e in rim if e.is_valid]
    if not rim:
        bm.normal_update()
        bm.to_mesh(mesh_obj.data)
        bm.free()
        mesh_obj.data.update()
        return 0, 0

    # Fill the boundary loop(s) with a proper triangulation instead of a centroid fan.
    # triangle_fill spans the loop with well-shaped triangles built only from the rim
    # verts (it adds no interior vertices), so (a) it fills the entire loop — no leftover
    # black gaps — and (b) the triangles are evenly sized by the rim spacing rather than
    # a pinwheel radiating from a single centre vertex.
    fill_faces = _fill_loops(bm, rim)
    _assign_fill(fill_faces, fill_slot, n_colors, uv_lay, mw)

    bm.to_mesh(mesh_obj.data)
    bm.free()
    mesh_obj.data.update()
    return len(rim), len(fill_faces)


def _assign_fill(fill_faces, fill_slot, n_colors, uv_lay, mw) -> None:
    """Give each fill triangle the fill material, a random palette UV, and an up normal."""
    for f in fill_faces:
        f.material_index = fill_slot
        if uv_lay is not None:
            uv = _palette_uv(n_colors)
            for loop in f.loops:
                loop[uv_lay].uv = uv
    if fill_faces:
        for f in fill_faces:
            f.normal_update()
        # Ground fill faces should face up, not be back-facing.
        for f in fill_faces:
            if (mw.to_3x3() @ f.normal).z < 0.0:
                f.normal_flip()


def _edge_between(a, b):
    for e in a.link_edges:
        if e.other_vert(a) is b:
            return e
    return None


def _close_footprint_holes(
    mesh_obj: bpy.types.Object,
    polys: list,
    n_colors: int,
    fill_slot: int,
) -> int:
    """
    Final cleanup for one terrain mesh: fill any *small* boundary loop that lies wholly
    inside a car footprint. These are thin no-data slivers the per-car cut can leave at
    the reconstructed car/ground border (the car occluded the ground, so photogrammetry
    has gaps there). Loops that extend outside every footprint — the real pre-existing
    photogrammetry holes and the tile's outer edge — are left untouched.
    """
    if not polys:
        return 0
    bm = bmesh.new()
    bm.from_mesh(mesh_obj.data)
    bm.edges.ensure_lookup_table()
    uv_lay = bm.loops.layers.uv.active
    if uv_lay is None and bm.loops.layers.uv:
        uv_lay = bm.loops.layers.uv[0]
    mw = mesh_obj.matrix_world

    def inside_any(v) -> bool:
        w = mw @ v.co
        return any(_point_in_convex(w.x, w.y, p) for p in polys)

    boundary = [e for e in bm.edges if len(e.link_faces) == 1]
    fill_edges: list = []
    for chain, closed in _separate_boundary_loops(boundary):
        if not closed or len(chain) < 3:
            continue
        if not all(inside_any(v) for v in chain):
            continue
        for i in range(len(chain)):
            e = _edge_between(chain[i], chain[(i + 1) % len(chain)])
            if e is not None:
                fill_edges.append(e)
    if not fill_edges:
        bm.free()
        return 0

    fill_faces = _fill_loops(bm, fill_edges)
    _assign_fill(fill_faces, fill_slot, n_colors, uv_lay, mw)
    bm.to_mesh(mesh_obj.data)
    bm.free()
    mesh_obj.data.update()
    return len(fill_faces)


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

    terrain_objs = [
        o
        for o in bpy.data.objects
        if o.type == "MESH" and o.data.uv_layers and o.data.materials
    ]
    log(f"[{octtree_path}] terrain meshes: {len(terrain_objs)}")

    for mesh_obj in terrain_objs:
        weld_terrain_mesh(mesh_obj)
    log(f"[{octtree_path}] terrain welded")

    terrain_bbox = measure_terrain_bbox()
    top = terrain_bbox["up"][1]

    log(f"[{octtree_path}] building terrain BVH")
    bvh = build_terrain_bvh(terrain_objs)
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

    collider_specs: list[dict] = []
    cars_json_path = item.get("cars_json")
    if cars_json_path and os.path.isfile(cars_json_path):
        with open(cars_json_path, encoding="utf-8") as f:
            cars_data = json.load(f)

        cars_list = cars_data.get("cars", [])
        log(f"[{octtree_path}] building {len(cars_list)} car collider(s)")
        for i, car in enumerate(cars_list):
            corners = car.get("corners_latlon")
            center = car.get("center_latlon")
            if not corners or len(corners) != 4 or not center:
                continue
            collider_specs.append(
                build_collider_mesh(
                    corners, center, latlon_bbox, terrain_bbox, top, bvh
                )
            )
        log(f"[{octtree_path}] collider meshes built: {len(collider_specs)}")

    car_objs: list[bpy.types.Object] = []
    for i, spec in enumerate(collider_specs):
        car_objs.append(create_car_object(i, spec["verts"], spec["faces"]))
    log(f"[{octtree_path}] car objects linked: {len(car_objs)}")

    # Full terrain z-span (plus margin) that every cutter prism is extruded through.
    z_range = (terrain_bbox["up"][0] - 10.0, terrain_bbox["up"][1] + 10.0)
    # One shared marker material tags cutter geometry so it can be knifed then deleted.
    mark_mat = bpy.data.materials.new("CUTTER_MARK")

    for i, mesh_obj in enumerate(terrain_objs):
        log(f"[{octtree_path}] mesh {i} ({mesh_obj.name}): build_fill_material")
        fill = build_fill_material(mesh_obj, i)
        if fill is None:
            continue
        n_colors, fill_slot = fill

        if not car_objs:
            continue

        log(f"[{octtree_path}] mesh {i}: cutting {len(car_objs)} car(s)")
        total_rim = 0
        total_fill = 0
        footprints: list = []
        for car_obj in car_objs:
            rim, filled, poly = cut_car_from_terrain(
                mesh_obj,
                car_obj,
                z_range,
                mark_mat,
                n_colors,
                fill_slot,
            )
            total_rim += rim
            total_fill += filled
            if poly:
                footprints.append(poly)
        cleanup = _close_footprint_holes(
            mesh_obj, footprints, n_colors, fill_slot
        )
        log(
            f"[{octtree_path}] mesh {i}: cut done, {total_rim} rim edges, "
            f"{total_fill} fill faces, {cleanup} cleanup faces"
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
