use super::materials::{AdditiveFxMaterial, BillboardParams, BlendFxMaterial};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::prelude::*;

#[derive(Component, Debug)]
pub struct VfxLifetime {
    pub despawn_at: f64, // seconds, absolute time elapsed
}

#[derive(Component, Debug)]
pub struct VfxDrift {
    pub velocity: Vec3,
}

#[derive(Resource, Debug)]
pub struct VfxMeshes {
    pub quad: Handle<Mesh>,
}

pub fn spawn_additive_billboard_fx(
    commands: &mut Commands,
    mats: &mut Assets<AdditiveFxMaterial>,
    meshes: &VfxMeshes,
    time: &Time,
    pos: Vec3,
    params: BillboardParams,
) -> Entity {
    let despawn_at = time.elapsed_secs_f64() + params.lifetime as f64 + 0.05;
    let mat = mats.add(AdditiveFxMaterial { params });
    commands
        .spawn((
            Mesh3d(meshes.quad.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos),
            // The vertex shader expands the 1x1 quad up to `radius` around the center,
            // well beyond the mesh AABB, so disable frustum culling to keep it from
            // popping out of view near the screen edges.
            NoFrustumCulling,
            VfxLifetime { despawn_at },
        ))
        .id()
}

pub fn spawn_blend_billboard_fx(
    commands: &mut Commands,
    mats: &mut Assets<BlendFxMaterial>,
    meshes: &VfxMeshes,
    time: &Time,
    pos: Vec3,
    params: BillboardParams,
) -> Entity {
    let despawn_at = time.elapsed_secs_f64() + params.lifetime as f64 + 0.05;
    let mat = mats.add(BlendFxMaterial { params });
    commands
        .spawn((
            Mesh3d(meshes.quad.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(pos),
            // See note in spawn_additive_billboard_fx: the shader expands past the mesh AABB.
            NoFrustumCulling,
            VfxLifetime { despawn_at },
        ))
        .id()
}

pub fn despawn_expired_fx(
    mut commands: Commands,
    time: Res<Time>,
    q: Query<(Entity, &VfxLifetime)>,
) {
    let now = time.elapsed_secs_f64();
    for (e, l) in &q {
        if now >= l.despawn_at {
            if let Ok(mut c) = commands.get_entity(e) {
                c.despawn();
            }
        }
    }
}

pub fn tick_vfx_drift(time: Res<Time>, mut q: Query<(&mut Transform, &VfxDrift)>) {
    let dt = time.delta_secs();
    for (mut tf, drift) in &mut q {
        tf.translation += drift.velocity * dt;
    }
}

pub fn setup_vfx_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    // Create a 1x1 quad in XY plane, centered at 0,0
    let quad = Rectangle::new(1.0, 1.0);
    let handle = meshes.add(quad);
    commands.insert_resource(VfxMeshes { quad: handle });
}
