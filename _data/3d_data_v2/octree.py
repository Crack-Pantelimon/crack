"""
Octree coordinate helpers for Google Earth tile system.

The Earth octree uses a 2-character root (covering hemispheres/quadrants)
followed by digits 0-7. At each level beyond the root, the lat/lon box
is halved on both axes (giving 4 sub-cells), with an additional z-split
(giving 8 sub-cells total, but only the lat/lon part matters for surface tiles).
"""

import math
from dataclasses import dataclass


@dataclass
class BBox:
    """Geographic bounding box in degrees."""
    north: float
    south: float
    west: float
    east: float

    @property
    def lat_span(self) -> float:
        return abs(self.north - self.south)

    @property
    def lon_span(self) -> float:
        return abs(self.east - self.west)

    @property
    def center_lat(self) -> float:
        return (self.north + self.south) / 2.0

    @property
    def center_lon(self) -> float:
        return (self.east + self.west) / 2.0


def parse_bbox(filepath: str) -> BBox:
    """
    Parse bounding box from file.
    File format: two lines, each with lat,lon.
    First line = NW corner, second line = SE corner.
    """
    with open(filepath, "r") as f:
        lines = [line.strip() for line in f.readlines() if line.strip()]
    assert len(lines) == 2, f"Expected 2 lines in bbox file, got {len(lines)}"

    lat1, lon1 = map(float, lines[0].split(","))
    lat2, lon2 = map(float, lines[1].split(","))

    return BBox(
        north=max(lat1, lat2),
        south=min(lat1, lat2),
        west=min(lon1, lon2),
        east=max(lon1, lon2),
    )


def get_first_octant(lat: float, lon: float) -> tuple[str, BBox]:
    """
    Get the first 2-character octant path and bounding box for a lat/lon.
    The Earth is split into 8 root quadrants.
    """
    if lat < 0:
        if lon < -90:
            return ("02", BBox(north=0, south=-90, west=-180, east=-90))
        if lon < 0:
            return ("03", BBox(north=0, south=-90, west=-90, east=0))
        if lon < 90:
            return ("12", BBox(north=0, south=-90, west=0, east=90))
        return ("13", BBox(north=0, south=-90, west=90, east=180))
    else:
        if lon < -90:
            return ("20", BBox(north=90, south=0, west=-180, east=-90))
        if lon < 0:
            return ("21", BBox(north=90, south=0, west=-90, east=0))
        if lon < 90:
            return ("30", BBox(north=90, south=0, west=0, east=90))
        return ("31", BBox(north=90, south=0, west=90, east=180))


def get_next_octant(box: BBox, lat: float, lon: float) -> tuple[int, BBox]:
    """
    Given a bounding box, determines which sub-octant (0-3) a lat/lon falls in.
    Returns (digit, new_box).

    Bit layout:
      - bit 1 (value 2): lat >= midpoint (north half)
      - bit 0 (value 1): lon >= midpoint (east half)
    """
    n, s, w, e = box.north, box.south, box.west, box.east
    mid_lat = (n + s) / 2.0
    mid_lon = (w + e) / 2.0

    key = 0

    if lat < mid_lat:
        # south half, y = 0
        n = mid_lat
    else:
        # north half, y = 1
        s = mid_lat
        key += 2

    # At poles, skip lon subdivision
    if n == 90 or s == -90:
        pass
    elif lon < mid_lon:
        # west half, x = 0
        e = mid_lon
    else:
        # east half, x = 1
        w = mid_lon
        key += 1

    return key, BBox(north=n, south=s, west=w, east=e)


def lat_lon_to_octant(lat: float, lon: float, level: int) -> str:
    """
    Convert a lat/lon to an octant path at the given level.
    Level is the total path length (including the 2-character root).
    """
    path, box = get_first_octant(lat, lon)
    for _ in range(level - 2):
        key, box = get_next_octant(box, lat, lon)
        path += str(key)
    return path


def octant_path_to_bbox(path: str) -> BBox:
    """
    Given an octant path, compute its lat/lon bounding box.
    """
    # Determine root quadrant
    first_two = path[:2]
    root_map = {
        "02": BBox(north=0, south=-90, west=-180, east=-90),
        "03": BBox(north=0, south=-90, west=-90, east=0),
        "12": BBox(north=0, south=-90, west=0, east=90),
        "13": BBox(north=0, south=-90, west=90, east=180),
        "20": BBox(north=90, south=0, west=-180, east=-90),
        "21": BBox(north=90, south=0, west=-90, east=0),
        "30": BBox(north=90, south=0, west=0, east=90),
        "31": BBox(north=90, south=0, west=90, east=180),
    }
    box = root_map[first_two]

    for ch in path[2:]:
        digit = int(ch)
        # Only use bits 0-1 for lat/lon (bit 2 is z-axis)
        lat_bit = (digit >> 1) & 1  # bit 1
        lon_bit = digit & 1  # bit 0

        mid_lat = (box.north + box.south) / 2.0
        mid_lon = (box.west + box.east) / 2.0

        if lat_bit == 0:
            # south half
            box = BBox(north=mid_lat, south=box.south, west=box.west, east=box.east)
        else:
            # north half
            box = BBox(north=box.north, south=mid_lat, west=box.west, east=box.east)

        # Skip lon for poles
        if box.north == 90 or box.south == -90:
            continue

        if lon_bit == 0:
            # west half
            box = BBox(north=box.north, south=box.south, west=box.west, east=mid_lon)
        else:
            # east half
            box = BBox(north=box.north, south=box.south, west=mid_lon, east=box.east)

    return box


def compute_best_level(bbox: BBox, target_grid: int = 8, max_level: int = 24) -> int:
    """
    Compute the octree level that gives approximately target_grid x target_grid tiles
    covering the bounding box.

    At level L (path length L), each tile covers:
      lat_span = 90 / 2^(L-2) degrees
      lon_span = 90 / 2^(L-2) degrees
    (The first 2 chars each halve 180° into 90° quadrants)

    `max_level` caps the result. Google Earth's photogrammetry octree typically
    bottoms out around level 20-22 (the tree traversal naturally stops at leaf
    nodes), so the cap mostly guards against pathologically tiny bounding boxes.
    """
    lat_span = bbox.lat_span
    lon_span = bbox.lon_span

    # We want: bbox_span / tile_span ≈ target_grid
    # tile_span = 90 / 2^(L-2)
    # So: 2^(L-2) ≈ target_grid * 90 / bbox_span
    # L ≈ log2(target_grid * 90 / bbox_span) + 2

    lat_level = math.log2(target_grid * 90.0 / lat_span) + 2
    lon_level = math.log2(target_grid * 90.0 / lon_span) + 2

    # Use the average and round to nearest integer
    level = round((lat_level + lon_level) / 2.0)

    # Clamp to reasonable range
    level = max(4, min(level, max_level))

    return level


def enumerate_octants_in_bbox(bbox: BBox, level: int) -> list[str]:
    """
    Enumerate all octant paths at the given level that overlap with the bbox.
    We generate a grid of sample points and find unique octant paths.
    """
    # Calculate tile size at this level
    tile_span_lat = 90.0 / (2 ** (level - 2))
    tile_span_lon = 90.0 / (2 ** (level - 2))

    # How many tiles we need in each direction
    n_lat = math.ceil(bbox.lat_span / tile_span_lat) + 1
    n_lon = math.ceil(bbox.lon_span / tile_span_lon) + 1

    # Generate sample points at half-tile-span intervals starting from the bbox corners
    paths = set()
    for i in range(n_lat + 1):
        for j in range(n_lon + 1):
            lat = bbox.south + i * tile_span_lat * 0.5
            lon = bbox.west + j * tile_span_lon * 0.5

            # Clamp to bbox
            lat = max(bbox.south, min(bbox.north, lat))
            lon = max(bbox.west, min(bbox.east, lon))

            # Clamp to valid range
            lat = max(-89.999, min(89.999, lat))
            lon = max(-179.999, min(179.999, lon))

            path = lat_lon_to_octant(lat, lon, level)
            paths.add(path)

    return sorted(paths)


def tile_grid_dimensions(bbox: BBox, level: int) -> tuple[int, int]:
    """Return approximate (rows, cols) of the tile grid at the given level."""
    tile_span_lat = 90.0 / (2 ** (level - 2))
    tile_span_lon = 90.0 / (2 ** (level - 2))
    rows = math.ceil(bbox.lat_span / tile_span_lat)
    cols = math.ceil(bbox.lon_span / tile_span_lon)
    return rows, cols
