pub mod bvh_minimap;
pub mod fake_map;
pub mod map_lod;
pub mod map_material_edit;
mod map_plugin_ui;

pub use map_lod::TreeMapTile;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::plugins::map_plugin::map_lod::{
    TileSwapRequests, check_map_loaded_status, do_merge_requests, do_split_requests,
    poll_tile_group_fetches, reveal_pending_tiles, spawn_root_map_tiles, start_tile_swap_requests,
};
use crate::plugins::map_plugin::map_plugin_ui::{
    configure_map_extent_gizmo, draw_map_extent_gizmo, draw_reference_points_gizmos,
    draw_tree_bboxes, tree_navigator_ui,
};
use crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MapPlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MapPlugin...");
        app.init_resource::<MapTree>()
            .init_resource::<MapLODState>()
            .init_resource::<TileSwapRequests>()
            .init_gizmo_group::<map_plugin_ui::MapExtentGizmoGroup>()
            .add_plugins(map_material_edit::MapMaterialEditPlugin)
            .add_plugins(fake_map::FakeMapPlugin)
            .add_systems(Startup, configure_map_extent_gizmo)
            .add_systems(
                EguiPrimaryContextPass,
                (tree_navigator_ui, bvh_minimap::bvh_minimap_window),
            )
            .add_systems(
                Update,
                (
                    draw_tree_bboxes,
                    draw_map_extent_gizmo,
                    draw_reference_points_gizmos,
                    spawn_root_map_tiles,
                    poll_tile_group_fetches,
                    reveal_pending_tiles,
                    check_map_loaded_status,
                ),
            )
            .add_systems(PostUpdate, (start_tile_swap_requests,))
            .add_systems(PreUpdate, (do_split_requests,))
            .add_systems(First, (do_merge_requests,))
            .add_systems(Last, clamp_camera_to_map_bbox);
        info!("done loading: MapPlugin");
    }
}

/// Vertical headroom above the map bbox top — keeps freecam / orbit cameras from drifting
/// arbitrarily high while still leaving Y mostly self-managed.
const CAMERA_BBOX_Y_HEADROOM: f32 = 10.0;

fn clamp_camera_to_map_bbox(
    map_tree: Option<Res<MapTree>>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    let Some(map_tree) = map_tree else {
        return;
    };
    if !map_tree.parsed {
        return;
    }

    let bbox = &map_tree.bbox;
    let min_x = bbox.min.x.min(bbox.max.x);
    let max_x = bbox.min.x.max(bbox.max.x);
    let min_z = bbox.min.z.min(bbox.max.z);
    let max_z = bbox.min.z.max(bbox.max.z);
    let max_y = bbox.min.y.max(bbox.max.y) + CAMERA_BBOX_Y_HEADROOM;

    for mut t in &mut cam {
        t.translation.x = t.translation.x.clamp(min_x, max_x);
        t.translation.z = t.translation.z.clamp(min_z, max_z);
        t.translation.y = t.translation.y.min(max_y);
    }
}

pub use game_logic::map::{
    BBox, MapRootNodeSummary, MapTileAssetId, MapTileAssetInfoSummary, MapTreeAssetInfo,
    MapTreeData, MapTreeNodeInfo, MapTreeNodePath,
};

#[derive(Resource, Default, Debug)]
pub struct MapTree {
    pub bbox: BBox,
    pub roots: Vec<MapRootNodeSummary>,
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
    /// Detail floor: below this level tiles always split (no visibility rays).
    pub min_tiles_per_diagonal: f32,
    /// Detail ceiling: between min and max, splits require BVH visibility.
    pub max_tiles_per_diagonal: f32,
    pub enable_visibility_cull: bool,
    pub sample_radius_freecam: f32,
    pub sample_radius_car: f32,
    pub sample_radius_pedestrian: f32,
}
