use super::materials::{AdditiveFxMaterial, BillboardParams, BlendFxMaterial, FxKind};
use super::settings::VfxSettings;
use super::spawn::{VfxDrift, VfxMeshes, spawn_additive_billboard_fx, spawn_blend_billboard_fx};
use crate::plugins::weapons::weapon_attach::{WeaponExtents, WeaponModelState};
use bevy::prelude::*;

#[derive(Event, Debug, Clone)]
pub struct GunFxEvent {
    pub muzzle: Vec3,
    pub impact: Vec3,
    pub is_person: bool,
    pub is_miss: bool,
    pub shooter: Entity,
}

#[derive(Resource, Default)]
pub struct GunFxCounter(pub u32);

#[derive(Component, Debug, Clone)]
pub struct GunSmokeEmitter {
    pub next_spawn_time: f32,
    pub active_until: f32,
}

pub fn gun_fx_observer(
    trigger: On<GunFxEvent>,
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<VfxSettings>,
    meshes: Option<Res<VfxMeshes>>,
    mut additive_mats: ResMut<Assets<AdditiveFxMaterial>>,
    mut blend_mats: ResMut<Assets<BlendFxMaterial>>,
    q_model_state: Query<&WeaponModelState>,
    mut q_smoke_emitter: Query<&mut GunSmokeEmitter>,
    counter: Option<ResMut<GunFxCounter>>,
) {
    let event = trigger.event();
    let muzzle = event.muzzle;
    let impact = event.impact;
    let is_person = event.is_person;
    let is_miss = event.is_miss;
    let shooter = event.shooter;
    let now = time.elapsed_secs();

    let Some(meshes) = meshes else {
        return;
    };

    // 1. Muzzle Flash
    if settings.gun_muzzle_flash {
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
            muzzle,
            params,
        );
    }

    // Shot counter throttling
    let mut should_spawn_smoke = true;
    if let Some(mut cnt) = counter {
        cnt.0 += 1;
        if settings.muzzle_smoke_every > 1 {
            should_spawn_smoke = cnt.0 % settings.muzzle_smoke_every == 0;
        }
    }

    // 2. Muzzle Smoke Emitter & Puffs
    if settings.gun_muzzle_smoke && should_spawn_smoke {
        // Immediate puffs
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

            let smoke_entity = spawn_blend_billboard_fx(
                &mut commands,
                &mut blend_mats,
                &meshes,
                &time,
                muzzle,
                params,
            );
            commands.entity(smoke_entity).insert(VfxDrift {
                velocity: drift_vel,
            });
        }

        let target_ent = if let Ok(model_state) = q_model_state.get(shooter) {
            model_state.entity.unwrap_or(shooter)
        } else {
            shooter
        };

        if let Ok(mut existing) = q_smoke_emitter.get_mut(target_ent) {
            existing.active_until = now + 1.5;
        } else {
            commands.entity(target_ent).insert(GunSmokeEmitter {
                next_spawn_time: now + 0.15,
                active_until: now + 1.5,
            });
        }
    }

    // 3. Tracer
    if settings.gun_tracer {
        let shot_vector = impact - muzzle;
        let length = shot_vector.length();
        if length > 0.01 {
            let shot_dir = shot_vector / length;
            let rotation = Quat::from_rotation_arc(Vec3::X, shot_dir);
            let scale = Vec3::new(length, 1.0, 1.0);

            let params = BillboardParams {
                color: Vec4::new(1.0, 0.95, 0.6, 1.0),
                spawn_time: now,
                lifetime: 0.05,
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
                // The tracer branch of the shader rebuilds the full muzzle->impact ribbon,
                // which extends past the entity's mesh AABB; keep it from being culled.
                bevy::camera::visibility::NoFrustumCulling,
                super::spawn::VfxLifetime { despawn_at },
            ));
        }
    }

    // 4. Hit Spark Burst
    if settings.gun_hit_sparks && !is_miss {
        let spark_color = if is_person {
            Vec4::new(0.95, 0.15, 0.15, 1.0)
        } else {
            Vec4::new(1.0, 0.9, 0.2, 1.0)
        };

        let params = BillboardParams {
            color: spark_color,
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
            impact,
            params,
        );
    }
}

pub fn tick_gun_smoke_emitters(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<VfxSettings>,
    meshes: Option<Res<VfxMeshes>>,
    mut blend_mats: ResMut<Assets<BlendFxMaterial>>,
    mut q_emitters: Query<(
        Entity,
        &GlobalTransform,
        Option<&WeaponExtents>,
        &mut GunSmokeEmitter,
    )>,
) {
    if !settings.gun_muzzle_smoke {
        return;
    }
    let Some(meshes) = meshes else {
        return;
    };
    let now = time.elapsed_secs();

    for (ent, gt, extents_opt, mut emitter) in &mut q_emitters {
        if now >= emitter.active_until {
            commands.entity(ent).remove::<GunSmokeEmitter>();
            continue;
        }

        if now >= emitter.next_spawn_time {
            let pos = if let Some(extents) = extents_opt {
                gt.transform_point(Vec3::new(extents.max_x, 0.0, 0.0))
            } else {
                gt.translation()
            };

            let drift_vel = Vec3::new(
                (rand::random::<f32>() - 0.5) * 0.2,
                rand::random::<f32>() * 0.4 + 0.3,
                (rand::random::<f32>() - 0.5) * 0.2,
            );

            let params = BillboardParams {
                color: Vec4::new(0.8, 0.8, 0.82, 0.35),
                spawn_time: now,
                lifetime: 0.9,
                start_radius: 0.15,
                end_radius: 0.9,
                seed: rand::random::<f32>(),
                kind: FxKind::SmokePuff as u32,
                _pad: 0.0,
            };

            let smoke_entity = spawn_blend_billboard_fx(
                &mut commands,
                &mut blend_mats,
                &meshes,
                &time,
                pos,
                params,
            );
            commands.entity(smoke_entity).insert(VfxDrift {
                velocity: drift_vel,
            });

            emitter.next_spawn_time = now + 0.12;
        }
    }
}
