use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct BBox {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MapTileAssetId(pub String);
impl MapTileAssetId {
    pub fn get_octant_path(&self) -> MapTreeNodePath {
        MapTreeNodePath(self.0.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MapTreeNodePath(pub String);
impl MapTreeNodePath {
    pub fn get_parent(&self) -> Option<MapTreeNodePath> {
        if self.0.is_empty() {
            return None;
        }
        let mut s = self.0.clone();
        s.pop();
        Some(MapTreeNodePath(s))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapTreeAssetInfo {
    pub name: MapTileAssetId,
    pub level: Option<i32>,
    pub bbox: BBox,
    pub _octant_path: MapTreeNodePath,
    pub glb_path: Option<String>,
    pub vertex_count: Option<i64>,
    pub mesh_count: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapTreeNodeInfo {
    pub path: MapTreeNodePath,
    pub assets: Vec<MapTileAssetId>,
    pub bbox: BBox,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapTreeData {
    pub assets: BTreeMap<MapTileAssetId, MapTreeAssetInfo>,
    pub all_nodes: BTreeMap<MapTreeNodePath, MapTreeNodeInfo>,
    pub children: BTreeMap<MapTreeNodePath, BTreeSet<MapTreeNodePath>>,
    pub parents: BTreeMap<MapTreeNodePath, MapTreeNodePath>,
    pub bbox: BBox,
    pub roots: BTreeSet<MapTreeNodePath>,
    /// Coarse horizon tiles (octree depth < 14) kept worker-side for fake-map rings.
    pub coarse_assets: Vec<MapTreeAssetInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FakeMapTile {
    pub octant_path: String,
    pub glb_path: String,
    pub bbox: BBox,
    pub depth: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapTileAssetInfoSummary {
    pub name: MapTileAssetId,
    pub glb_path: String,
    pub bbox: BBox,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapRootNodeSummary {
    pub path: MapTreeNodePath,
    pub assets: Vec<MapTileAssetInfoSummary>,
    pub bbox: BBox,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapManifestResult {
    pub bbox: BBox,
    pub roots: Vec<MapRootNodeSummary>,
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
