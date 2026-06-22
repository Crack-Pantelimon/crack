mod map_metadata_parquet;
mod map_plugin_ui;
mod map_lod;

use bevy::asset::{Asset, AssetLoader, LoadContext, io::Reader};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use bytes::Bytes;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use crate::plugins::map_plugin::map_lod::{recompute_lod_system, sync_node_models};
use crate::plugins::map_plugin::map_metadata_parquet::{
    ParquetAsset, ParquetAssetLoader, check_and_parse_parquet, init_parquet_handles,
};
use crate::plugins::map_plugin::map_plugin_ui::{
    draw_reference_points_gizmos, draw_tree_bboxes, handle_click_raycast, tree_navigator_ui,
};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MapPlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MapPlugin...");
        app.init_asset::<ParquetAsset>()
            .init_asset_loader::<ParquetAssetLoader>()
            .init_resource::<Data3DResource>()
            .add_systems(Startup, init_parquet_handles)
            .add_systems(EguiPrimaryContextPass, tree_navigator_ui)
            .add_systems(
                Update,
                (
                    check_and_parse_parquet,
                    draw_tree_bboxes,
                    handle_click_raycast,
                    draw_reference_points_gizmos,


                    sync_node_models,
                    recompute_lod_system,
                ),
            );
        info!("done loading: MapPlugin");
    }
}

#[derive(Clone, Debug)]
pub struct MapTreeNode {
    pub name: String,
    pub r#type: String,
    pub level: Option<i32>,
    pub bbox: BBox,
    pub octant_path: String,
    pub filename: Option<String>,
    pub vertex_count: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Resource, Default, Debug)]
pub struct Data3DResource {
    pub nodes: BTreeMap<String, MapTreeNode>,
    pub children: BTreeMap<String, BTreeMap<char, String>>,
    pub parents: BTreeMap<String, String>,
    pub bbox: Option<BBox>,
    pub parsed: bool,
    pub rendered_nodes: BTreeSet<String>,
    pub selected_node: Option<String>,

    // LOD and Reference point fields
    pub reference_points: Vec<Vec3>,
    pub lod_budget: u32,
    pub roots: Vec<String>,
    pub target_rendered_nodes: Option<BTreeSet<String>>,
    pub loaded_scenes: BTreeMap<String, Handle<WorldAsset>>,
    pub loading_scenes: BTreeMap<String, Handle<WorldAsset>>,
    pub lod_timer: Option<Timer>,

    // Iterative caching fields
    pub last_reference_points: Vec<Vec3>,
    pub last_lod_budget: u32,
    pub node_distances: BTreeMap<String, Vec<f32>>,
    pub node_min_distances: BTreeMap<String, f32>,
}
