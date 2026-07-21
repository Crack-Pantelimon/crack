use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// fx kind.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxKind {
/// Documented public item.
    Fireball = 0,
/// Documented public item.
    SmokePuff = 1,
/// Documented public item.
    BlackSmoke = 2,
/// Documented public item.
    MuzzleFlash = 3,
/// Documented public item.
    SparkBurst = 4,
/// Documented public item.
    Tracer = 5,
}

/// billboard params.
#[derive(Clone, Copy, ShaderType, Debug)]
pub struct BillboardParams {
/// color field.
    pub color: Vec4,     // base tint incl. alpha multiplier
/// spawn time field.
    pub spawn_time: f32, // globals.time at spawn
/// lifetime field.
    pub lifetime: f32,   // seconds
/// start radius field.
    pub start_radius: f32,
/// end radius field.
    pub end_radius: f32, // for expanding fireball/smoke
/// seed field.
    pub seed: f32,       // per-instance noise offset
/// kind field.
    pub kind: u32,       // FxKind
/// pad field.
    pub _pad: f32,
}

// Additive transparency material (used for glowy effects)
/// additive fx material.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct AdditiveFxMaterial {
/// params field.
    #[uniform(0)]
    pub params: BillboardParams,
}

impl Material for AdditiveFxMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/visual_fx/billboard_fx.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/visual_fx/billboard_fx.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

// Alpha blending transparency material (used for smoke effects)
/// blend fx material.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct BlendFxMaterial {
/// params field.
    #[uniform(0)]
    pub params: BillboardParams,
}

impl Material for BlendFxMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/visual_fx/billboard_fx.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/visual_fx/billboard_fx.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _layout: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}
