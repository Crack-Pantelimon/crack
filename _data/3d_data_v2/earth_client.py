"""
Server communication with Google Earth's 3D tile endpoint.

Handles fetching PlanetoidMetadata, BulkMetadata, and NodeData
via HTTP from kh.google.com, with retry logic and protobuf parsing.
"""

from config import SOCKS_PROXY
import time
import logging
import requests
import hashlib
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
from google.protobuf.message import DecodeError
from google.protobuf.json_format import MessageToJson

import rocktree_pb2 as pb
import config

logger = logging.getLogger(__name__)


def _save_as_json(object_type: str, url_path: str, msg):
    """
    Save the parsed protobuf message as JSON in data_cache/json_decoded/...
    using the same hash-based filename schema.
    """
    sha1 = hashlib.sha1(url_path.encode("utf-8")).hexdigest()
    json_dir = Path("data_cache") / "json_decoded" / object_type / sha1[:2]
    json_file = json_dir / f"{sha1}.json"
    
    if not json_file.exists():
        json_dir.mkdir(parents=True, exist_ok=True)
        try:
            json_str = MessageToJson(msg, preserving_proto_field_name=True)
            json_file.write_text(json_str, encoding="utf-8")
        except Exception as e:
            logger.warning(f"Failed to save JSON for {url_path}: {e}")

BASE_URL = "https://kh.google.com/rt/earth/"

# Texture format constants
TEX_FORMAT_JPG = 1
TEX_FORMAT_CRN_DXT1 = 6

# NodeMetadata flag constants
FLAG_RICH3D_LEAF = 1
FLAG_RICH3D_NODATA = 2
FLAG_LEAF = 4
FLAG_NODATA = 8
FLAG_USE_IMAGERY_EPOCH = 16


def _fetch_raw(url_path: str, max_retries: int = 5, base_delay: float = 1.0) -> bytes:
    """
    Fetch raw bytes from the Google Earth endpoint with retry logic.
    """
    import hashlib
    from pathlib import Path

    object_type = url_path.split("/")[0]
    sha1 = hashlib.sha1(url_path.encode("utf-8")).hexdigest()

    cache_dir = Path("data_cache") / "raw_fetch" / object_type / sha1[:2]
    cache_file = cache_dir / f"{sha1}.bytes"

    if cache_file.exists():
        return cache_file.read_bytes()

    url = BASE_URL + url_path
    logger.info("Fetching " + url)
    headers = {
        "User-Agent": config.USER_AGENT,
        "Referer": config.REFERER,
    }
    for attempt in range(1, max_retries + 1):
        try:
            resp = requests.get(
                url, headers=headers, timeout=30,
                proxies=dict(http=SOCKS_PROXY, https=SOCKS_PROXY,),
            )
            if resp.status_code == 200:
                content = resp.content
                cache_dir.mkdir(parents=True, exist_ok=True)
                temp_file = cache_file.with_suffix(".tmp")
                try:
                    temp_file.write_bytes(content)
                    temp_file.rename(cache_file)
                except Exception as e:
                    logger.warning(f"Failed to write cache for {url_path}: {e}")
                return content
            logger.warning(f"HTTP {resp.status_code} for {url} (attempt {attempt}/{max_retries})")
        except requests.RequestException as e:
            logger.warning(f"Request error for {url}: {e} (attempt {attempt}/{max_retries})")

        if attempt < max_retries:
            delay = min(base_delay * (2 ** (attempt - 1)), 16)
            logger.info(f"Retrying in {delay}s...")
            time.sleep(delay)

    raise RuntimeError(f"Failed to fetch {url} after {max_retries} attempts")


def fetch_planetoid_metadata() -> pb.PlanetoidMetadata:
    """
    Fetch PlanetoidMetadata from the server.
    Returns the root node metadata including the root epoch.
    """
    raw = _fetch_raw("PlanetoidMetadata")
    msg = pb.PlanetoidMetadata()
    msg.ParseFromString(raw)
    _save_as_json("PlanetoidMetadata", "PlanetoidMetadata", msg)
    return msg


def fetch_bulk_metadata(path: str, epoch: int) -> pb.BulkMetadata:
    """
    Fetch BulkMetadata for a given octant path and epoch.
    URL: BulkMetadata/pb=!1m2!1s{path}!2u{epoch}
    """
    url_path = f"BulkMetadata/pb=!1m2!1s{path}!2u{epoch}"
    raw = _fetch_raw(url_path)
    msg = pb.BulkMetadata()
    msg.ParseFromString(raw)
    _save_as_json("BulkMetadata", url_path, msg)
    return msg


def fetch_node_data(
    path: str,
    epoch: int,
    texture_format: int = TEX_FORMAT_JPG,
    imagery_epoch: int | None = None,
) -> pb.NodeData:
    """
    Fetch NodeData for a given octant path, epoch, and texture format.
    URL: NodeData/pb=!1m2!1s{path}!2u{epoch}!2e{tex_fmt}[!3u{img_epoch}]!4b0
    """
    url_path = f"NodeData/pb=!1m2!1s{path}!2u{epoch}!2e{texture_format}"
    if imagery_epoch is not None:
        url_path += f"!3u{imagery_epoch}"
    url_path += "!4b0"

    raw = _fetch_raw(url_path)
    msg = pb.NodeData()
    msg.ParseFromString(raw)
    _save_as_json("NodeData", url_path, msg)
    return msg


def unpack_path_and_flags(path_and_flags: int) -> tuple[str, int]:
    """
    Unpack the path_and_flags field from NodeMetadata.

    Layout:
      - bits 0-1: path length minus 1 (so length = 1 + (val & 3))
      - next 3*length bits: path digits (3 bits each, 0-7)
      - remaining bits: flags
    """
    level = 1 + (path_and_flags & 3)
    path_and_flags >>= 2
    path = ""
    for _ in range(level):
        path += str(path_and_flags & 7)
        path_and_flags >>= 3
    flags = path_and_flags
    return path, flags


class BulkIndex:
    """
    Parsed index of a BulkMetadata message, providing fast lookup
    of node metadata by relative path.
    """

    def __init__(self, bulk: pb.BulkMetadata, parent_path: str):
        self.bulk = bulk
        self.parent_path = parent_path
        self.head_epoch = bulk.head_node_key.epoch if bulk.HasField("head_node_key") else 0

        # Build lookup from relative path -> (NodeMetadata, flags)
        self.nodes: dict[str, tuple[pb.NodeMetadata, int]] = {}
        for nm in bulk.node_metadata:
            rel_path, flags = unpack_path_and_flags(nm.path_and_flags)
            self.nodes[rel_path] = (nm, flags)

    def get_node(self, rel_path: str) -> tuple[pb.NodeMetadata, int] | None:
        """Look up a node by its relative path within this bulk."""
        return self.nodes.get(rel_path)

    def has_data(self, rel_path: str) -> bool:
        """Check if a node has renderable data (not flagged NODATA)."""
        entry = self.nodes.get(rel_path)
        if entry is None:
            return False
        _, flags = entry
        return not (flags & FLAG_NODATA)

    def is_leaf(self, rel_path: str) -> bool:
        """Check if a node is a leaf (no children)."""
        entry = self.nodes.get(rel_path)
        if entry is None:
            return True
        _, flags = entry
        return bool(flags & FLAG_LEAF)

    def has_bulk_children(self, rel_path: str) -> bool:
        """Check if a node's 4-char children have sub-bulk metadata."""
        entry = self.nodes.get(rel_path)
        if entry is None:
            return False
        _, flags = entry
        return len(rel_path) == 4 and not (flags & FLAG_LEAF)

    def get_node_epoch(self, rel_path: str) -> int:
        """Get the epoch for a specific node."""
        entry = self.nodes.get(rel_path)
        if entry is None:
            return self.head_epoch
        nm, _ = entry
        return nm.epoch if nm.HasField("epoch") else self.head_epoch

    def get_bulk_epoch(self, rel_path: str) -> int:
        """Get the bulk metadata epoch for a child bulk."""
        entry = self.nodes.get(rel_path)
        if entry is None:
            return self.head_epoch
        nm, _ = entry
        return nm.bulk_metadata_epoch if nm.HasField("bulk_metadata_epoch") else self.head_epoch

    def get_texture_format(self, rel_path: str) -> int:
        """Get the available texture format for a node."""
        entry = self.nodes.get(rel_path)
        if entry is None:
            return TEX_FORMAT_JPG

        nm, _ = entry
        if nm.HasField("available_texture_formats"):
            available = nm.available_texture_formats
        elif self.bulk.HasField("default_available_texture_formats"):
            available = self.bulk.default_available_texture_formats
        else:
            return TEX_FORMAT_JPG

        # Prefer JPG, fall back to CRN_DXT1
        if available & (1 << (TEX_FORMAT_JPG - 1)):
            return TEX_FORMAT_JPG
        if available & (1 << (TEX_FORMAT_CRN_DXT1 - 1)):
            return TEX_FORMAT_CRN_DXT1
        return TEX_FORMAT_JPG

    def get_imagery_epoch(self, rel_path: str) -> int | None:
        """Get the imagery epoch if USE_IMAGERY_EPOCH flag is set."""
        entry = self.nodes.get(rel_path)
        if entry is None:
            return None
        nm, flags = entry
        if not (flags & FLAG_USE_IMAGERY_EPOCH):
            return None
        if nm.HasField("imagery_epoch"):
            return nm.imagery_epoch
        if self.bulk.HasField("default_imagery_epoch"):
            return self.bulk.default_imagery_epoch
        return None


class NodeInfo:
    """Resolved information needed to download a NodeData."""

    def __init__(self, path: str, epoch: int, texture_format: int, imagery_epoch: int | None):
        self.path = path
        self.epoch = epoch
        self.texture_format = texture_format
        self.imagery_epoch = imagery_epoch


import threading
_bulk_lock = threading.Lock()

# Cache for bulk metadata to avoid redundant downloads
_bulk_cache: dict[str, BulkIndex] = {}


def _get_bulk(path: str, epoch: int) -> BulkIndex:
    """
    Fetch and cache a BulkIndex.

    The network fetch happens OUTSIDE the lock (double-checked caching) so that
    many bulks can be fetched concurrently. Two threads racing on the same key may
    both fetch (rare, and the raw bytes are disk-cached anyway); we keep whichever
    instance lands in the cache first.
    """
    cache_key = f"{path}:{epoch}"
    with _bulk_lock:
        idx = _bulk_cache.get(cache_key)
    if idx is not None:
        return idx

    bulk = fetch_bulk_metadata(path, epoch)
    idx = BulkIndex(bulk, path)

    with _bulk_lock:
        existing = _bulk_cache.get(cache_key)
        if existing is not None:
            return existing
        _bulk_cache[cache_key] = idx
    return idx


def resolve_node(octant_path: str, root_epoch: int) -> NodeInfo | None:
    """
    Walk the bulk metadata tree from the root to resolve a specific octant path.

    BulkMetadata covers 4 levels of the tree at a time:
    - Root bulk at "" covers paths of length 1-4
    - Child bulk at "ABCD" covers paths of relative length 1-4 (absolute 5-8)
    - etc.

    Returns NodeInfo with the epoch/format needed to download the node,
    or None if the node doesn't exist or has no data.
    """
    # Walk in chunks of 4 characters
    # The full path starts with 2 root chars, then chunks of variable length
    # Actually, bulk metadata chunks start at 0, 4, 8, ... character offsets
    # from the full path. But the first 2 chars are special (root quadrant).
    #
    # From the reference code:
    # - Root bulk at path "" covers relative paths of length 1-4
    # - For path "30201234", we'd do:
    #   1. Fetch bulk("", root_epoch), look up "3020" (4 chars)
    #   2. Fetch bulk("3020", child_epoch), look up "1234" (4 chars)

    bulk_path = ""
    epoch = root_epoch

    # Process in chunks of 4 from the full octant path
    # Chunks: path[0:4], path[4:8], path[8:12], ...
    chunks = []
    for i in range(0, len(octant_path), 4):
        chunks.append(octant_path[i : i + 4])

    for chunk_idx, chunk in enumerate(chunks):
        is_last = chunk_idx == len(chunks) - 1

        try:
            bulk_idx = _get_bulk(bulk_path, epoch)
        except Exception as e:
            logger.warning(f"Failed to fetch bulk at '{bulk_path}' epoch={epoch}: {e}")
            return None

        entry = bulk_idx.get_node(chunk)
        if entry is None:
            logger.debug(f"Node '{chunk}' not found in bulk '{bulk_path}'")
            return None

        nm, flags = entry

        if is_last:
            # This is our target node
            if flags & FLAG_NODATA:
                logger.debug(f"Node '{octant_path}' has NODATA flag")
                return None

            node_epoch = bulk_idx.get_node_epoch(chunk)
            tex_format = bulk_idx.get_texture_format(chunk)
            img_epoch = bulk_idx.get_imagery_epoch(chunk)

            return NodeInfo(
                path=octant_path,
                epoch=node_epoch,
                texture_format=tex_format,
                imagery_epoch=img_epoch,
            )
        else:
            # Need to go deeper — get child bulk epoch
            epoch = bulk_idx.get_bulk_epoch(chunk)
            bulk_path = octant_path[: (chunk_idx + 1) * 4]

    return None


def download_node(node_info: NodeInfo) -> pb.NodeData:
    """Download and parse NodeData for a resolved node."""
    return fetch_node_data(
        path=node_info.path,
        epoch=node_info.epoch,
        texture_format=node_info.texture_format,
        imagery_epoch=node_info.imagery_epoch,
    )


def find_tiles_in_bbox(bbox, target_level: int, root_epoch: int) -> list[str]:
    """
    Dynamically traverse the bulk metadata tree to find all non-empty tiles
    at the target level that overlap with the bounding box.
    """
    from octree import octant_path_to_bbox

    def overlap(box1, box2):
        return not (
            box1.south > box2.north or
            box1.north < box2.south or
            box1.east < box2.west or
            box1.west > box2.east
        )

    def check_node_traversable(path: str) -> bool:
        chunks = []
        for i in range(0, len(path), 4):
            chunks.append(path[i : i + 4])
        bulk_path = ""
        epoch = root_epoch
        for chunk_idx, chunk in enumerate(chunks):
            is_last = chunk_idx == len(chunks) - 1
            try:
                bulk_idx = _get_bulk(bulk_path, epoch)
            except Exception:
                return False
            entry = bulk_idx.get_node(chunk)
            if entry is None:
                return False
            _, flags = entry
            if flags & FLAG_LEAF:
                return False
            if not is_last:
                epoch = bulk_idx.get_bulk_epoch(chunk)
                bulk_path = path[: (chunk_idx + 1) * 4]
        return True

    results = []

    def traverse(path: str, box):
        if len(path) == target_level:
            node_info = resolve_node(path, root_epoch)
            if node_info is not None:
                results.append(path)
            return

        if len(path) < target_level:
            if not check_node_traversable(path):
                return
            for child_digit in range(8):
                child_path = path + str(child_digit)
                child_box = octant_path_to_bbox(child_path)
                if overlap(child_box, bbox):
                    traverse(child_path, child_box)

    # Roots to check
    roots = ["02", "03", "12", "13", "20", "21", "30", "31"]
    for r in roots:
        r_box = octant_path_to_bbox(r)
        if overlap(r_box, bbox):
            traverse(r, r_box)

    return results


# Number of concurrent BulkMetadata fetches during multi-level enumeration.
BULK_FETCH_WORKERS = 100


def find_tiles_in_bbox_levels(
    bbox,
    min_level: int,
    max_level: int,
    root_epoch: int,
    bulk_fetch_workers: int = BULK_FETCH_WORKERS,
) -> dict[int, list[str]]:
    """
    Enumerate all non-empty tiles overlapping `bbox` for every level in
    [min_level, max_level], in a single breadth-first pass.

    This is far faster than calling find_tiles_in_bbox once per level because:
      * the bulk-metadata context is carried down the tree, so each node costs a
        single dict lookup instead of re-walking from the root;
      * child bounding boxes are subdivided incrementally (octree.child_bbox);
      * at each BFS frontier, all child BulkMetadata blobs that need to be fetched
        are downloaded concurrently. Crossing a bulk boundary (every 4 octree
        levels, e.g. level 17 and 21) otherwise triggers thousands of *serial*
        network requests, which is what made deep levels appear to hang.

    A node enters the result for its level when it exists and is not flagged
    NODATA; the tree is only descended through existing, non-leaf nodes (matching
    the semantics of find_tiles_in_bbox).
    """
    from octree import octant_path_to_bbox, child_bbox

    def overlap(box1, box2):
        return not (
            box1.south > box2.north or
            box1.north < box2.south or
            box1.east < box2.west or
            box1.west > box2.east
        )

    results: dict[int, list[str]] = {lvl: [] for lvl in range(min_level, max_level + 1)}

    def record(path: str, flags: int):
        level = len(path)
        if min_level <= level <= max_level and not (flags & FLAG_NODATA):
            results[level].append(path)

    try:
        root_bulk = _get_bulk("", root_epoch)
    except Exception as e:
        logger.warning(f"Failed to fetch root bulk: {e}")
        return results

    # Frontier entries: (path, box, bulk_idx, bulk_len). bulk_len is the absolute
    # path length covered by bulk_idx, so the relative lookup key is path[bulk_len:].
    frontier: list[tuple] = []
    roots = ["02", "03", "12", "13", "20", "21", "30", "31"]
    for r in roots:
        r_box = octant_path_to_bbox(r)
        if not overlap(r_box, bbox):
            continue
        entry = root_bulk.get_node(r)
        if entry is None:
            continue
        _, flags = entry
        record(r, flags)
        if len(r) < max_level and not (flags & FLAG_LEAF):
            frontier.append((r, r_box, root_bulk, 0))

    current_level = 2
    while frontier and current_level < max_level:
        # Step 1: parents sitting on a bulk boundary (relative path length 4) need
        # their child bulk fetched. Do all of those fetches in parallel.
        boundary_parents = [p for p in frontier if len(p[0]) - p[3] == 4]
        child_bulk_by_path: dict[str, BulkIndex] = {}

        if boundary_parents:
            def _prefetch(parent):
                path, _box, bulk_idx, bulk_len = parent
                rel = path[bulk_len:]
                epoch = bulk_idx.get_bulk_epoch(rel)
                try:
                    return path, _get_bulk(path, epoch)
                except Exception as e:
                    logger.debug(f"Failed to fetch child bulk at '{path}': {e}")
                    return path, None

            workers = max(1, min(bulk_fetch_workers, len(boundary_parents)))
            with ThreadPoolExecutor(max_workers=workers) as ex:
                for path, child_bulk in ex.map(_prefetch, boundary_parents):
                    child_bulk_by_path[path] = child_bulk

        # Step 2: expand every parent into its (in-bbox) children.
        next_frontier: list[tuple] = []
        for path, box, bulk_idx, bulk_len in frontier:
            if len(path) - bulk_len == 4:
                child_bulk = child_bulk_by_path.get(path)
                if child_bulk is None:
                    continue
                cbi, cbl = child_bulk, len(path)
            else:
                cbi, cbl = bulk_idx, bulk_len

            for d in range(8):
                child_path = path + str(d)
                cbox = child_bbox(box, d)
                if not overlap(cbox, bbox):
                    continue
                entry = cbi.get_node(child_path[cbl:])
                if entry is None:
                    continue
                _, flags = entry
                record(child_path, flags)
                if len(child_path) < max_level and not (flags & FLAG_LEAF):
                    next_frontier.append((child_path, cbox, cbi, cbl))

        frontier = next_frontier
        current_level += 1

    return results

