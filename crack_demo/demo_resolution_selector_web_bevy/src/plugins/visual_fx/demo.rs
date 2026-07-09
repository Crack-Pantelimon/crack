use super::car_explosion::CarExplosionEvent;
use super::materials::{AdditiveFxMaterial, BillboardParams, BlendFxMaterial, FxKind};
use super::settings::VfxSettings;
use super::spawn::{VfxDrift, VfxMeshes, spawn_additive_billboard_fx, spawn_blend_billboard_fx};
use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DemoEffect {
    Fireball,
    ExplosionSmoke,
    BlackSmoke,
    MuzzleFlash,
    SparkBurst,
    Tracer,
    MuzzleSmoke,
    CarExplosion,
}

impl DemoEffect {
    pub const ALL: [DemoEffect; 8] = [
        DemoEffect::Fireball,
        DemoEffect::ExplosionSmoke,
        DemoEffect::BlackSmoke,
        DemoEffect::MuzzleFlash,
        DemoEffect::SparkBurst,
        DemoEffect::Tracer,
        DemoEffect::MuzzleSmoke,
        DemoEffect::CarExplosion,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DemoEffect::Fireball => "Fireball",
            DemoEffect::ExplosionSmoke => "Explosion Smoke",
            DemoEffect::BlackSmoke => "Black Smoke",
            DemoEffect::MuzzleFlash => "Muzzle Flash",
            DemoEffect::SparkBurst => "Spark Burst",
            DemoEffect::Tracer => "Tracer (Beam)",
            DemoEffect::MuzzleSmoke => "Muzzle Smoke Puff",
            DemoEffect::CarExplosion => "Full Car Explosion (Combo)",
        }
    }
}

#[derive(Resource)]
pub struct VfxDemoState {
    pub selected: DemoEffect,
    pub last_pick: Option<Vec3>,
    pub last_spawn: f32,
    pub auto_face_camera: bool,
}

impl Default for VfxDemoState {
    fn default() -> Self {
        Self {
            selected: DemoEffect::Fireball,
            last_pick: None,
            last_spawn: -1.0,
            auto_face_camera: true,
        }
    }
}

pub struct VfxDemoPlugin;

impl Plugin for VfxDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VfxDemoState>()
            .add_systems(Update, (click_ground_to_spawn, draw_pick_gizmo))
            .add_systems(EguiPrimaryContextPass, vfx_demo_ui);
    }
}

fn click_ground_to_spawn(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    spatial: SpatialQuery,
    meshes: Option<Res<VfxMeshes>>,
    mut additive_mats: ResMut<Assets<AdditiveFxMaterial>>,
    mut blend_mats: ResMut<Assets<BlendFxMaterial>>,
    settings: Res<VfxSettings>,
    mut state: ResMut<VfxDemoState>,
    mut contexts: EguiContexts,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }

    if let Ok(ctx) = contexts.ctx_mut() {
        if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
            return;
        }
    }

    let now = time.elapsed_secs();
    if now - state.last_spawn < 0.1 {
        return;
    }

    let Some(meshes) = meshes else {
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = cameras.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let Some(hit) = spatial.cast_ray(
        ray.origin,
        ray.direction,
        10000.0,
        true,
        &SpatialQueryFilter::default(),
    ) else {
        return;
    };

    let hit_point = ray.origin + *ray.direction * hit.distance;
    state.last_spawn = now;
    state.last_pick = Some(hit_point);

    match state.selected {
        DemoEffect::Fireball => {
            let params = BillboardParams {
                color: Vec4::new(1.0, 0.6, 0.1, 1.0),
                spawn_time: now,
                lifetime: settings.fireball_lifetime,
                start_radius: 1.0,
                end_radius: settings.fireball_radius,
                seed: rand::random::<f32>(),
                kind: FxKind::Fireball as u32,
                _pad: 0.0,
            };
            spawn_additive_billboard_fx(
                &mut commands,
                &mut additive_mats,
                &meshes,
                &time,
                hit_point + Vec3::Y * 0.5,
                params,
            );
        }
        DemoEffect::ExplosionSmoke => {
            let params = BillboardParams {
                color: Vec4::new(0.6, 0.6, 0.6, settings.smoke_opacity),
                spawn_time: now,
                lifetime: settings.smoke_lifetime,
                start_radius: 1.5,
                end_radius: 5.0,
                seed: rand::random::<f32>(),
                kind: FxKind::SmokePuff as u32,
                _pad: 0.0,
            };
            let drift_vel = Vec3::new(
                (rand::random::<f32>() - 0.5) * 0.4,
                rand::random::<f32>() * 0.8 + 0.5,
                (rand::random::<f32>() - 0.5) * 0.4,
            );
            let ent = spawn_blend_billboard_fx(
                &mut commands,
                &mut blend_mats,
                &meshes,
                &time,
                hit_point,
                params,
            );
            commands.entity(ent).insert(VfxDrift {
                velocity: drift_vel,
            });
        }
        DemoEffect::BlackSmoke => {
            let params = BillboardParams {
                color: Vec4::new(0.15, 0.15, 0.15, 0.65),
                spawn_time: now,
                lifetime: 2.5,
                start_radius: 0.8,
                end_radius: 3.2,
                seed: rand::random::<f32>(),
                kind: FxKind::BlackSmoke as u32,
                _pad: 0.0,
            };
            let drift_vel = Vec3::new(
                (rand::random::<f32>() - 0.5) * 0.3,
                rand::random::<f32>() * 0.6 + 0.4,
                (rand::random::<f32>() - 0.5) * 0.3,
            );
            let ent = spawn_blend_billboard_fx(
                &mut commands,
                &mut blend_mats,
                &meshes,
                &time,
                hit_point,
                params,
            );
            commands.entity(ent).insert(VfxDrift {
                velocity: drift_vel,
            });
        }
        DemoEffect::MuzzleFlash => {
            let params = BillboardParams {
                color: Vec4::new(1.0, 0.95, 0.6, 1.0),
                spawn_time: now,
                lifetime: 0.04,
                start_radius: 0.15,
                end_radius: 0.15,
                seed: rand::random::<f32>(),
                kind: FxKind::MuzzleFlash as u32,
                _pad: 0.0,
            };
            spawn_additive_billboard_fx(
                &mut commands,
                &mut additive_mats,
                &meshes,
                &time,
                hit_point + Vec3::Y * 0.2,
                params,
            );
        }
        DemoEffect::SparkBurst => {
            let params = BillboardParams {
                color: Vec4::new(1.0, 0.9, 0.2, 1.0),
                spawn_time: now,
                lifetime: 0.15,
                start_radius: 0.05,
                end_radius: 0.5 * settings.spark_count_scale.clamp(0.1, 4.0),
                seed: rand::random::<f32>(),
                kind: FxKind::SparkBurst as u32,
                _pad: 0.0,
            };
            spawn_additive_billboard_fx(
                &mut commands,
                &mut additive_mats,
                &meshes,
                &time,
                hit_point + Vec3::Y * 0.1,
                params,
            );
        }
        DemoEffect::Tracer => {
            let muzzle = hit_point + Vec3::new(0.0, 0.1, 0.0);
            let impact = if state.auto_face_camera {
                camera_transform.translation() - camera_transform.forward() * 2.0
            } else {
                hit_point + Vec3::new(-3.0, 1.5, 3.0)
            };

            let shot_vector = impact - muzzle;
            let length = shot_vector.length();
            if length > 0.01 {
                let shot_dir = shot_vector / length;
                let rotation = Quat::from_rotation_arc(Vec3::X, shot_dir);
                let scale = Vec3::new(length, 1.0, 1.0);

                let params = BillboardParams {
                    color: Vec4::new(1.0, 0.95, 0.6, 1.0),
                    spawn_time: now,
                    lifetime: 0.08,
                    start_radius: settings.tracer_width,
                    end_radius: settings.tracer_width * 0.5,
                    seed: rand::random::<f32>(),
                    kind: FxKind::Tracer as u32,
                    _pad: 0.0,
                };

                let despawn_at = time.elapsed_secs_f64() + params.lifetime as f64 + 0.05;
                let mat = additive_mats.add(AdditiveFxMaterial { params });

                commands.spawn((
                    Mesh3d(meshes.quad.clone()),
                    MeshMaterial3d(mat),
                    Transform {
                        translation: muzzle,
                        rotation,
                        scale,
                    },
                    bevy::camera::visibility::NoFrustumCulling,
                    super::spawn::VfxLifetime { despawn_at },
                ));
            }
        }
        DemoEffect::MuzzleSmoke => {
            for _ in 0..2 {
                let drift_vel = Vec3::new(
                    (rand::random::<f32>() - 0.5) * 0.2,
                    rand::random::<f32>() * 0.4 + 0.3,
                    (rand::random::<f32>() - 0.5) * 0.2,
                );

                let params = BillboardParams {
                    color: Vec4::new(0.8, 0.8, 0.82, 0.7),
                    spawn_time: now,
                    lifetime: 0.9,
                    start_radius: 0.15,
                    end_radius: 0.9,
                    seed: rand::random::<f32>(),
                    kind: FxKind::SmokePuff as u32,
                    _pad: 0.0,
                };

                let ent = spawn_blend_billboard_fx(
                    &mut commands,
                    &mut blend_mats,
                    &meshes,
                    &time,
                    hit_point,
                    params,
                );
                commands.entity(ent).insert(VfxDrift {
                    velocity: drift_vel,
                });
            }
        }
        DemoEffect::CarExplosion => {
            commands.trigger(CarExplosionEvent {
                position: hit_point,
            });
        }
    }
}

fn draw_pick_gizmo(state: Res<VfxDemoState>, mut gizmos: Gizmos) {
    let Some(p) = state.last_pick else {
        return;
    };
    let color = Color::srgb(1.0, 0.2, 0.6);
    gizmos.sphere(p, 0.15, color);
    gizmos.line(p, p + Vec3::Y * 0.8, color);
}

fn vfx_demo_ui(mut contexts: EguiContexts, mut state: ResMut<VfxDemoState>) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("🎆 VFX Demo")
        .default_width(260.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Left-click the ground to spawn the selected visual effect.")
                    .size(11.0),
            );
            ui.separator();

            ui.checkbox(&mut state.auto_face_camera, "Point Tracer toward camera");

            ui.separator();
            ui.label(egui::RichText::new("Effects:").strong());

            for effect in DemoEffect::ALL.iter() {
                let label = effect.label();
                if ui
                    .radio_value(&mut state.selected, *effect, label)
                    .clicked()
                {
                    info!("Selected VFX effect: {:?}", effect);
                }
            }
        });
}
