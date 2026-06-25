use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::plugins::map_plugin::{
    MapLODState, MapTree, MapTreeNodeInfo, MapTreeNodePath, map_lod::TreeMapTile,
};

pub fn draw_tree_bboxes(
    mut gizmos: Gizmos,
    data_res: Res<MapTree>,
    lod_state: Res<MapLODState>,
    tiles_query: Query<&TreeMapTile>,
) {
    if !data_res.parsed {
        return;
    }

    // Use a set to avoid drawing the same node's bounding box multiple times
    let mut drawn = std::collections::BTreeSet::new();

    for tile in tiles_query.iter() {
        let node_path = &tile.node_path;
        if drawn.insert(node_path.clone()) {
            if let Some(node) = data_res.all_nodes.get(node_path) {
                let is_selected = lod_state.selected_node.as_ref() == Some(&node_path.0);
                let color = if is_selected {
                    Color::srgba(1.0, 0.0, 0.0, 0.3) // Red if selected
                } else if data_res.parents.get(node_path).is_none() {
                    Color::srgba(0.0, 1.0, 0.0, 0.3) // Green for root
                } else {
                    Color::srgba(0.0, 0.5, 1.0, 0.3) // Blue for others
                };
                draw_node_bbox(&mut gizmos, node, color);
            }
        }
    }
}

fn draw_node_bbox(gizmos: &mut Gizmos, node: &MapTreeNodeInfo, color: Color) {
    let center = Vec3::new(
        (node.bbox.min.x + node.bbox.max.x) / 2.0,
        (node.bbox.min.y + node.bbox.max.y) / 2.0,
        (node.bbox.min.z + node.bbox.max.z) / 2.0,
    );
    let size = Vec3::new(
        (node.bbox.max.x - node.bbox.min.x).abs(),
        (node.bbox.max.y - node.bbox.min.y).abs(),
        (node.bbox.max.z - node.bbox.min.z).abs(),
    );
    let cuboid = Cuboid::new(size.x, size.y, size.z);
    gizmos.primitive_3d(&cuboid, Isometry3d::from_translation(center), color);
}

pub fn tree_navigator_ui(
    mut contexts: EguiContexts,
    data_res: Res<MapTree>,
    mut lod_state: ResMut<MapLODState>,
    tiles_query: Query<&TreeMapTile>,
) {
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
    let mut total_vertices = 0;
    for tile in tiles_query.iter() {
        if let Some(asset_info) = data_res.assets.get(&tile.asset_id) {
            if let Some(vc) = asset_info.vertex_count {
                total_vertices += vc;
            }
        }
    }

    egui::Window::new("LOD Configuration & Tree Navigator").show(ctx, |ui| {
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
                if let Some(node) = data_res.all_nodes.get(&node_path) {
                    let is_selected = lod_state.selected_node.as_ref() == Some(&node_path.0);
                    let label_text = format!(
                        "Path: {} | Assets: {} | BBox: {:?}",
                        node.path.0,
                        node.assets.len(),
                        node.bbox
                    );

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
            }
        });
    });

    if node_to_deselect {
        lod_state.selected_node = None;
    } else if let Some(name) = node_to_select {
        lod_state.selected_node = Some(name);
    }
}

pub fn handle_click_raycast(
    mut lod_state: ResMut<MapLODState>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: avian3d::prelude::SpatialQuery,
    mut contexts: EguiContexts,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
        return;
    }

    if mouse_button.just_pressed(MouseButton::Left) {
        let Ok(window) = window_query.single() else {
            return;
        };
        if let Some(cursor_pos) = window.cursor_position() {
            let Ok((camera, camera_transform)) = camera_query.single() else {
                return;
            };

            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if let Some(hit) = spatial_query.cast_ray(
                    ray.origin,
                    ray.direction,
                    10000.0,
                    true,
                    &avian3d::prelude::SpatialQueryFilter::default(),
                ) {
                    let hit_point = ray.origin + *ray.direction * hit.distance;
                    lod_state.reference_points.push(hit_point);
                    info!("Added reference point at {:?}", hit_point);
                }
            }
        }
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
