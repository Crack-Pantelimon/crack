#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::{view, globals}

struct CloudParams {
    color: vec4<f32>,
    coverage: f32,
    opacity: f32,
    wind: vec2<f32>,
    scale: f32,
    debug_solid: f32,
    _pad1: f32,
    _pad2: f32,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: CloudParams;

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

// cheap hash noise (no textures)
fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p); let f = fract(p);
    let a = hash2(i); let b = hash2(i + vec2(1.0, 0.0));
    let c = hash2(i + vec2(0.0, 1.0)); let d = hash2(i + vec2(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn fbm(p: vec2<f32>) -> f32 {
    return 0.5 * vnoise(p) + 0.25 * vnoise(p * 2.03) + 0.125 * vnoise(p * 4.01);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if (u.debug_solid > 0.5) {
        return vec4<f32>(1.0, 1.0, 1.0, 0.3 * u.opacity * u.color.a);
    }
    let uv = in.world_pos.xz * u.scale + globals.time * u.wind;
    // fbm ∈ [0,0.875]; remap to ~[0,1] so `coverage` behaves intuitively.
    let n = clamp(fbm(uv) / 0.875, 0.0, 1.0);
    let d = smoothstep(u.coverage, u.coverage + 0.35, n); // wider ramp than (coverage,1)
    if (d <= 0.001) { discard; }
    return vec4<f32>(u.color.rgb, d * u.opacity * u.color.a);
}
