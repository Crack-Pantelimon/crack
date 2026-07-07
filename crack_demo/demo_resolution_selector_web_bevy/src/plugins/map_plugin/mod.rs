pub mod map_lod;
pub mod map_material_edit;
mod map_plugin_ui;

pub use map_lod::TreeMapTile;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use std::collections::{BTreeMap, BTreeSet};

use crate::plugins::map_plugin::map_lod::{
    TileSwapRequests, check_map_loaded_status, do_merge_requests, do_split_requests,
    reveal_pending_tiles, spawn_root_map_tiles, start_tile_swap_requests,
};
use crate::plugins::map_plugin::map_plugin_ui::{
    draw_reference_points_gizmos, draw_tree_bboxes, tree_navigator_ui,
};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MapPlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MapPlugin...");
        app.init_resource::<MapTree>()
            .init_resource::<MapLODState>()
            .init_resource::<TileSwapRequests>()
            .add_plugins(map_material_edit::MapMaterialEditPlugin)
            .add_systems(EguiPrimaryContextPass, tree_navigator_ui)
            .add_systems(
                Update,
                (
                    draw_tree_bboxes,
                    draw_reference_points_gizmos,
                    spawn_root_map_tiles,
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

pub use game_logic::map::{
    BBox, MapTileAssetId, MapTreeAssetInfo, MapTreeData, MapTreeNodeInfo, MapTreeNodePath,
};

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
