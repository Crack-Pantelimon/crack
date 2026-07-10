//! Debug 3D minimap of the map-tile octree (Debug > 3D BVH Minimap).
//!
//! Renders the cubic bounding boxes of every currently spawned map tile into a corner window,
//! seen from a virtual camera very high above the map so all boxes fit in frame. Tile boxes are
//! colored by their LOD state (active / pending reveal / splitting / merging / dropping), so the
//! split/merge churn — and what the BVH occluder culls — is visible while moving around the map.
//!
//! Deliberately *not* a second `Camera3d`: many gameplay systems query
//! `Query<..., With<Camera3d>>::single()` and a second real camera would silently break them.
//! Instead the boxes are projected manually and painted straight into the egui window.

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::collections::HashMap;

use super::map_lod::{
    PendingTileGroupFetch, PendingTileReveal, TileGroupFetchPurpose, TileShouldMerge,
    TileShouldSplit, TileSwapRequests, TreeMapTile,
};
use super::{MapLODState, MapTree, MapTreeNodePath};

/// Direction from the map center toward the virtual minimap camera. Mostly straight up with a
/// slight lateral offset so box heights read as 3D instead of collapsing into a flat plan view.
const VIEW_DIR: Vec3 = Vec3::new(0.30, 1.0, 0.30);
/// Camera distance as a multiple of the map bbox bounding-sphere radius.
const VIEW_DIST_FACTOR: f32 = 2.4;

const COLOR_ACTIVE: egui::Color32 = egui::Color32::from_rgb(0, 220, 80);
const COLOR_PENDING_REVEAL: egui::Color32 = egui::Color32::from_rgb(240, 210, 0);
const COLOR_SPLITTING: egui::Color32 = egui::Color32::from_rgb(255, 140, 0);
const COLOR_MERGING: egui::Color32 = egui::Color32::from_rgb(220, 80, 255);
const COLOR_DROPPING: egui::Color32 = egui::Color32::from_rgb(255, 60, 60);
const COLOR_MAP_EXTENT: egui::Color32 = egui::Color32::from_gray(110);
const COLOR_CAMERA: egui::Color32 = egui::Color32::WHITE;
const COLOR_CULLED: egui::Color32 = egui::Color32::from_rgb(0, 80, 220); // Dark Blue

#[derive(Clone, Copy, PartialEq)]
enum TileState {
    Active,
    PendingReveal,
    Splitting,
    Merging,
    Dropping,
}

impl TileState {
    fn color(self) -> egui::Color32 {
        match self {
            TileState::Active => COLOR_ACTIVE,
            TileState::PendingReveal => COLOR_PENDING_REVEAL,
            TileState::Splitting => COLOR_SPLITTING,
            TileState::Merging => COLOR_MERGING,
            TileState::Dropping => COLOR_DROPPING,
        }
    }
}

/// Perspective projector for the fixed high-up minimap view.
struct MiniView {
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    center_px: egui::Pos2,
    scale: f32,
}

impl MiniView {
    fn new(bbox_min: Vec3, bbox_max: Vec3, rect: egui::Rect) -> Self {
        let center = (bbox_min + bbox_max) / 2.0;
        let radius = ((bbox_max - bbox_min).length() / 2.0).max(1.0);
        let eye = center + VIEW_DIR.normalize() * radius * VIEW_DIST_FACTOR;
        let forward = (center - eye).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);

        // Fit: project the map bbox corners at unit scale, then scale to the paint rect.
        let mut view = Self {
            eye,
            right,
            up,
            forward,
            center_px: rect.center(),
            scale: 1.0,
        };
        let mut max_ext: f32 = 1e-4;
        for corner in box_corners(bbox_min, bbox_max) {
            if let Some(p) = view.project_unit(corner) {
                max_ext = max_ext.max(p.x.abs()).max(p.y.abs());
            }
        }
        view.scale = 0.46 * rect.width().min(rect.height()) / max_ext;
        view
    }

    /// Projects onto the virtual image plane at focal length 1 (before pixel scaling).
    fn project_unit(&self, p: Vec3) -> Option<Vec2> {
        let v = p - self.eye;
        let z = v.dot(self.forward);
        if z <= 1e-3 {
            return None;
        }
        Some(Vec2::new(v.dot(self.right) / z, v.dot(self.up) / z))
    }

    fn project(&self, p: Vec3) -> Option<egui::Pos2> {
        let u = self.project_unit(p)?;
        Some(egui::pos2(
            self.center_px.x + u.x * self.scale,
            self.center_px.y - u.y * self.scale,
        ))
    }

    fn line(&self, painter: &egui::Painter, a: Vec3, b: Vec3, stroke: egui::Stroke) {
        if let (Some(pa), Some(pb)) = (self.project(a), self.project(b)) {
            painter.line_segment([pa, pb], stroke);
        }
    }

    fn wire_box(&self, painter: &egui::Painter, min: Vec3, max: Vec3, color: egui::Color32) {
        let c = box_corners(min, max);
        const EDGES: [(usize, usize); 12] = [
            (0, 1),
            (1, 3),
            (3, 2),
            (2, 0),
            (4, 5),
            (5, 7),
            (7, 6),
            (6, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        let stroke = egui::Stroke::new(1.0, color);
        for (a, b) in EDGES {
            self.line(painter, c[a], c[b], stroke);
        }
    }
}

fn box_corners(min: Vec3, max: Vec3) -> [Vec3; 8] {
    [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ]
}

/// World-space AABB of a spawned tile, merged from all mesh descendants. `None` until the tile's
/// scene has finished instancing (no mesh `Aabb`s yet).
fn compute_tile_aabb(
    tile: Entity,
    q_children: &Query<&Children>,
    q_aabbs: &Query<(&GlobalTransform, &Aabb)>,
) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    let mut found = false;

    let mut stack = vec![tile];
    while let Some(ent) = stack.pop() {
        if let Ok((gt, aabb)) = q_aabbs.get(ent) {
            let center = Vec3::from(aabb.center);
            let half = Vec3::from(aabb.half_extents);
            for corner in box_corners(center - half, center + half) {
                let world = gt.transform_point(corner);
                min = min.min(world);
                max = max.max(world);
            }
            found = true;
        }
        if let Ok(children) = q_children.get(ent) {
            stack.extend(children.iter());
        }
    }

    found.then_some((min, max))
}

/// Classifies the LOD state of a spawned tile from the in-flight split/merge/reveal bookkeeping.
#[allow(clippy::too_many_arguments)]
fn classify_tile(
    path: &MapTreeNodePath,
    visibility: &Visibility,
    q_splits: &Query<&TileShouldSplit>,
    q_merges: &Query<&TileShouldMerge>,
    q_reveals: &Query<&PendingTileReveal>,
    q_fetches: &Query<&PendingTileGroupFetch>,
) -> TileState {
    for reveal in q_reveals.iter() {
        if reveal.drop_parent.as_ref() == Some(path) {
            return TileState::Dropping;
        }
        if reveal
            .drop_descendants_of
            .iter()
            .any(|d| path.0.starts_with(&d.0))
        {
            return TileState::Dropping;
        }
    }
    for split in q_splits.iter() {
        if &split.drop_parent == path {
            return TileState::Splitting;
        }
    }
    for merge in q_merges.iter() {
        if merge.drop_children.contains(path) {
            return TileState::Merging;
        }
    }
    for fetch in q_fetches.iter() {
        match &fetch.purpose {
            TileGroupFetchPurpose::Split { split_summary } => {
                if &split_summary.parent_path == path {
                    return TileState::Splitting;
                }
            }
            TileGroupFetchPurpose::Merge { drop_children, .. } => {
                if drop_children.contains(path) {
                    return TileState::Merging;
                }
            }
            TileGroupFetchPurpose::Root { .. } => {}
        }
    }
    if matches!(visibility, Visibility::Hidden) {
        return TileState::PendingReveal;
    }
    TileState::Active
}

/// Corner window with the 3D tile-bbox minimap, state legend, and the BVH-occluder toggle.
#[allow(clippy::too_many_arguments)]
pub fn bvh_minimap_window(
    mut contexts: EguiContexts,
    ui_state: Option<ResMut<crate::ui_egui::UiState>>,
    mut lod_state: ResMut<MapLODState>,
    map_tree: Res<MapTree>,
    res_tiles: Res<TileSwapRequests>,
    q_tiles: Query<(Entity, &TreeMapTile, &Visibility)>,
    q_children: Query<&Children>,
    q_aabbs: Query<(&GlobalTransform, &Aabb)>,
    q_splits: Query<&TileShouldSplit>,
    q_merges: Query<&TileShouldMerge>,
    q_reveals: Query<&PendingTileReveal>,
    q_fetches: Query<&PendingTileGroupFetch>,
    q_camera: Query<&GlobalTransform, With<Camera3d>>,
    mut aabb_cache: Local<HashMap<Entity, (Vec3, Vec3)>>,
) {
    let Some(mut state) = ui_state else {
        return;
    };
    if !state.show_bvh_minimap {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !map_tree.parsed {
        return;
    }

    // Mesh AABBs are static per tile entity; compute once and drop entries for despawned tiles.
    aabb_cache.retain(|ent, _| q_tiles.contains(*ent));

    let mut open = state.show_bvh_minimap;
    egui::Window::new("3D BVH Minimap")
        .open(&mut open)
        .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
        .default_size([360.0, 340.0])
        .show(ctx, |ui| {
            // Toggling re-runs the LOD recompute: the flag is part of `spawn_lod_task`'s
            // change-detection key in `lod_flow.rs`.
            ui.checkbox(
                &mut lod_state.enable_visibility_cull,
                "BVH occluder (visibility cull)",
            );

            // Gather tiles + states first so the legend can show live counts.
            let mut boxes: Vec<((Vec3, Vec3), TileState)> = Vec::new();
            let mut counts = [0usize; 5];
            for (entity, tile, visibility) in q_tiles.iter() {
                let aabb = match aabb_cache.get(&entity) {
                    Some(aabb) => *aabb,
                    None => match compute_tile_aabb(entity, &q_children, &q_aabbs) {
                        Some(aabb) => {
                            aabb_cache.insert(entity, aabb);
                            aabb
                        }
                        None => continue, // scene not instanced yet
                    },
                };
                let tile_state = classify_tile(
                    &tile.node_path,
                    visibility,
                    &q_splits,
                    &q_merges,
                    &q_reveals,
                    &q_fetches,
                );
                counts[tile_state as usize] += 1;
                boxes.push((aabb, tile_state));
            }

            ui.horizontal_wrapped(|ui| {
                for (label, color, count) in [
                    ("active", COLOR_ACTIVE, counts[TileState::Active as usize]),
                    (
                        "reveal",
                        COLOR_PENDING_REVEAL,
                        counts[TileState::PendingReveal as usize],
                    ),
                    (
                        "split",
                        COLOR_SPLITTING,
                        counts[TileState::Splitting as usize],
                    ),
                    ("merge", COLOR_MERGING, counts[TileState::Merging as usize]),
                    ("drop", COLOR_DROPPING, counts[TileState::Dropping as usize]),
                    ("culled", COLOR_CULLED, res_tiles.culled_nodes.len()),
                ] {
                    ui.colored_label(color, format!("■ {label} {count}"));
                }
            });

            let size = ui.available_size();
            let size = egui::vec2(size.x.max(280.0), size.y.max(220.0));
            let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
            let rect = response.rect;
            painter.rect_filled(rect, 4.0, egui::Color32::from_black_alpha(230));

            let view = MiniView::new(map_tree.bbox.min, map_tree.bbox.max, rect);

            // Full map extent as the reference frame.
            view.wire_box(
                &painter,
                map_tree.bbox.min,
                map_tree.bbox.max,
                COLOR_MAP_EXTENT,
            );

            for ((min, max), tile_state) in &boxes {
                view.wire_box(&painter, *min, *max, tile_state.color());
            }

            for culled in &res_tiles.culled_nodes {
                view.wire_box(&painter, culled.bbox.min, culled.bbox.max, COLOR_CULLED);
            }

            // Main camera marker: position dot + flattened view direction tick.
            if let Some(cam) = q_camera.iter().next() {
                let pos = cam.translation();
                if let Some(p) = view.project(pos) {
                    painter.circle_filled(p, 3.0, COLOR_CAMERA);
                }
                let mut fwd = cam.forward().as_vec3();
                fwd.y = 0.0;
                let fwd = fwd.normalize_or_zero();
                if fwd != Vec3::ZERO {
                    let reach = (map_tree.bbox.max - map_tree.bbox.min).length() * 0.05;
                    view.line(
                        &painter,
                        pos,
                        pos + fwd * reach,
                        egui::Stroke::new(1.5, COLOR_CAMERA),
                    );
                }
            }
        });
    state.show_bvh_minimap = open;
}
