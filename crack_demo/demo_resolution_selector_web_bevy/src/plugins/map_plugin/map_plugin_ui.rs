use bevy::gizmos::config::GizmoConfigStore;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::plugins::map_plugin::{MapLODState, MapTree, MapTreeNodePath, map_lod::TreeMapTile};

/// Gizmo group with default depth testing (no `depth_bias = -1` always-on-top hack).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub(crate) struct MapExtentGizmoGroup;

pub fn configure_map_extent_gizmo(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<MapExtentGizmoGroup>();
    config.depth_bias = 0.0;
    config.line.width = 1.0;
}

pub fn draw_tree_bboxes(
    _gizmos: Gizmos,
    _data_res: Res<MapTree>,
    _lod_state: Res<MapLODState>,
    _tiles_query: Query<&TreeMapTile>,
    _ui_state: Option<Res<crate::ui_egui::UiState>>,
) {
    // Per-tile bbox drawing disabled on client; see `draw_map_extent_gizmo`.
}

fn draw_bbox_wireframe(
    gizmos: &mut Gizmos<MapExtentGizmoGroup>,
    min: Vec3,
    max: Vec3,
    color: Color,
) {
    let c = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (a, b) in edges {
        gizmos.line(c[a], c[b], color);
    }
}

pub fn draw_map_extent_gizmo(
    mut gizmos: Gizmos<MapExtentGizmoGroup>,
    data_res: Res<MapTree>,
    ui_state: Option<Res<crate::ui_egui::UiState>>,
) {
    let Some(state) = ui_state else {
        return;
    };
    if !state.draw_map_bboxes || !data_res.parsed {
        return;
    }

    let min = data_res.bbox.min;
    let max = data_res.bbox.max;
    draw_bbox_wireframe(&mut gizmos, min, max, Color::BLACK);
}

pub fn tree_navigator_ui(
    mut contexts: EguiContexts,
    data_res: Res<MapTree>,
    mut lod_state: ResMut<MapLODState>,
    tiles_query: Query<&TreeMapTile>,
    ui_state: Option<ResMut<crate::ui_egui::UiState>>,
) {
    let Some(mut state) = ui_state else {
        return;
    };
    if !state.show_lod_configurator {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !data_res.parsed {
        return;
    }

    let mut node_to_select = None;
    let mut node_to_deselect = false;

    // Calculate metrics
    let rendered_paths: std::collections::BTreeSet<MapTreeNodePath> = tiles_query
        .iter()
        .map(|tile| tile.node_path.clone())
        .collect();
    let num_nodes = rendered_paths.len();
    let num_assets = tiles_query.iter().count();
    let total_vertices = 0;

    egui::Window::new("LOD Configuration & Tree Navigator")
        .open(&mut state.show_lod_configurator)
        .show(ctx, |ui| {
            // Slider for budget: roots.len() to 1000
            let min_budget = data_res.roots.len() as u32;
            let mut budget = lod_state.lod_budget;
            ui.horizontal(|ui| {
                ui.label("Budget:");
                ui.add(egui::Slider::new(&mut budget, min_budget..=1000).text(""));
            });
            if budget != lod_state.lod_budget {
                lod_state.lod_budget = budget;
            }

            let mut max_lod = lod_state.max_lod;
            ui.horizontal(|ui| {
                ui.label("Max LOD:");
                ui.add(egui::Slider::new(&mut max_lod, 16..=24).text(""));
            });
            if max_lod != lod_state.max_lod {
                lod_state.max_lod = max_lod;
            }

            let mut tiles_per_diagonal = lod_state.tiles_per_diagonal;
            ui.horizontal(|ui| {
                ui.label("Tiles per diagonal:");
                ui.add(egui::Slider::new(&mut tiles_per_diagonal, 0.3..=2.5).text(""));
            });
            if tiles_per_diagonal != lod_state.tiles_per_diagonal {
                lod_state.tiles_per_diagonal = tiles_per_diagonal;
            }

            ui.checkbox(
                &mut lod_state.enable_visibility_cull,
                "Enable Visibility Culling",
            );

            ui.separator();

            ui.heading("Reference Points");
            let mut to_remove = None;
            for (i, pt) in lod_state.reference_points.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("Pt {}: ({:.1}, {:.1}, {:.1})", i, pt.x, pt.y, pt.z));
                    if ui.button("Remove").clicked() {
                        to_remove = Some(i);
                    }
                });
            }
            if let Some(idx) = to_remove {
                lod_state.reference_points.remove(idx);
            }

            ui.separator();
            ui.label(format!("Spawned Nodes: {}", num_nodes));
            ui.label(format!("Spawned Assets: {}", num_assets));
            ui.label(format!("Spawned Vertices: {}", total_vertices));

            ui.separator();
            ui.heading("Tree Navigator");

            egui::ScrollArea::vertical().show(ui, |ui| {
                for node_path in rendered_paths {
                    let is_selected = lod_state.selected_node.as_ref() == Some(&node_path.0);
                    let label_text = format!("Path: {}", node_path.0);

                    ui.horizontal(|ui| {
                        let resp = ui.selectable_label(is_selected, label_text);
                        if resp.clicked() {
                            if is_selected {
                                node_to_deselect = true;
                            } else {
                                node_to_select = Some(node_path.0.clone());
                            }
                        }
                    });
                }
            });
        });

    if node_to_deselect {
        lod_state.selected_node = None;
    } else if let Some(name) = node_to_select {
        lod_state.selected_node = Some(name);
    }
}

pub fn draw_reference_points_gizmos(
    mut gizmos: Gizmos,
    data_res: Res<MapTree>,
    lod_state: Res<MapLODState>,
    camera_query: Query<&Transform, With<Camera>>,
) {
    if !data_res.parsed {
        return;
    }
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };
    let camera_pos = camera_transform.translation;

    for pt in &lod_state.reference_points {
        let dist = camera_pos.distance(*pt);
        let radius = dist * 0.02; // 2% of the distance
        let sphere = Sphere::new(radius);
        gizmos.primitive_3d(
            &sphere,
            Isometry3d::from_translation(*pt),
            Color::srgb(1.0, 0.5, 0.0),
        );
    }
}
