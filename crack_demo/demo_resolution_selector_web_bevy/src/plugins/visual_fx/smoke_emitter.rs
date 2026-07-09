use super::materials::{BillboardParams, BlendFxMaterial, FxKind};
use super::settings::VfxSettings;
use super::spawn::{VfxDrift, VfxMeshes, spawn_blend_billboard_fx};
use bevy::prelude::*;

#[derive(Component, Debug, Clone)]
pub struct SmokeEmitter {
    pub next_spawn_time: f32, // elapsed seconds when next smoke puff should spawn
    pub active_until: f32,    // elapsed seconds when this emitter should stop
}

pub fn tick_smoke_emitters(
    mut commands: Commands,
    time: Res<Time>,
    settings: Res<VfxSettings>,
    meshes: Option<Res<VfxMeshes>>,
    mut blend_mats: ResMut<Assets<BlendFxMaterial>>,
    mut q_emitters: Query<(Entity, &GlobalTransform, &mut SmokeEmitter)>,
) {
    if !settings.car_black_smoke {
        return;
    }
    let Some(meshes) = meshes else {
        return;
    };
    let now = time.elapsed_secs();

    for (ent, gt, mut emitter) in &mut q_emitters {
        if now >= emitter.active_until {
            // Emitter expired, remove the emitter component
            commands.entity(ent).remove::<SmokeEmitter>();
            continue;
        }

        if now >= emitter.next_spawn_time {
            // Spawn black smoke puff at the wreck top (approx 1.0m above wreck center)
            let pos = gt.translation() + Vec3::new(0.0, 1.0, 0.0);

            // Random offset & drift velocity
            let offset = Vec3::new(
                (rand::random::<f32>() - 0.5) * 0.4,
                0.0,
                (rand::random::<f32>() - 0.5) * 0.4,
            );
            let drift_vel = Vec3::new(
                (rand::random::<f32>() - 0.5) * 0.4,
                rand::random::<f32>() * 0.8 + 0.6, // rising
                (rand::random::<f32>() - 0.5) * 0.4,
            );

            let params = BillboardParams {
                color: Vec4::new(0.15, 0.15, 0.15, 0.65), // dark smoke
                spawn_time: now,
                lifetime: 2.5,
                start_radius: 0.8,
                end_radius: 3.2,
                seed: rand::random::<f32>(),
                kind: FxKind::BlackSmoke as u32,
                _pad: 0.0,
            };

            let smoke_entity = spawn_blend_billboard_fx(
                &mut commands,
                &mut blend_mats,
                &meshes,
                &time,
                pos + offset,
                params,
            );
            commands.entity(smoke_entity).insert(VfxDrift {
                velocity: drift_vel,
            });

            emitter.next_spawn_time = now + 0.4;
        }
    }
}
