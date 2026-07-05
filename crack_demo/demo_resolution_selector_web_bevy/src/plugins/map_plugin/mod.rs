mod map_lod;
pub mod map_material_edit;
mod map_metadata_parquet;
mod map_plugin_ui;

pub use map_lod::TreeMapTile;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use std::collections::{BTreeMap, BTreeSet};

use crate::plugins::map_plugin::map_lod::{
    TileSwapRequests, check_map_loaded_status, do_merge_requests, do_split_requests,
    recompute_lod_mark_changes, reveal_pending_tiles, spawn_root_map_tiles,
    start_tile_swap_requests,
};
use crate::plugins::map_plugin::map_metadata_parquet::{
    ParquetAsset, ParquetAssetLoader, check_and_parse_parquet, init_parquet_handles,
};
use crate::plugins::map_plugin::map_plugin_ui::{
    draw_reference_points_gizmos, draw_tree_bboxes, tree_navigator_ui,
};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MapPlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MapPlugin...");
        app.init_asset::<ParquetAsset>()
            .init_asset_loader::<ParquetAssetLoader>()
            .init_resource::<MapTree>()
            .init_resource::<MapLODState>()
            .init_resource::<TileSwapRequests>()
            .add_plugins(map_material_edit::MapMaterialEditPlugin)
            .add_systems(Startup, init_parquet_handles)
            .add_systems(EguiPrimaryContextPass, tree_navigator_ui)
            .add_systems(
                Update,
                (
                    check_and_parse_parquet,
                    draw_tree_bboxes,
                    draw_reference_points_gizmos,
                    spawn_root_map_tiles,
                    recompute_lod_mark_changes,
                    reveal_pending_tiles,
                    check_map_loaded_status,
                ),
            )
            .add_systems(PostUpdate, (start_tile_swap_requests,))
            .add_systems(PreUpdate, (do_split_requests,))
            .add_systems(First, (do_merge_requests,));
        info!("done loading: MapPlugin");
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct MapTreeAssetInfo {
    pub name: MapTileAssetId,
    pub level: Option<i32>,
    pub bbox: BBox,
    pub _octant_path: MapTreeNodePath,
    pub glb_path: Option<String>,
    pub vertex_count: Option<i64>,
    pub mesh_count: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BBox {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MapTileAssetId(pub String);
impl MapTileAssetId {
    pub fn get_octant_path(&self) -> MapTreeNodePath {
        MapTreeNodePath(self.0.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    // pub fn is_empty(&self) -> bool {
    //     return self.0.is_empty();
    // }
}

#[derive(Clone, Debug)]
pub struct MapTreeNodeInfo {
    pub path: MapTreeNodePath,
    pub assets: Vec<MapTileAssetId>,
    pub bbox: BBox,
}

#[derive(Resource, Default, Debug)]
pub struct MapTree {
    pub assets: BTreeMap<MapTileAssetId, MapTreeAssetInfo>,

    pub all_nodes: BTreeMap<MapTreeNodePath, MapTreeNodeInfo>,
    pub children: BTreeMap<MapTreeNodePath, BTreeSet<MapTreeNodePath>>,
    pub parents: BTreeMap<MapTreeNodePath, MapTreeNodePath>,
    pub bbox: BBox,
    pub roots: BTreeSet<MapTreeNodePath>,
    pub parsed: bool,
}

#[derive(Resource, Default, Debug)]
pub struct MapLODState {
    // pub rendered_nodes: BTreeSet<String>,
    pub selected_node: Option<String>,
    pub reference_points: Vec<Vec3>,
    pub lod_budget: u32,
    pub lod_timer: Option<Timer>,
    pub max_lod: i32,
    pub tiles_per_diagonal: f32,
}
