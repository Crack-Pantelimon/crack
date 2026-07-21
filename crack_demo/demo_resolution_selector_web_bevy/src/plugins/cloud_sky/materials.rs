use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// Uniforms shared by the sky dome and the precipitation overlay.
///
/// Everything is packed into `Vec4`s so the struct stays 16-byte aligned
/// and WebGL2-friendly.
#[derive(Clone, Copy, ShaderType, Debug)]
pub struct SkyParamsUniform {
    /// xyz = direction toward the sun, w = day factor (0 night .. 1 day).
    pub sun_dir: Vec4,
    /// Sunlight color temperature in Kelvin (1500..6000).
    pub sun_temperature: f32,
    /// x = cumulus amount, y = cirrus amount, z = storm amount, w = overcast.
    pub amounts: Vec4,
    /// x = cumulus octaves, y = cirrus octaves, z = storm octaves, w = cloud scale.
    pub detail: Vec4,
    /// x = wind uv.x, y = wind uv.y (per second), z = rain intensity, w = snow intensity.
    pub wind: Vec4,
}

// Sky dome material: blue gradient + sun + 3 cloud layers, fully procedural.
/// sky dome material.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct SkyDomeMaterial {
/// params field.
    #[uniform(0)]
    pub params: SkyParamsUniform,
}

impl Material for SkyDomeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/cloud_sky/skybox_clouds.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/cloud_sky/skybox_clouds.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        // Opaque on purpose: the dome sits in the opaque phase (with depth
        // write off) so transparent overlays — rain/snow, ground shadow —
        // always draw on top of the sky. With Blend, the transparent phase
        // would sort the camera-centered dome last and it would cover them.
        AlphaMode::Opaque
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
        // Keep the depth test (ground occludes the sky) but don't write:
        // the dome behaves like geometry at infinity.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

// Camera-following overlay quad for rain/snow. No depth testing so the
// precipitation also draws in front of the world geometry.
/// precip overlay material.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct PrecipOverlayMaterial {
/// params field.
    #[uniform(0)]
    pub params: SkyParamsUniform,
}

impl Material for PrecipOverlayMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/cloud_sky/precip_overlay.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/cloud_sky/precip_overlay.wgsl".into()
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
        // Keep the depth attachment format (the render pass has one) but
        // never test or write depth: precipitation draws over the world.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = Some(false);
            depth.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always);
        }
        Ok(())
    }
}

/// ground shadow uniform.
#[derive(Clone, Copy, ShaderType, Debug)]
pub struct GroundShadowUniform {
    /// x = intensity, y = uv scale, z/w = unused.
    pub params: Vec4,
    /// x/y = scroll speed (uv units per second), z/w = unused.
    pub wind: Vec4,
}

// Flat decal multiplying the ground with scrolling cloud shadows.
// The texture is generated on the CPU once at startup (see systems.rs).
/// cloud ground shadow material.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct CloudGroundShadowMaterial {
/// params field.
    #[uniform(0)]
    pub params: GroundShadowUniform,
/// texture field.
    #[texture(1)]
    #[sampler(2)]
    pub texture: Handle<Image>,
}

impl Material for CloudGroundShadowMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/cloud_sky/ground_shadow.wgsl".into()
    }
    fn vertex_shader() -> ShaderRef {
        "embedded://demo_resolution_selector_web_bevy/plugins/cloud_sky/ground_shadow.wgsl".into()
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
