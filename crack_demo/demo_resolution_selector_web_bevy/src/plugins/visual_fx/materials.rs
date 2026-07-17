use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxKind {
    Fireball = 0,
    SmokePuff = 1,
    BlackSmoke = 2,
    MuzzleFlash = 3,
    SparkBurst = 4,
    Tracer = 5,
}

#[derive(Clone, Copy, ShaderType, Debug)]
pub struct BillboardParams {
    pub color: Vec4,     // base tint incl. alpha multiplier
    pub spawn_time: f32, // globals.time at spawn
    pub lifetime: f32,   // seconds
    pub start_radius: f32,
    pub end_radius: f32, // for expanding fireball/smoke
    pub seed: f32,       // per-instance noise offset
    pub kind: u32,       // FxKind
    pub _pad: f32,
}

// Additive transparency material (used for glowy effects)
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct AdditiveFxMaterial {
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
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct BlendFxMaterial {
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
