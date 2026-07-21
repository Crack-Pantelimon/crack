use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Axis-aligned bounding box in world space.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct BBox {
    /// Minimum corner of the box.
    pub min: Vec3,
    /// Maximum corner of the box.
    pub max: Vec3,
}

/// Stable string id for one renderable map tile asset.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MapTileAssetId(pub String);
impl MapTileAssetId {
    /// Returns the octant path derived from this asset id.
    pub fn get_octant_path(&self) -> MapTreeNodePath {
        MapTreeNodePath(self.0.clone())
    }
}

/// Octree node path encoded as a digit string (e.g. `"023"`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MapTreeNodePath(pub String);
impl MapTreeNodePath {
    /// Returns the parent path, or `None` for the empty root path.
    pub fn get_parent(&self) -> Option<MapTreeNodePath> {
        if self.0.is_empty() {
            return None;
        }
        let mut s = self.0.clone();
        s.pop();
        Some(MapTreeNodePath(s))
    }
}

/// Metadata for one tile asset in the map manifest.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapTreeAssetInfo {
    /// Asset id matching the tile name.
    pub name: MapTileAssetId,
    /// Octree depth level when known.
    pub level: Option<i32>,
    /// World-space bounds of the tile mesh.
    pub bbox: BBox,
    /// Octant path of the owning node.
    pub _octant_path: MapTreeNodePath,
    /// Relative path to the tile `.glb` when available.
    pub glb_path: Option<String>,
    /// Total vertex count across meshes when known.
    pub vertex_count: Option<i64>,
    /// Number of meshes in the GLB when known.
    pub mesh_count: Option<i64>,
}

/// One node in the map octree listing its child assets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapTreeNodeInfo {
    /// Octant path identifying this node.
    pub path: MapTreeNodePath,
    /// Asset ids attached to this node.
    pub assets: Vec<MapTileAssetId>,
    /// Union bounds of assets on this node.
    pub bbox: BBox,
}

/// Full in-memory map octree built from the manifest parquet.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapTreeData {
    /// All tile assets keyed by id.
    pub assets: BTreeMap<MapTileAssetId, MapTreeAssetInfo>,
    /// Node metadata keyed by octant path.
    pub all_nodes: BTreeMap<MapTreeNodePath, MapTreeNodeInfo>,
    /// Child paths keyed by parent path.
    pub children: BTreeMap<MapTreeNodePath, BTreeSet<MapTreeNodePath>>,
    /// Parent path keyed by child path.
    pub parents: BTreeMap<MapTreeNodePath, MapTreeNodePath>,
    /// Playable world extent derived from geo bounds and root tiles.
    pub bbox: BBox,
    /// Top-level octree root paths.
    pub roots: BTreeSet<MapTreeNodePath>,
    /// Coarse horizon tiles (octree depth < 14) kept worker-side for fake-map rings.
    pub coarse_assets: Vec<MapTreeAssetInfo>,
}

/// Lightweight tile descriptor for procedural horizon rings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FakeMapTile {
    /// Octant path string for the placeholder tile.
    pub octant_path: String,
    /// Relative path to the placeholder `.glb`.
    pub glb_path: String,
    /// World-space bounds of the placeholder tile.
    pub bbox: BBox,
    /// Octree depth of this placeholder.
    pub depth: i32,
}

/// Client-facing summary of one tile asset on a node.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapTileAssetInfoSummary {
    /// Asset id for the tile.
    pub name: MapTileAssetId,
    /// Relative path to the tile `.glb`.
    pub glb_path: String,
    /// World-space bounds of the tile mesh.
    pub bbox: BBox,
}

/// Client-facing summary of one root octree node and its assets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapRootNodeSummary {
    /// Octant path of the root node.
    pub path: MapTreeNodePath,
    /// Renderable assets attached to this node.
    pub assets: Vec<MapTileAssetInfoSummary>,
    /// Union bounds of assets on this node.
    pub bbox: BBox,
}

/// Map manifest returned to clients after parquet ingestion.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapManifestResult {
    /// Playable world extent.
    pub bbox: BBox,
    /// Root nodes with asset summaries for initial spawn.
    pub roots: Vec<MapRootNodeSummary>,
    /// Maximum simultaneous tile asset budget for LOD.
    pub lod_budget: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_map_tree_node_info_serde_round_trip() {
        let node = MapTreeNodeInfo {
            path: MapTreeNodePath("02".to_string()),
            assets: vec![MapTileAssetId("tile_02".to_string())],
            bbox: BBox {
                min: Vec3::new(-1.0, 0.0, -1.0),
                max: Vec3::new(1.0, 10.0, 1.0),
            },
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: MapTreeNodeInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, node.path);
        assert_eq!(back.assets, node.assets);
        assert_eq!(back.bbox.min, node.bbox.min);
        assert_eq!(back.bbox.max, node.bbox.max);
    }
}
