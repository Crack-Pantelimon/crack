
image.png
1360×833
we don't see the cloud vfx on the main game map, probably because it is not scaled properly in the space to come some fixed distance over the map max y .

we also don't see proper rotation of the player controlled body - we have attempted maybe in the past some realignment of the spine but that didn't work out - let's just make sure the character is always facing the direction of shooting and aiming, regardless of what the feet are doing. we would like to see a decoupling at the spine/hip of the aiming / hitting side (arms and chest and head ) and the running side (feet and hips) but at this point let's just snap the mesh in front and leave that as todo. see picture for current behavior 

when in car and scrolling through weapons using mouse wheel, we shouldn't show or cycle through any non-gun weapon e.g. unarmed/melee.

we don't understand if/how the bvh occluder works or not properly. let's have the possibility to display (when clicking Debug > 3d bvh minimap) a window with a 3d minimap in the corner of the screen with only a minimap of the tiles being loaded. use different colors for different tile states. this minimap will only show the cubic bboxes of the loaded tiles and the view is captured from a camera very high up to capture all the boxes. when we move about the map, we should see the lod configurator. add the bvh occluder optional default on as a checkbox, and when it is switched, we will re-reun the lod recompute logic again. 

fix all of the above in the code immediately, then write a review of the code analysed and future improvement steps in _slop/improvement-v1.md and stop.
This is a big multi-part task: cloud VFX placement, character aim-facing, weapon cycling in cars, a new 3D BVH minimap debug window, then a review doc. Let me start by querying sigmap per the project instructions, and looking at the relevant files.

PowerShell Query sigmap for relevant code locations
IN
sigmap ask "where are clouds spawned and what height; where is player character rotation set from aiming; where is weapon cycling with mouse wheel handled; where is lod recompute triggered and bvh occluder used"

OUT
[sigmap] using project root: C:/Users/Naxxramas/Desktop/TETROS/crack
────────────────────────────────────────────
 sigmap ask  "where are clouds spawned and what height; where is player character rotation set from aiming; where is weapon cycling with mouse wheel handled; where is lod recompute triggered and bvh occluder used"
 Intent    : navigate
 Context   : 896 tokens  →  .context\query-context.md
 Coverage  : 100%
 Risk      : LOW
 Cost      : $0.0045/query  (was $0.0801 · saved 94%)
────────────────────────────────────────────
Read c:\Users\Naxxramas\Desktop\TETROS\crack\.context\query-context.md
The sigmap context is thin; let me read the actual source files.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\clouds.rs
Globpattern: "crack_demo/demo_resolution_selector_web_bevy/src/plugins/**/*.rs"
Found 79 files
Now let me read the key files for each task.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\controller.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\mod.rs
face_aim already exists but apparently doesn't produce correct results. Let me look at the controller plugin's mod, camera rig, and the weapons plugin.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\mod.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\weapons\mod.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\locomotion.rs
TodoWrite is available now. Let me set up the plan and keep reading.

Update Todos

Fix cloud plane height/position so clouds show over the main map (scale to map max y, follow camera)

Snap player body to face aim/shoot direction (leave spine decoupling as TODO)

Skip non-gun weapons when scrolling weapons in car

Add Debug > 3D BVH minimap window (tile bboxes, colored by state, top-down camera) with bvh-occluder checkbox that re-runs LOD recompute

Write review + future improvements in _slop/improvement-v1.md
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\camera.rs
Grep "weapon_wheel|WeaponSelection|MouseWheel|scroll" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\interaction_ui.rs)
10 lines of output
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\interaction_ui.rs (lines 851-990)
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\interaction_ui.rs (lines 1151-1280)
Now the map/LOD side. Let me read the map plugin and LOD flow.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\mod.rs
MapLODState already has enable_visibility_cull. Now the LOD flow and where the occluder is used.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\crack_plugin\lod_flow.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\map_plugin_ui.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\ui_egui.rs
Now let me read the tile LOD implementation to understand tile states for the minimap.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\map_lod.rs
The minimap needs per-tile bboxes. Let me check how node paths map to bboxes in game_logic.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\game_logic\src\map.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\game_logic\src\visibility.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\crack_plugin\manifest_flow.rs
Let me check how fake_map and geo derive bboxes from octant paths — that may give me a client-side path→bbox mapping for the minimap.

Grep "octant|bbox" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\fake_map.rs)
24 lines of output
Now let me look at the arm IK and where firing is triggered, to understand the aiming path.

Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\arm_ik.rs
Grep "from_rotation_y|forward|Vec3::Z|NEG_Z" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\spawn.rs)
1 line of output
Grep "from_rotation_y|Vec3::Z|NEG_Z|forward" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\spawn_pedestrian.rs)
1 line of output
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\settings.rs
Read c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\weapons\weapon_manifest.rs
I have the full picture now. Let me check the Bevy version and existing RenderLayers/viewport usage patterns before writing the minimap.

Grep "RenderLayers|Viewport" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo)
1 line of output
Grep "^bevy|^\[dependencies|bevy = |bevy_egui" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\Cargo.toml)
4 lines of output
Bevy 0.19. Let me check how this repo imports Aabb/camera types.

Grep "primitives::Aabb|use bevy::camera|render::view|ClearColorConfig|GizmoConfig" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src)
9 lines of output
Grep "Camera3d|Msaa|Camera \{|Tonemapping" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\main_scene_plugin.rs)
41 lines of output
One critical constraint: many systems query With<Camera3d> and use .single() — adding a second 3D camera for the minimap would break them. Let me check the extent.

Grep "With<Camera3d>|With<Camera>|&Camera[,)]" (in c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src)
39 lines of output
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\controller.rs
Added 13 lines
Now the arm-IK forward convention fix (this was the twisted-spine culprit):

Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\arm_ik.rs
Added 3 lines
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\arm_ik.rs
Added 1 line
Update Todos

Fix cloud plane height/position so clouds show over the main map (scale to map max y, follow camera)

Snap player body to face aim/shoot direction (leave spine decoupling as TODO)

Skip non-gun weapons when scrolling weapons in car

Add Debug > 3D BVH minimap window (tile bboxes, colored by state, top-down camera) with bvh-occluder checkbox that re-runs LOD recompute

Write review + future improvements in _slop/improvement-v1.md
Now the clouds fix — reposition/rescale the plane once the map manifest is parsed:

Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\clouds.rs
Added 10 lines
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\clouds.rs
Modified
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\clouds.rs
Added 34 lines
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\mod.rs
Modified
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\visual_fx\mod.rs
Added 1 line
Now the car weapon wheel — cycle guns only:

Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\pedestrians\pedestrian_controller_plugin\interaction_ui.rs
Added 24 lines
Update Todos

Fix cloud plane height/position so clouds show over the main map (scale to map max y, follow camera)

Snap player body to face aim/shoot direction (leave spine decoupling as TODO)

Skip non-gun weapons when scrolling weapons in car

Add Debug > 3D BVH minimap window (tile bboxes, colored by state, top-down camera) with bvh-occluder checkbox that re-runs LOD recompute

Write review + future improvements in _slop/improvement-v1.md
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\crack_plugin\lod_flow.rs
Modified
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\crack_plugin\lod_flow.rs
Added 7 lines
Now make PendingTileReveal fields readable for the minimap, then create the minimap module:

Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\map_lod.rs
Modified
Write c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\bvh_minimap.rs
369 lines
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
    TileShouldSplit, TreeMapTile,
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
    if visibility == Visibility::Hidden {
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
            view.wire_box(&painter, map_tree.bbox.min, map_tree.bbox.max, COLOR_MAP_EXTENT);

            for ((min, max), tile_state) in &boxes {
                view.wire_box(&painter, *min, *max, tile_state.color());
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
Fix a type issue in the visibility comparison:

Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\bvh_minimap.rs
Modified
Register the module and system, then wire the UiState flag and Debug menu entry:

Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\mod.rs
Added 1 line
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\plugins\map_plugin\mod.rs
Added 3 lines
Now the UiState field and Debug menu entry:

Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\ui_egui.rs
Added 1 line
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\ui_egui.rs
Added 1 line
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\ui_egui.rs
Added 1 line
Edit c:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy\src\ui_egui.rs
Added 9 lines
All edits are in. Now compile-check:

PowerShell Type-check the game crate
IN
cargo check -p demo_resolution_selector_web_bevy 2>&1 | Select-Object -Last 60

OUT
cargo :     Checking game_logic v0.1.0 (C:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\game_logic)
At line:1 char:1
+ cargo check -p demo_resolution_selector_web_bevy 2>&1 | Select-Object ...
+ ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    + CategoryInfo          : NotSpecified: (    Checking ga...emo\game_logic):String) [], RemoteException
    + FullyQualifiedErrorId : NativeCommandError
 
    Checking thread_worker v0.1.0 (C:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\thread_worker)
    Checking demo_resolution_selector_web_bevy v0.1.0 
(C:\Users\Naxxramas\Desktop\TETROS\crack\crack_demo\demo_resolution_selector_web_bevy)
    Finished `dev` profile [optimized] target(s) in 13.30s
Compiles clean. Now the review document:

Bash Check for _slop directory
IN
ls "c:/Users/Naxxramas/Desktop/TETROS/crack/_slop" 2>/dev/null || echo "no _slop dir"

OUT
bugs.md
examples
map.png
plan_lod_tree_prompt.md
plan_lod_tree_v1.md
plan_lod_tree_v2.md
plan_lod_tree_v3.md
plan_vfx_prompt.md
plan_vfx_shader_v1.md
plan_vfx_shader_v2.md
Write c:\Users\Naxxramas\Desktop\TETROS\crack\_slop\improvement-v1.md
140 lines
# Improvement Review v1

Review of the code analysed while fixing the July batch (clouds, aim-facing, car weapon
wheel, BVH minimap), plus future improvement steps. Written 2026-07-10.

## What was changed in this batch

### 1. Cloud VFX not visible on the main map
`setup_clouds` spawned the 10 km cloud quad at a hard-coded world `y = 120` at `Startup`,
before the map manifest exists. The real map bbox sits at whatever elevation the ENU
projection produces, so on the main map the plane ended up below the terrain (or hopelessly
off-center) and was never seen. New `position_clouds_over_map` system
(`visual_fx/clouds.rs`) re-anchors the plane when `MapTree` parses: centered on the map
bbox, `CLOUD_HEIGHT_ABOVE_MAP` (150 m) above the bbox *top*, and scales the quad to twice
the map extent. Demo/sim binaries without `MapTree` keep the old placement.

### 2. Player body not facing the aim/shoot direction
Two compounding bugs:

- `face_aim` (controller.rs) only ran while RMB was held and *slerped* the yaw at
  `TURN_SPEED`, so un-aimed LMB shots never squared the body and aimed shots lagged the
  crosshair. It now runs while aiming **or** while a combat overlay is active
  (`CombatState.kind != None`) and **snaps** the yaw (no slerp).
- `apply_arm_ik` (arm_ik.rs) used `rotation * Vec3::NEG_Z` as character forward, while the
  controller convention everywhere else (`face_movement`, `face_aim`) is model-forward =
  `+Z`. The spine "compensation" (`torso_yaw_toward`) therefore saw the target ~180° behind
  the character, computed a huge excess yaw, and twisted the torso sideways — the past
  "spine realignment that didn't work out", and the sideways pose in the screenshot. Both
  branches (on foot, driving) now use `+Z`.

Left as `TODO` (documented on `face_aim`): true upper/lower body decoupling — aim side
(chest/head/arms) tracks the crosshair, locomotion side (hips/legs) follows movement. For
now the whole mesh snaps.

### 3. Car weapon wheel cycles through non-guns
`driving_weapon_wheel` (interaction_ui.rs) indexed the full `WeaponManifest.all` (unarmed,
melee, guns). It now cycles through gun entries only; if the driver currently has a
non-gun equipped, the first scroll snaps to the first/last gun depending on direction. If
the manifest has no guns, scrolling does nothing.

### 4. BVH occluder observability — Debug > 3D BVH Minimap
New `map_plugin/bvh_minimap.rs`: an egui corner window with a 3D wireframe view of every
spawned map tile's cubic bbox, projected from a virtual camera high above the map (whole
bbox always in frame). Colors encode LOD state:

| Color  | State |
|--------|-------|
| green  | active (visible) |
| yellow | loaded, pending reveal |
| orange | split in flight (children fetching/loading) |
| purple | merge in flight (parent fetching/loading) |
| red    | about to be dropped by a pending reveal |

The main camera is drawn as a white dot with a heading tick. A **"BVH occluder
(visibility cull)"** checkbox (default on — set from the manifest in
`poll_manifest_task`) toggles `MapLODState.enable_visibility_cull`; the flag is now part
of `spawn_lod_task`'s change-detection key (`lod_flow.rs`), so flipping it forces a fresh
LOD recompute immediately. Opening the minimap also opens the LOD configurator so the
split/merge churn can be watched while moving.

**Deliberate design choice:** the minimap is *not* a second `Camera3d` with a viewport.
~15 gameplay systems do `Query<..., With<Camera3d>>::single()` (follow camera, character
input, weapon transforms, LOD flow, traffic…); a second real camera makes every one of
them silently bail. The boxes are hand-projected and painted with the egui painter
instead — zero render-graph interaction.

## Review of the code analysed

### Occluder / LOD pipeline (`game_logic/visibility.rs`, `lod.rs`, `lod_flow.rs`)
- The occluder world is rebuilt worker-side from 64x64 heightfields per tile. Reasonable,
  but there is **no client-side signal of what got culled** — the client only sees fewer
  split requests. The minimap shows spawned-tile states, but a culled *candidate* never
  appears at all. See improvement 1 below.
- `is_node_visible` casts up to `cameras × 16 samples × 9 corners` rays per node. The
  Fibonacci-sphere "camera drift" sampling is clever, but ray count grows quickly with
  the number of camera references; there is no time budget or early-out ordering
  (e.g. test the center point first — it does test corners in fixed order).
- `spawn_lod_task`'s change key (nodes, quantized refs, budget, cull flag) omits
  `max_lod` and `tiles_per_diagonal` — moving those sliders only takes effect after the
  camera moves 2 m. Same class of bug as the cull-flag one just fixed.
- `HEIGHTMAP_CACHE` is never evicted; on a long session over a big map this grows
  unboundedly (64x64 f32 ≈ 16 KB per node, plus parry structures).

### Map tile flow (`map_lod.rs`)
- The split/merge machinery uses five entity-component "queues" (`PendingTileGroupFetch`,
  `TileShouldSplit`, `TileShouldMerge`, `PendingTileReveal`, plus `TileSwapRequests`).
  It works, but state is smeared across components and the LOD task refuses to run while
  *any* of them is non-empty — one slow tile fetch stalls all LOD progress globally.
- Tile world AABBs are not stored on `TreeMapTile`; the minimap has to reconstruct them
  from mesh `Aabb`s. The server knows the exact node bbox (`MapTreeNodeInfo.bbox`) —
  shipping it with the split/merge summaries (or in the root manifest) would remove the
  reconstruction and let the minimap draw *fetching* tiles before their meshes exist.

### Character controller / combat (`pedestrian_controller_plugin`)
- Forward-axis convention (+Z) is implicit and was already violated once (arm_ik). Worth
  a single `pub const MODEL_FORWARD: Vec3` (or `Dir3`) next to `MODEL_FORWARD_OFFSET`
  that every consumer uses.
- `face_movement` and `face_aim` both run in `Update` while `move_and_slide` runs in
  `FixedUpdate`; rotation is written on the physics body outside the physics step. Fine
  for a kinematic capsule, but worth revisiting if rotation ever affects collision.
- `interaction_ui.rs` is ~1400 lines mixing car interaction, weapon wheels, HUDs,
  crosshairs, and driver-mesh lifecycle. The on-foot and driving weapon wheels are
  near-duplicates (reader loop, debounce, over-UI check) — extract a
  `read_scroll_step(&mut wheel, &mut contexts) -> i32` helper and a shared cycle fn.

### Clouds / VFX
- `sync_cloud_uniforms` + `position_clouds_over_map` both react to change detection;
  clouds are still a single scrolling-noise quad. Good enough, but the quad is scaled up
  to 2× map extent while the noise frequency (`cloud_scale`) is world-space, so nothing
  changes visually with scale — correct, just non-obvious (documented by constants now).
- `VfxSettings` is one flat struct consumed by everything; per-category sub-structs would
  cut the `is_changed()` fan-out.

## Future improvement steps (prioritized)

1. **Culled-tile feedback in the minimap.** Return the set of nodes rejected by the
   visibility gate in `LodComputeResponse` (paths + bboxes) and draw them dark-blue in
   the minimap. That directly answers "is the BVH occluder working" — today it can only
   be inferred from split behavior.
2. **Add `max_lod` / `tiles_per_diagonal` to the LOD change key** so slider changes
   recompute immediately (one-line each, same pattern as the cull flag).
3. **Spine/hip decoupling** (the TODO): drive a chest/spine yaw offset post-animation
   (same slot as `apply_arm_ik`, which already rotates the spine bone) from the delta
   between aim yaw and movement yaw, clamped to ±60°, instead of snapping the whole
   controller. The `PedestrianSkeleton` classification already exposes the spine chain.
4. **Bound the occluder cost:** cap total rays per `compute_lod_changes` call and evict
   `HEIGHTMAP_CACHE` entries for nodes absent from the manifest-reachable set (or LRU).
5. **Ship node bboxes to the client** with split/merge summaries; store on
   `TreeMapTile`. Simplifies the minimap, enables client-side frustum debug, and removes
   the per-entity AABB reconstruction cache.
6. **Unstall the LOD loop:** allow `spawn_lod_task` to run when queues are non-empty but
   stale (e.g. per-request timeout), so one hung fetch doesn't freeze LOD adaptation.
7. **Weapon wheel unification:** shared gun-filtering + scroll-step helper for the
   on-foot and driving wheels; consider also filtering the on-foot wheel HUD order so
   guns group together.
8. **Camera identity:** introduce a `MainCamera` marker component and migrate the ~15
   `With<Camera3d>` `single()` queries to it. That unlocks real picture-in-picture
   cameras (a true rendered minimap, kill-cam, mirrors) without breaking gameplay
   systems.
Update Todos

Fix cloud plane height/position so clouds show over the main map (scale to map max y, follow camera)

Snap player body to face aim/shoot direction (leave spine decoupling as TODO)

Skip non-gun weapons when scrolling weapons in car

Add Debug > 3D BVH minimap window (tile bboxes, colored by state, top-down camera) with bvh-occluder checkbox that re-runs LOD recompute

Write review + future improvements in _slop/improvement-v1.md
All four fixes are in, the crate compiles clean, and the review is written to _slop/improvement-v1.md.

Clouds — the plane was spawned at a hard-coded world y = 120 at startup, which sits below the terrain on the real map (the ENU-projected bbox lives at real elevations; even the spawn camera goes to bbox_center.y + 100). New position_clouds_over_map system in clouds.rs re-anchors the quad when the manifest parses: map-centered, 150 m above the bbox top, scaled to 2× the map extent.

Aim facing — two compounding bugs, both fixed:

face_aim only ran while RMB was held and slerped slowly. It now runs while aiming or while a combat overlay is playing (so un-aimed LMB shots square up too) and snaps the yaw. The spine/hip decoupling you want eventually is documented as a TODO on the function.
The likely cause of your screenshot: arm_ik.rs used NEG_Z as character forward while the controller convention is +Z — the spine "compensation" saw the target ~180° behind and twisted the torso sideways. That's the past spine-realignment attempt that never worked. Both branches now use +Z.
Car weapon scroll — driving_weapon_wheel now cycles gun entries only; if you enter the car holding melee/unarmed, the first scroll snaps to a gun.

3D BVH minimap — new Debug > "3D BVH Minimap" menu item opens a corner window (bvh_minimap.rs) drawing every spawned tile's cubic bbox from a virtual camera high above the map, colored by state: green = active, yellow = pending reveal, orange = splitting, purple = merging, red = about to drop, plus a white dot/heading for your camera and live per-state counts. It includes the "BVH occluder" checkbox (default on, set from the manifest**)**, and toggling it now forces an immediate LOD recompute — the flag was added to spawn_lod_task's change-detection key in lod_flow.rs, which previously ignored it. Opening the minimap also opens the LOD configurator so you can watch the churn while moving. One deliberate choice: it's egui-painted rather than a second Camera3d, because ~15 gameplay systems do .single() on With<Camera3d> queries and a second real camera would silently break them all (that migration is listed as improvement #8 in the doc).

The review doc also flags a few things found along the way: max_lod/tiles_per_diagonal slider changes don't trigger recompute (same bug class as the cull flag), the worker's HEIGHTMAP_CACHE never evicts, one slow tile fetch stalls all LOD progress, and culled tiles are invisible to the client — returning them in LodComputeResponse and drawing them in the minimap is the top suggested next step for verifying the occluder end-to-end