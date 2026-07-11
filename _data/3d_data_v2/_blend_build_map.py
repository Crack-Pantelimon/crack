"""
Blender batch script: drape OSM road centerlines and car markers onto photogrammetry GLBs.

Invoked as:
    blender -b -P _blend_build_map.py -- <batch.json>
"""

import json
import os
import sys

import bpy
from mathutils import Vector

APEX_OFFSET = 3.0


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


def raycast_height(x: float, y: float, top: float, depsgraph) -> float | None:
    """Cast downward from above the terrain bbox; return hit z or None."""
    origin = Vector((x, y, top + 1.0))
    direction = Vector((0.0, 0.0, -1.0))
    hit, loc, *_rest = bpy.context.scene.ray_cast(
        depsgraph, origin, direction
    )
    if hit:
        return loc.z
    return None


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


def create_car_pyramid(
    car_index: int,
    corners_latlon: list[list[float]],
    center_latlon: list[float],
    latlon_bbox: dict,
    terrain_bbox: dict,
    top: float,
    depsgraph,
) -> bpy.types.Object | None:
    """Build an apex-raised pyramid from four corner lat/lon points and a center."""
    base_z: list[float] = []
    base_xy: list[tuple[float, float]] = []

    for lat, lon in corners_latlon:
        x, y = latlon_to_xy(lon, lat, latlon_bbox, terrain_bbox)
        z = raycast_height(x, y, top, depsgraph)
        if z is None:
            z = top
        base_xy.append((x, y))
        base_z.append(z)

    cx, cy = latlon_to_xy(center_latlon[1], center_latlon[0], latlon_bbox, terrain_bbox)
    center_z = raycast_height(cx, cy, top, depsgraph)
    if center_z is None:
        center_z = top
    apex_z = center_z + APEX_OFFSET

    verts = [
        (base_xy[0][0], base_xy[0][1], base_z[0]),
        (base_xy[1][0], base_xy[1][1], base_z[1]),
        (base_xy[2][0], base_xy[2][1], base_z[2]),
        (base_xy[3][0], base_xy[3][1], base_z[3]),
        (cx, cy, apex_z),
    ]
    faces = [
        (0, 1, 2, 3),
        (0, 4, 1),
        (1, 4, 2),
        (2, 4, 3),
        (3, 4, 0),
    ]

    mesh = bpy.data.meshes.new(name=f"car_{car_index}_mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()

    obj = bpy.data.objects.new(name=f"car_{car_index}", object_data=mesh)
    cars_coll = get_or_create_collection("cars")
    cars_coll.objects.link(obj)
    return obj


def process_item(item: dict) -> None:
    octtree_path = item["octtree_path"]
    glb_path = os.path.abspath(item["glb_path"])
    out_blend_path = os.path.abspath(item["out_blend_path"])
    latlon_bbox = item["latlon_bbox"]
    roads = item.get("roads", [])

    clear_scene()
    bpy.ops.import_scene.gltf(filepath=glb_path)

    terrain_bbox = measure_terrain_bbox()
    top = terrain_bbox["up"][1]
    depsgraph = bpy.context.evaluated_depsgraph_get()

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
            raw_heights.append(raycast_height(x, y, top, depsgraph))

        resolved = resolve_heights(raw_heights)
        if resolved is None:
            continue

        create_road_object(feature_id, xy_coords, resolved)
        roads_created += 1

    cars_created = 0
    cars_json_path = item.get("cars_json")
    if cars_json_path and os.path.isfile(cars_json_path):
        with open(cars_json_path, encoding="utf-8") as f:
            cars_data = json.load(f)
        for car in cars_data.get("cars", []):
            corners = car.get("corners_latlon")
            center = car.get("center_latlon")
            if not corners or len(corners) != 4 or not center:
                continue
            create_car_pyramid(
                cars_created,
                corners,
                center,
                latlon_bbox,
                terrain_bbox,
                top,
                depsgraph,
            )
            cars_created += 1

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
