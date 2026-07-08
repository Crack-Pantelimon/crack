"""
Shared helpers for selecting depth-20 road tiles (Șoseaua Pantelimon).

Used by download, render, and YOLO stages of the street cleanup pipeline.
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import shapely.wkb
from shapely.geometry import box as shapely_box

from octree import BBox, octant_path_to_bbox
from earth_client import fetch_planetoid_metadata, find_tiles_in_bbox_levels
from street_cleanup.coord_utils import latlon_to_enu

ROAD_POLYGON_WKB = Path("street_cleanup/road_polygons_enu.wkb")
OUTPUT_DIR = Path("data_out")

# Șoseaua Pantelimon bounds in lat/lon
PANTELIMON_LAT_RANGE = (44.4424473, 44.4488994)
PANTELIMON_LON_RANGE = (26.1308401, 26.1656894)
TARGET_DEPTH = 20


@dataclass(frozen=True)
class RoadTile:
    octant_path: str
    glb_path: Path
    lat_south: float
    lat_north: float
    lon_west: float
    lon_east: float

    @property
    def render_dir(self) -> Path:
        return Path("street_cleanup/renders") / self.octant_path[-3:]

    @property
    def render_jpg(self) -> Path:
        return self.render_dir / f"{self.octant_path}.jpg"

    @property
    def render_meta(self) -> Path:
        return self.render_dir / f"{self.octant_path}.json"

    @property
    def yolo_png(self) -> Path:
        return self.render_dir / f"{self.octant_path}_yolo.png"

    @property
    def yolo_json(self) -> Path:
        return self.render_dir / f"{self.octant_path}_yolo.json"

    def lat_lon_bbox(self) -> dict[str, float]:
        return {
            "lat_south": self.lat_south,
            "lat_north": self.lat_north,
            "lon_west": self.lon_west,
            "lon_east": self.lon_east,
        }


def glb_path_for_octant(octant_path: str, output_dir: Path = OUTPUT_DIR) -> Path:
    depth = len(octant_path)
    last_three = octant_path[-3:]
    return output_dir / str(depth) / last_three / f"{octant_path}.glb"


def load_road_polygon():
    with open(ROAD_POLYGON_WKB, "rb") as f:
        polygon = shapely.wkb.loads(f.read())
    shapely.prepare(polygon)
    return polygon


def pantelimon_bbox() -> BBox:
    return BBox(
        north=PANTELIMON_LAT_RANGE[1],
        south=PANTELIMON_LAT_RANGE[0],
        west=PANTELIMON_LON_RANGE[0],
        east=PANTELIMON_LON_RANGE[1],
    )


def find_online_d20_tiles() -> list[str]:
    planetoid = fetch_planetoid_metadata()
    root_epoch = planetoid.root_node_metadata.epoch
    level_tiles = find_tiles_in_bbox_levels(pantelimon_bbox(), TARGET_DEPTH, TARGET_DEPTH, root_epoch)
    return level_tiles.get(TARGET_DEPTH, [])


def list_road_d20_tiles(*, only_existing: bool = True) -> list[RoadTile]:
    """
    Return depth-20 tiles in the Pantelimon bbox that intersect the road polygon.

    When only_existing is True, skip tiles whose GLB is not on disk yet.
    """
    road_polygon = load_road_polygon()
    tiles: list[RoadTile] = []

    for path in find_online_d20_tiles():
        tile_box = octant_path_to_bbox(path)
        e_min, n_min = latlon_to_enu(tile_box.south, tile_box.west)
        e_max, n_max = latlon_to_enu(tile_box.north, tile_box.east)
        if not road_polygon.intersects(shapely_box(e_min, n_min, e_max, n_max)):
            continue

        glb_path = glb_path_for_octant(path)
        if only_existing and not glb_path.exists():
            continue

        tiles.append(
            RoadTile(
                octant_path=path,
                glb_path=glb_path,
                lat_south=tile_box.south,
                lat_north=tile_box.north,
                lon_west=tile_box.west,
                lon_east=tile_box.east,
            )
        )

    tiles.sort(key=lambda t: t.octant_path)
    return tiles
