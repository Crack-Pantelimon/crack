//! Pedestrian V2 viewer — a thin driver around [`PedestriansPlugin`].
//!
//! This binary owns only viewer concerns: the scene (floor/camera/lights), spawning a grid of
//! every pedestrian in the manifest, mouse picking + camera focus, and a single egui control
//! window. All reusable pedestrian logic lives in `plugins::pedestrians`.

use avian3d::prelude::{
    Collider, CollisionLayers, LinearVelocity, PhysicsDebugPlugin, PhysicsPlugins, Restitution,
    RigidBody, SpatialQuery, SpatialQueryFilter, SubstepCount,
};
use bevy::{
    asset::RenderAssetUsages,
    ecs::relationship::Relationship,
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        settings::{Backends, WgpuSettings},
        view::window::screenshot::{Screenshot, save_to_disk},
    },
    window::WindowResolution,
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use demo_resolution_selector_web_bevy::plugins::{
    cars_driving::driving_plugin::GamePhysicsLayer,
    game_freecam::camera_controls::{ActiveCameraAnimation, CameraControlsPlugin},
    map_plugin::{BBox, MapTree},
    pedestrians::{
        ModelRoot, PedestrianAnimationControlEvent, PedestrianAnimations, PedestrianManifest,
        PedestriansPlugin, RAGDOLL_ANIMATION, RagdollReady, SkeletonDebug, SpawnPedestrianEvent,
        ragdoll::RagdollBoneConstraint,
    },
    states::GameControlState,
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

/// Automated verification harness (enabled via `RAGDOLL_VERIFY=1`): once every pedestrian is
/// ragdoll-ready, flips them all into ragdoll, screenshots at +0.1/+1.0/+3.0s (logging pose +
/// velocity of the first 10 models), and exits at +4.0s.
#[derive(Resource)]
struct RagdollVerify {
    enabled: bool,
    triggered_at: Option<f32>,
    shots_done: [bool; 3],
    exited: bool,
    last_ready: usize,
    last_change_at: f32,
    last_log_at: f32,
}

impl Default for RagdollVerify {
    fn default() -> Self {
        Self {
            enabled: std::env::var("RAGDOLL_VERIFY").is_ok(),
            triggered_at: None,
            shots_done: [false; 3],
            exited: false,
            last_ready: 0,
            last_change_at: 0.0,
            last_log_at: 0.0,
        }
    }
}

const RAGDOLL_SHOT_TIMES: [f32; 3] = [0.1, 1.0, 3.0];
const RAGDOLL_VERIFY_EXIT: f32 = 4.0;

fn main() {
    #[cfg(feature = "web")]
    let backends = Backends::GL;
    #[cfg(not(feature = "web"))]
    let backends = Backends::PRIMARY;

    App::new()
        .add_plugins(
            DefaultPlugins
                .build()
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Pedestrian V2 Viewer".into(),
                        resolution: WindowResolution::new(1280, 720),
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: bevy::render::settings::RenderCreation::Automatic(Box::new(
                        WgpuSettings {
                            backends: Some(backends),
                            ..default()
                        },
                    )),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .add_plugins((PhysicsPlugins::default(),             avian3d::diagnostics::PhysicsDiagnosticsPlugin,
            // Add the `PhysicsDiagnosticsUiPlugin` to display physics diagnostics
            // in a debug UI. Requires the `diagnostic_ui` feature.
            avian3d::diagnostics::ui::PhysicsDiagnosticsUiPlugin,))
        
        .add_plugins(PhysicsDebugPlugin::default())
        // Substeps: 1 is enough for a stable ragdoll here — the per-bone damping, hard speed caps,
        // and slightly-compliant joints carry the stability instead of brute-force substepping.
        // Overridable via SUBSTEPS env for tuning.
        .insert_resource(SubstepCount(
            std::env::var("SUBSTEPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
        ))
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
        .init_resource::<RagdollVerify>()
        
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (
                spawn_grid_system,
                picker_system,
                draw_hovered_bbox_system,
                ragdoll_verify_system,
            ),
        )
        .add_systems(EguiPrimaryContextPass, draw_gui_system)
        .run();
}

fn create_grayscale_texture(gray1: u8, gray2: u8) -> Image {
    let mut texture_data = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let color = if (x / 4 + y / 4) % 2 == 0 {
                gray1
            } else {
                gray2
            };
            let offset = (y * 32 + x) * 4;
            texture_data[offset] = color;
            texture_data[offset + 1] = color;
            texture_data[offset + 2] = color;
            texture_data[offset + 3] = 255;
        }
    }
    let mut image = Image::new_fill(
        Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &texture_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        ..default()
    });
    image
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut gizmo_store: ResMut<GizmoConfigStore>,
) {
    let (config, _) = gizmo_store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;

    let cubes_info = [
        (Vec3::new(250.0, -250.0, 250.0), (50, 70)),
        (Vec3::new(-250.0, -250.0, 250.0), (90, 110)),
        (Vec3::new(250.0, -250.0, -250.0), (130, 150)),
        (Vec3::new(-250.0, -250.0, -250.0), (170, 190)),
    ];

    for (center, (gray1, gray2)) in cubes_info {
        let tile_repeat: f32 = 1.0 + rand::random::<f32>() * 2.0;

        let mut mesh = Mesh::from(Cuboid::from_size(Vec3::new(500.0, 500.0, 500.0)));
        let repeat = 500.0 / tile_repeat;
        if let Some(bevy::render::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
        {
            for uv in uvs.iter_mut() {
                uv[0] *= repeat;
                uv[1] *= repeat;
            }
        }
        let mesh_handle = meshes.add(mesh);

        let texture = create_grayscale_texture(gray1, gray2);
        let texture_handle = images.add(texture);

        let material_handle = materials.add(StandardMaterial {
            base_color_texture: Some(texture_handle),
            perceptual_roughness: 0.9,
            ..default()
        });

        commands.spawn((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            Transform::from_translation(center),
            RigidBody::Static,
            Collider::cuboid(500.0, 500.0, 500.0),
            Restitution::ZERO.with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
            CollisionLayers::new(
                [GamePhysicsLayer::Map],
                [
                    GamePhysicsLayer::Map,
                    GamePhysicsLayer::Car,
                    GamePhysicsLayer::Wheel,
                    // Ragdoll spheres + tubes must land on the ground.
                    GamePhysicsLayer::Bone1,
                    GamePhysicsLayer::Bone2,
                ],
            ),
        ));
    }

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-10.0, 2.0, -15.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        AmbientLight {
            color: Color::srgb(0.8, 0.85, 1.0),
            brightness: 1000.0,
            ..default()
        },
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(200.0, 400.0, 200.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
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

    for (idx, url) in manifest.urls.iter().take(20).enumerate() {
        let col = idx % cols;
        let row = idx / cols;

        const GRID_SIZE: f32 = 1.6;
        let x = (col as f32 - (cols - 1) as f32 / 2.0) * GRID_SIZE;
        let z = (row as f32 - (((count as f32 / cols as f32).ceil() - 1.0) / 2.0)) * GRID_SIZE;
        let y = 0.0;

        commands.trigger(SpawnPedestrianEvent {
            url: url.clone(),
            position: Vec3::new(x, y, z),
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

#[allow(clippy::too_many_arguments)]
fn ragdoll_verify_system(
    time: Res<Time>,
    mut verify: ResMut<RagdollVerify>,
    manifest: Res<PedestrianManifest>,
    mut commands: Commands,
    models: Query<(Entity, &ModelRoot, &GlobalTransform, Has<RagdollReady>)>,
    bones: Query<(&RagdollBoneConstraint, &LinearVelocity)>,
    mut exit: MessageWriter<AppExit>,
) {
    if !verify.enabled {
        return;
    }
    let now = time.elapsed_secs();

    // Phase 1: wait until every pedestrian is ragdoll-ready, then flip them all to ragdoll.
    if verify.triggered_at.is_none() {
        let total = manifest.urls.len();
        if total == 0 {
            return;
        }
        let ready = models.iter().filter(|(_, _, _, r)| *r).count();

        if ready != verify.last_ready {
            verify.last_ready = ready;
            verify.last_change_at = now;
        }
        if now - verify.last_log_at >= 1.0 {
            verify.last_log_at = now;
            info!(
                "RAGDOLL_VERIFY: waiting — {}/{} ragdoll-ready (spawned {})",
                ready,
                total,
                models.iter().count()
            );
        }

        // Fire when everyone is ready, or when readiness has stalled for 5s (stragglers/failed
        // network loads shouldn't block the whole verification).
        let all_ready = ready >= total;
        let stalled = ready > 0 && (now - verify.last_change_at) > 5.0;
        if !all_ready && !stalled {
            return;
        }
        for (ent, _, _, is_ready) in models.iter() {
            if !is_ready {
                continue;
            }
            commands.trigger(PedestrianAnimationControlEvent {
                ped: ent,
                animation: RAGDOLL_ANIMATION.to_string(),
                speed: 1.0,
            });
        }
        info!(
            "RAGDOLL_VERIFY: switching {}/{} ragdoll-ready models to ragdoll at t={:.2}s{}",
            ready,
            total,
            now,
            if all_ready { "" } else { " (stalled)" }
        );
        verify.triggered_at = Some(now);
        return;
    }

    let elapsed = now - verify.triggered_at.unwrap();

    // Phase 2: timed screenshots + per-model pose/velocity logging.
    for (i, &shot_t) in RAGDOLL_SHOT_TIMES.iter().enumerate() {
        if verify.shots_done[i] || elapsed < shot_t {
            continue;
        }
        verify.shots_done[i] = true;

        let path = format!("/home/p/VIDOEGAME/crack/ragdoll_{:.1}s.png", shot_t);
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
        info!(
            "RAGDOLL_VERIFY: === t+{:.1}s (elapsed {:.2}s) screenshot -> {} ===",
            shot_t, elapsed, path
        );

        // Aggregate bone linear velocities per model root.
        let mut agg: std::collections::HashMap<Entity, (f32, u32, f32)> =
            std::collections::HashMap::new();
        for (c, lv) in bones.iter() {
            let speed = lv.0.length();
            let e = agg.entry(c.root).or_insert((0.0, 0, 0.0));
            e.0 += speed;
            e.1 += 1;
            e.2 = e.2.max(speed);
        }

        let mut list: Vec<(Entity, &ModelRoot, &GlobalTransform)> = models
            .iter()
            .filter(|(_, m, _, _)| m.index < 10)
            .map(|(e, m, gt, _)| (e, m, gt))
            .collect();
        list.sort_by_key(|(_, m, _)| m.index);
        for (ent, m, gt) in list {
            let pos = gt.translation();
            let (avg, maxs) = agg
                .get(&ent)
                .map(|(s, c, mx)| (if *c > 0 { *s / *c as f32 } else { 0.0 }, *mx))
                .unwrap_or((0.0, 0.0));
            info!(
                "  model #{} '{}' pos=({:.2},{:.2},{:.2}) bone_vel avg={:.3} max={:.3} m/s",
                m.index, m.name, pos.x, pos.y, pos.z, avg, maxs
            );
        }
    }

    // Phase 3: exit.
    if elapsed >= RAGDOLL_VERIFY_EXIT && !verify.exited {
        verify.exited = true;
        info!("RAGDOLL_VERIFY: exiting at t+{:.2}s", elapsed);
        exit.write(AppExit::Success);
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

            // "ragdoll" first, then "A_TPose", then the rest of the catalog.
            let mut anim_names: Vec<String> = vec![RAGDOLL_ANIMATION.to_string()];
            if anims.catalog.contains_key("A_TPose") {
                anim_names.push("A_TPose".to_string());
            }
            for name in anims.catalog.keys() {
                if name != "A_TPose" {
                    anim_names.push(name.clone());
                }
            }
            let current = anim_sel.selected.clone();
            egui::ScrollArea::vertical()
                .max_height(160.0)
                .show(ui, |ui| {
                    for name in &anim_names {
                        if ui
                            .radio(current.as_ref() == Some(name), name)
                            .clicked()
                        {
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
