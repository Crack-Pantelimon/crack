"""
Coordinate conversion utilities for the street obstacle detection pipeline.

Provides conversions between:
- WGS84 lat/lon (degrees)
- Local ENU (East, North, Up) in meters
- Bevy/GLTF coordinate space (Y-up)
- Blender coordinate space (Z-up)

Reference point: Cora Pantelimon (Lat: 44.445522, Lon: 26.142436)
"""

import math
from typing import Sequence

# Reference point for local ENU coordinate system (Cora Pantelimon)
REF_LAT = 44.445522
REF_LON = 26.142436

# Approximate meters per degree of latitude
METERS_PER_DEGREE = 111319.9


def latlon_to_enu(
    lat: float, lon: float, ref_lat: float = REF_LAT, ref_lon: float = REF_LON
) -> tuple[float, float]:
    """
    Convert WGS84 lat/lon to local ENU (East, North) coordinates in meters.

    Args:
        lat: Latitude in degrees
        lon: Longitude in degrees
        ref_lat: Reference latitude (origin) in degrees
        ref_lon: Reference longitude (origin) in degrees

    Returns:
        (east_m, north_m) tuple in meters relative to the reference point
    """
    east = (lon - ref_lon) * math.cos(math.radians(ref_lat)) * METERS_PER_DEGREE
    north = (lat - ref_lat) * METERS_PER_DEGREE
    return (east, north)


def enu_to_latlon(
    east_m: float, north_m: float, ref_lat: float = REF_LAT, ref_lon: float = REF_LON
) -> tuple[float, float]:
    """
    Convert local ENU (East, North) coordinates in meters back to WGS84 lat/lon.

    Args:
        east_m: East coordinate in meters
        north_m: North coordinate in meters
        ref_lat: Reference latitude (origin) in degrees
        ref_lon: Reference longitude (origin) in degrees

    Returns:
        (lat, lon) tuple in degrees
    """
    lat = ref_lat + north_m / METERS_PER_DEGREE
    lon = ref_lon + east_m / (math.cos(math.radians(ref_lat)) * METERS_PER_DEGREE)
    return (lat, lon)


def enu_to_bevy(east: float, north: float, up: float) -> tuple[float, float, float]:
    """
    Convert ENU coordinates to Bevy/GLTF coordinate space (Y-up).

    Bevy mapping: X=East, Y=Up, Z=-North

    Args:
        east: East coordinate in meters
        north: North coordinate in meters
        up: Up coordinate in meters

    Returns:
        (bevy_x, bevy_y, bevy_z) tuple
    """
    return (east, up, -north)


def bevy_to_enu(
    bevy_x: float, bevy_y: float, bevy_z: float
) -> tuple[float, float, float]:
    """
    Convert Bevy/GLTF coordinates (Y-up) to ENU coordinates.

    Args:
        bevy_x: Bevy X coordinate (East)
        bevy_y: Bevy Y coordinate (Up)
        bevy_z: Bevy Z coordinate (-North)

    Returns:
        (east, north, up) tuple in meters
    """
    return (bevy_x, -bevy_z, bevy_y)


def manifest_xyz_to_enu_bbox(
    x_min: float,
    y_min: float,
    z_min: float,
    x_max: float,
    y_max: float,
    z_max: float,
) -> dict[str, float]:
    """
    Convert manifest xyz bounding box (Bevy/GLTF space) to ENU bbox.

    The manifest's xyz values are in GLTF/Bevy coordinate space:
      - x = East
      - y = Up (height)
      - z = -North

    So:
      - east_range = [x_min, x_max]
      - north_range = [-z_max, -z_min] (z is negated north)
      - up_range = [y_min, y_max]

    Args:
        x_min..z_max: Bounding box from the manifest parquet

    Returns:
        Dict with keys: east_min, east_max, north_min, north_max, up_min, up_max
    """
    return {
        "east_min": x_min,
        "east_max": x_max,
        "north_min": -z_max,  # z is -North, so -z_max is minimum north
        "north_max": -z_min,  # -z_min is maximum north
        "up_min": y_min,
        "up_max": y_max,
    }


def latlon_coords_to_enu(
    coords: Sequence[Sequence[float]],
    ref_lat: float = REF_LAT,
    ref_lon: float = REF_LON,
) -> list[tuple[float, float]]:
    """
    Convert a list of [lon, lat] coordinate pairs (GeoJSON order) to ENU.

    Args:
        coords: List of [lon, lat] pairs (GeoJSON convention)
        ref_lat: Reference latitude
        ref_lon: Reference longitude

    Returns:
        List of (east, north) tuples in meters
    """
    return [latlon_to_enu(lat=c[1], lon=c[0], ref_lat=ref_lat, ref_lon=ref_lon) for c in coords]


def blender_to_enu(
    bx: float, by: float, bz: float
) -> tuple[float, float, float]:
    """
    Convert Blender coordinates (Z-up) to ENU.

    Blender mapping: X=East, Y=North, Z=Up

    Returns:
        (east, north, up) tuple
    """
    return (bx, by, bz)


def enu_to_blender(
    east: float, north: float, up: float
) -> tuple[float, float, float]:
    """
    Convert ENU to Blender coordinates (Z-up).

    Blender mapping: X=East, Y=North, Z=Up

    Returns:
        (blender_x, blender_y, blender_z) tuple
    """
    return (east, north, up)


def pixel_to_latlon(
    px: float,
    py: float,
    width: int,
    height: int,
    lat_lon_bbox: dict[str, float],
) -> tuple[float, float]:
    """
    Map a pixel coordinate to WGS84 lat/lon using the tile geographic bbox.

    Image origin is top-left; north is up and west is left.
    """
    u = px / width
    v = py / height
    lon = lat_lon_bbox["lon_west"] + u * (lat_lon_bbox["lon_east"] - lat_lon_bbox["lon_west"])
    lat = lat_lon_bbox["lat_north"] - v * (lat_lon_bbox["lat_north"] - lat_lon_bbox["lat_south"])
    return lat, lon


def pixel_to_blender_xyz(
    px: float,
    py: float,
    width: int,
    height: int,
    bbox_xyz: dict[str, list[float]],
    *,
    z: float | None = None,
) -> tuple[float, float, float]:
    """
    Map a pixel coordinate to Blender XYZ using the mesh bounding box.

    Uses a simple rule-of-three linear interpolation across the top face.
    """
    u = px / width
    v = py / height
    x_min, y_min, z_min = bbox_xyz["min"]
    x_max, y_max, z_max = bbox_xyz["max"]
    x = x_min + u * (x_max - x_min)
    y = y_max - v * (y_max - y_min)
    z_val = z_max if z is None else z
    return x, y, z_val


def bbox_center_latlon(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    width: int,
    height: int,
    lat_lon_bbox: dict[str, float],
) -> tuple[float, float]:
    cx = (x1 + x2) / 2.0
    cy = (y1 + y2) / 2.0
    return pixel_to_latlon(cx, cy, width, height, lat_lon_bbox)


def bbox_center_blender_xyz(
    x1: float,
    y1: float,
    x2: float,
    y2: float,
    width: int,
    height: int,
    bbox_xyz: dict[str, list[float]],
) -> tuple[float, float, float]:
    cx = (x1 + x2) / 2.0
    cy = (y1 + y2) / 2.0
    return pixel_to_blender_xyz(cx, cy, width, height, bbox_xyz)
