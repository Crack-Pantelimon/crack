//! Pedestrian V2 viewer — a thin driver around [`PedestriansPlugin`].
//!
//! This binary owns only viewer concerns: the scene (floor/camera/lights), spawning a grid of
//! every pedestrian in the manifest, mouse picking + camera focus, and a single egui control
//! window. All reusable pedestrian logic lives in `plugins::pedestrians`.

use avian3d::prelude::{PhysicsPlugins, SpatialQuery, SpatialQueryFilter};
use bevy::{ecs::relationship::Relationship, prelude::*};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        game_freecam::camera_controls::{ActiveCameraAnimation, CameraControlsPlugin},
        map_plugin::{BBox, MapTree},
        pedestrians::{
            ModelRoot, PedestrianAnimationControlEvent, PedestrianAnimations, PedestrianManifest,
            PedestriansPlugin, SkeletonDebug, SpawnPedestrianEvent,
        },
        states::GameControlState,
    },
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

#[derive(Resource, Default)]
struct SelectedModel {
    entity: Option<Entity>,
}

#[derive(Resource, Default)]
struct HoveredModel {
    entity: Option<Entity>,
}

/// Viewer-side animation selection, mirrored out to every pedestrian on change.
#[derive(Resource)]
struct ViewerAnimSelection {
    selected: Option<String>,
    speed: f32,
}

impl Default for ViewerAnimSelection {
    fn default() -> Self {
        Self {
            selected: None,
            speed: 1.0,
        }
    }
}

fn main() {
    make_basic_app("Pedestrian V2 - list")
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .init_state::<GameControlState>()
        .insert_resource(MapTree {
            parsed: true,
            bbox: BBox {
                min: Vec3::new(-1000.0, -100.0, -1000.0),
                max: Vec3::new(1000.0, 100.0, 1000.0),
            },
            ..default()
        })
        .add_plugins(CameraControlsPlugin)
        .add_plugins(PedestriansPlugin)
        .init_resource::<SelectedModel>()
        .init_resource::<HoveredModel>()
        .init_resource::<ViewerAnimSelection>()
        .add_plugins(SetupDebugScenePlugin)
        .add_systems(
            Update,
            (spawn_grid_system, picker_system, draw_hovered_bbox_system),
        )
        .add_systems(EguiPrimaryContextPass, draw_gui_system)
        .run();
}

/// Once the manifest is loaded, spawn every pedestrian in a square grid (runs once).
fn spawn_grid_system(
    mut commands: Commands,
    manifest: Res<PedestrianManifest>,
    mut spawned: Local<bool>,
) {
    if *spawned || !manifest.loaded {
        return;
    }

    let count = manifest.urls.len();
    if count == 0 {
        *spawned = true;
        return;
    }
    let cols = (count as f32).sqrt().ceil() as usize;

    for (idx, url) in manifest.urls.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;

        const GRID_SIZE: f32 = 1.6;
        let x = (col as f32 - (cols - 1) as f32 / 2.0) * GRID_SIZE;
        let z = (row as f32 - (((count as f32 / cols as f32).ceil() - 1.0) / 2.0)) * GRID_SIZE;
        let y = 0.0;

        let dummy_parent = commands.spawn(
            Transform::from_translation(Vec3::new(x, y, z))
        ).id();
        commands.trigger(SpawnPedestrianEvent {
            url: url.clone(),
            position: Vec3::new(x, y, z),
            controller: dummy_parent,
            parent: dummy_parent,
        });
    }

    *spawned = true;
}

fn picker_system(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: SpatialQuery,
    parent_query: Query<&ChildOf>,
    model_root_query: Query<(Entity, &ModelRoot, &GlobalTransform)>,
    mut hovered: ResMut<HoveredModel>,
    mut selected: ResMut<SelectedModel>,
    mut contexts: EguiContexts,
) {
    let egui_focused = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
    } else {
        false
    };
    if egui_focused {
        hovered.entity = None;
        return;
    }

    let Some(window) = windows.iter().next() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        hovered.entity = None;
        return;
    };
    let Some((camera, camera_transform)) = camera_query.iter().next() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let ray_dir = ray.direction;

    hovered.entity = None;

    if let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray_dir,
        1000.0,
        true,
        &SpatialQueryFilter::default(),
    ) {
        let mut current = hit.entity;
        let mut found_root = None;
        loop {
            if let Ok((root_ent, root, _)) = model_root_query.get(current) {
                found_root = Some((root_ent, root.index));
                break;
            }
            if let Ok(parent) = parent_query.get(current) {
                current = parent.get();
            } else {
                break;
            }
        }

        if let Some((root_ent, model_idx)) = found_root {
            hovered.entity = Some(root_ent);

            if mouse_button.just_pressed(MouseButton::Left) {
                selected.entity = Some(root_ent);
                info!("Selected model: {} (entity: {:?})", model_idx, root_ent);

                if let Ok((_, root, root_gt)) = model_root_query.get(root_ent) {
                    let model_pos = root_gt.translation();
                    let head_height = root.size.y;

                    let start_pos = camera_transform.translation();
                    let start_rot = camera_transform.rotation();

                    // Camera position in front of pedestrian (facing away towards -Z means front is at -Z)
                    let target_pos = model_pos + Vec3::new(0.0, head_height / 2.0 + 0.3, -1.8);

                    // Look back at the pedestrian's upper chest / face
                    let look_target = model_pos + Vec3::new(0.0, head_height / 4.0, 0.0);
                    let target_rot = Transform::from_translation(target_pos)
                        .looking_at(look_target, Vec3::Y)
                        .rotation;

                    commands.insert_resource(ActiveCameraAnimation {
                        start_pos,
                        start_rot,
                        target_pos,
                        target_rot,
                        elapsed: 0.0,
                        duration: 0.8,
                    });
                }
            }
        }
    }
}

fn draw_hovered_bbox_system(
    mut gizmos: Gizmos,
    hovered: Res<HoveredModel>,
    model_root_query: Query<(&GlobalTransform, &ModelRoot)>,
) {
    if let Some(hovered_ent) = hovered.entity {
        if let Ok((gt, root)) = model_root_query.get(hovered_ent) {
            let center = gt.translation();
            let size = root.size;
            let cuboid = Cuboid::new(size.x, size.y, size.z);
            gizmos.primitive_3d(
                &cuboid,
                Isometry3d::from_translation(center),
                Color::srgb(1.0, 1.0, 0.0),
            );
        }
    }
}

fn draw_gui_system(
    mut commands: Commands,
    mut contexts: EguiContexts,
    selected: Res<SelectedModel>,
    model_roots: Query<(Entity, &ModelRoot)>,
    anims: Res<PedestrianAnimations>,
    mut skeleton_debug: ResMut<SkeletonDebug>,
    mut anim_sel: ResMut<ViewerAnimSelection>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Default the selection to a sensible clip once the catalog is ready.
    if anim_sel.selected.is_none() {
        anim_sel.selected = anims.default_animation();
    }

    let mut anim_changed = false;

    egui::Window::new("Pedestrian V2")
        .default_pos(egui::pos2(12.0, 50.0))
        .default_size(egui::vec2(250.0, 320.0))
        .show(ctx, |ui| {
            ui.checkbox(&mut skeleton_debug.show, "Show Skeleton Graph");

            ui.separator();
            if ui
                .add(egui::Slider::new(&mut anim_sel.speed, 0.3..=3.0).text("Speed"))
                .changed()
            {
                anim_changed = true;
            }

            ui.separator();
            ui.label("Select Animation:");

            let anim_names: Vec<String> = anims.catalog.keys().cloned().collect();
            let current = anim_sel.selected.clone();
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for name in &anim_names {
                        if ui.radio(current.as_ref() == Some(name), name).clicked() {
                            anim_sel.selected = Some(name.clone());
                            anim_changed = true;
                        }
                    }
                });

            ui.separator();
            if let Some(selected_ent) = selected.entity {
                if let Ok((_, root)) = model_roots.get(selected_ent) {
                    ui.heading("Selected Pedestrian:");
                    ui.label(format!("Index: {}", root.index));
                    ui.label(format!("Name: {}", root.name));
                    ui.label(format!(
                        "Size: {:.2} x {:.2} x {:.2}",
                        root.size.x, root.size.y, root.size.z
                    ));
                }
            } else {
                ui.label("No pedestrian selected");
            }
        });

    // Mirror the selection out to every pedestrian when it changes.
    if anim_changed {
        if let Some(animation) = anim_sel.selected.clone() {
            let speed = anim_sel.speed;
            for (ped, _) in model_roots.iter() {
                commands.trigger(PedestrianAnimationControlEvent {
                    ped,
                    animation: animation.clone(),
                    speed,
                });
            }
        }
    }
}
