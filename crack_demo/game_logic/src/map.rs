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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapRootNodeSummary {
    pub path: MapTreeNodePath,
    pub assets: Vec<MapTileAssetInfoSummary>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MapManifestResult {
    pub bbox: BBox,
    pub roots: Vec<MapRootNodeSummary>,
    pub lod_budget: u32,
}
