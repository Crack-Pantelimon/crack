// Scrolling cloud shadows on the ground. Samples a CPU-generated tileable
// FBM texture (white = cloud) and outputs a black blend layer whose alpha is
// the cloud coverage — cheap moving shadows coherent with the wind.

#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::{view, globals}

struct GroundShadowParams {
    params: vec4<f32>, // x = intensity, y = uv scale
    wind: vec4<f32>,   // xy = scroll speed (uv / s)
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: GroundShadowParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var shadow_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var shadow_smp: sampler;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
};

@vertex
fn vertex(@location(0) pos: vec3<f32>,
          @builtin(instance_index) inst: u32) -> VertexOutput {
    let model = get_world_from_local(inst);
    let world_pos = (model * vec4<f32>(pos, 1.0)).xyz;
    var out: VertexOutput;
    out.world_pos = world_pos;
    out.clip = view.clip_from_world * vec4<f32>(world_pos, 1.0);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let intensity = u.params.x;
    if (intensity <= 0.001) { discard; }
    let uv = in.world_pos.xz * u.params.y + u.wind.xy * globals.time;
    let cloud = textureSample(shadow_tex, shadow_smp, uv).r;
    let alpha = cloud * intensity;
    if (alpha <= 0.001) { discard; }
    return vec4<f32>(0.0, 0.0, 0.02, alpha);
}
