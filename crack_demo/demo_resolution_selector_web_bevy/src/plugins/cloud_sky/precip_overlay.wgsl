// Rain / snow overlay. Drawn on a quad held 2m in front of the camera with
// depth testing disabled, so precipitation covers the whole view.
// WebGL2-safe: pure hash noise, one uniform binding.

#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::{view, globals}

struct SkyParams {
    sun_dir: vec4<f32>, // xyz = direction toward sun, w = day factor
    amounts: vec4<f32>, // x = cumulus, y = cirrus, z = storm, w = overcast
    detail: vec4<f32>,  // per-layer octaves + cloud scale
    wind: vec4<f32>,    // xy = wind scroll, z = rain intensity, w = snow intensity
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> u: SkyParams;

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

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Rain: thin, elongated, slightly wind-slanted streaks on a hashed column
// grid over (yaw, pitch).
fn rain(rd: vec3<f32>, intensity: f32, day: f32) -> f32 {
    let yaw = atan2(rd.x, rd.z);
    // slant: columns shift with pitch so streaks lean along the wind
    let slant = (u.wind.x + u.wind.y) * 6.0 + 0.15;
    let p_yaw = yaw + rd.y * slant;
    let cols = 70.0;
    let col_id = floor(p_yaw * cols);
    let rnd = hash2(vec2<f32>(col_id, 3.7));
    // only a subset of columns carries a streak
    if (rnd > intensity * 0.5) { return 0.0; }
    // very thin horizontally
    let cx = abs(fract(p_yaw * cols) - 0.5);
    let thin = smoothstep(0.10, 0.03, cx);
    if (thin <= 0.001) { return 0.0; }
    // fall animation: pitch scrolls downward fast, per-column speed/offset.
    // Long tail + quick head fade -> elongated streak, not a square dash.
    let speed = 2.5 + rnd * 2.0;
    let span = 4.0 + rnd * 3.0;
    let phase = fract(rd.y * span + globals.time * speed + rnd * 40.0);
    let streak = smoothstep(0.0, 0.02, phase) * smoothstep(0.60, 0.35, phase);
    return thin * streak * intensity;
}

// Snow: soft drifting dots on a hashed cell grid over (yaw, pitch).
fn snow(rd: vec3<f32>, intensity: f32, day: f32) -> f32 {
    let yaw = atan2(rd.x, rd.z);
    let grid = vec2<f32>(yaw * 36.0, rd.y * 24.0);
    let drift = globals.time * 0.55;
    // slow fall + sideways wobble
    let p = vec2<f32>(grid.x + sin(drift + grid.y * 2.0) * 0.15, grid.y + drift);
    let cell = floor(p);
    let rnd = hash2(cell);
    if (rnd > intensity * 0.5) { return 0.0; }
    let center = vec2<f32>(hash2(cell + 1.3), hash2(cell + 5.1));
    let d = length(fract(p) - center);
    let flake = smoothstep(0.10, 0.02, d);
    return flake * intensity;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let rain_amt = u.wind.z;
    let snow_amt = u.wind.w;
    if (rain_amt <= 0.001 && snow_amt <= 0.001) { discard; }

    let rd = normalize(in.world_pos - view.world_position);
    let day = u.sun_dir.w;
    let r = rain(rd, rain_amt, day);
    let s = snow(rd, snow_amt, day);
    if (r + s <= 0.001) { discard; }

    let rain_col = vec3<f32>(0.65, 0.72, 0.85) * (0.2 + 0.8 * day);
    let snow_col = vec3<f32>(0.95, 0.96, 1.0) * (0.25 + 0.75 * day);
    let col = rain_col * r + snow_col * s;
    return vec4<f32>(col, clamp(r * 0.55 + s * 0.9, 0.0, 1.0));
}
