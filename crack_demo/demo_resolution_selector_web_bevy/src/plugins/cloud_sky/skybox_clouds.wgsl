// Procedural sky dome: blue-sky gradient, sun, day/night, and three analytic
// cloud layers (cumulus / cirrus / storm). WebGL2-safe: textureless hash
// noise, a single uniform binding, no storage buffers / compute.

#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::{view, globals}

struct SkyParams {
    sun_dir: vec4<f32>, // xyz = direction toward sun, w = day factor (0..1)
    amounts: vec4<f32>, // x = cumulus, y = cirrus, z = storm, w = overcast
    detail: vec4<f32>,  // x/y/z = FBM octaves per layer, w = cloud scale
    wind: vec4<f32>,    // xy = wind uv scroll / s, z = rain, w = snow
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

// ---------------------------------------------------------------------------
// Noise (hash-based value noise + FBM with octave rotation, no textures)
// ---------------------------------------------------------------------------

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    // quintic smoothing (smoother gradients than cubic Hermite)
    let w = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

// FBM in ~[0, 1). `octaves` is a uniform, so loop to a compile-time max and
// break — legal in WGSL and GLSL ES 3.0.
fn fbm(p_in: vec2<f32>, octaves: f32) -> f32 {
    var p = p_in;
    var sum = 0.0;
    var amp = 0.5;
    var norm = 0.0;
    let rot = mat2x2<f32>(vec2<f32>(0.8, -0.6), vec2<f32>(0.6, 0.8));
    for (var i = 0; i < 8; i++) {
        if (f32(i) >= octaves) { break; }
        sum += amp * vnoise(p);
        norm += amp;
        p = rot * p * 2.03;
        amp *= 0.5;
    }
    return sum / max(norm, 1e-4);
}

// ---------------------------------------------------------------------------
// Sky background
// ---------------------------------------------------------------------------

fn sky_color(rd: vec3<f32>, sun_dir: vec3<f32>, day: f32, overcast: f32) -> vec3<f32> {
    let elev = clamp(rd.y, 0.0, 1.0);
    let g = pow(elev, 0.55);

    // day gradient: light blue horizon -> deep blue zenith
    let day_col = mix(vec3<f32>(0.62, 0.76, 0.95), vec3<f32>(0.10, 0.32, 0.85), g);
    // night gradient: dark blue
    let night_col = mix(vec3<f32>(0.05, 0.08, 0.16), vec3<f32>(0.01, 0.02, 0.06), g);
    var col = mix(night_col, day_col, day);

    let sundot = clamp(dot(rd, sun_dir), 0.0, 1.0);
    // warm glow around the sun, stronger when the sun is low
    let low_sun = 1.0 - clamp(sun_dir.y * 2.5, 0.0, 1.0);
    let warm = mix(vec3<f32>(1.0, 0.9, 0.7), vec3<f32>(1.0, 0.55, 0.25), low_sun);
    col += warm * pow(sundot, 8.0) * 0.30 * day;
    col += warm * pow(sundot, 64.0) * 0.45 * day;
    // sun disc
    col += vec3<f32>(1.0, 0.95, 0.85) * pow(sundot, 1800.0) * 8.0 * day;

    // stars at night
    if (day < 0.35 && rd.y > 0.02) {
        let sp = floor(rd.xz / max(rd.y, 0.1) * 140.0);
        let star = step(0.996, hash2(sp));
        let twinkle = 0.6 + 0.4 * sin(globals.time * 3.0 + hash2(sp + 7.7) * 40.0);
        col += vec3<f32>(0.9, 0.95, 1.0) * star * twinkle * (0.35 - day) * 1.6;
    }

    // overcast: flatten toward gray
    let gray = vec3<f32>(0.38, 0.40, 0.45) * (0.15 + 0.85 * day);
    return mix(col, gray, overcast * 0.75);
}

// ---------------------------------------------------------------------------
// Cloud layers — each returns vec4(rgb, alpha)
// ---------------------------------------------------------------------------

fn wind_scroll() -> vec2<f32> {
    return u.wind.xy * globals.time;
}

fn layer_uv(rd: vec3<f32>, h: f32, scale: f32) -> vec2<f32> {
    let t = min((h - view.world_position.y) / max(rd.y, 0.03), 25000.0);
    let hit = view.world_position + rd * t;
    return hit.xz * scale * u.detail.w + wind_scroll();
}

fn cumulus_layer(rd: vec3<f32>, sundot: f32) -> vec4<f32> {
    let amount = u.amounts.x;
    if (amount <= 0.001) { return vec4<f32>(0.0); }
    let uv = layer_uv(rd, 200.0, 0.0016);
    // cheap domain warp for puffy, non-grid shapes
    let w = vec2<f32>(vnoise(uv * 2.1 + 11.3), vnoise(uv * 2.1 + 47.1));
    let n = fbm(uv + 0.4 * w, u.detail.x);
    let cov = 1.0 - amount;
    let dens = smoothstep(cov, cov + 0.4, n);
    if (dens <= 0.001) { return vec4<f32>(0.0); }

    // cheap directional lighting: one extra FBM sample toward the sun
    let sun_xz = normalize(u.sun_dir.xz + vec2<f32>(1e-4, 0.0));
    let n2 = fbm(uv + 0.4 * w + sun_xz * 0.12, max(u.detail.x - 2.0, 1.0));
    let dif = clamp((n - n2) * 6.0, -1.0, 1.0);

    let day = u.sun_dir.w;
    let lit = vec3<f32>(1.0, 0.98, 0.95) * (0.15 + 0.85 * day);
    let shaded = vec3<f32>(0.55, 0.58, 0.68) * (0.15 + 0.85 * day);
    var col = mix(shaded, lit, 0.5 + 0.5 * dif);
    // darker bottoms for thick clouds
    col *= mix(1.0, 0.62, dens * (1.0 - 0.5 * dif));
    // silver lining on thin edges facing the sun
    let edge = dens * (1.0 - dens) * 4.0;
    col += vec3<f32>(1.0, 0.85, 0.65) * pow(sundot, 5.0) * edge * 0.35 * day;
    return vec4<f32>(col, dens * 0.95);
}

fn cirrus_layer(rd: vec3<f32>, day: f32) -> vec4<f32> {
    let amount = u.amounts.y;
    if (amount <= 0.001) { return vec4<f32>(0.0); }
    // anisotropic uv -> long streaky wisps; slower wind
    let t = min((800.0 - view.world_position.y) / max(rd.y, 0.03), 25000.0);
    let hit = view.world_position + rd * t;
    let uv2 = hit.xz * vec2<f32>(0.00045, 0.0015) * u.detail.w + wind_scroll() * 0.5;
    let warp = vec2<f32>(vnoise(uv2 * 3.1), vnoise(uv2 * 3.1 + 31.7));
    let n = fbm(uv2 + 0.9 * warp, u.detail.y);
    let cov = 1.0 - amount * 0.85;
    let a = smoothstep(cov, cov + 0.5, n) * 0.4 * amount;
    if (a <= 0.001) { return vec4<f32>(0.0); }
    let col = vec3<f32>(0.92, 0.94, 1.0) * (0.1 + 0.9 * day);
    return vec4<f32>(col, a);
}

fn storm_layer(rd: vec3<f32>, day: f32) -> vec4<f32> {
    let amount = u.amounts.z;
    if (amount <= 0.001) { return vec4<f32>(0.0); }
    let t = min((140.0 - view.world_position.y) / max(rd.y, 0.03), 25000.0);
    let hit = view.world_position + rd * t;
    let uv = hit.xz * 0.0007 * u.detail.w + wind_scroll() * 1.6;
    let n = fbm(uv, u.detail.z);
    let cov = 1.0 - amount;
    let dens = smoothstep(cov, cov + 0.55, n);
    if (dens <= 0.001) { return vec4<f32>(0.0); }
    let top = vec3<f32>(0.48, 0.50, 0.56) * (0.1 + 0.9 * day);
    let bottom = vec3<f32>(0.16, 0.17, 0.21) * (0.1 + 0.9 * day);
    let col = mix(top, bottom, dens);
    return vec4<f32>(col, dens * 0.98);
}

// ---------------------------------------------------------------------------

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let cam = view.world_position;
    let rd = normalize(in.world_pos - cam);
    let sun_dir = normalize(u.sun_dir.xyz);
    let day = u.sun_dir.w;
    let overcast = u.amounts.w;
    let sundot = clamp(dot(rd, sun_dir), 0.0, 1.0);

    var col = sky_color(rd, sun_dir, day, overcast);

    // clouds only above the horizon; fade them into the haze near it
    if (rd.y > 0.005) {
        let horizon_fade = smoothstep(0.005, 0.10, rd.y);
        // back to front: cirrus (highest) -> cumulus -> storm (lowest)
        let cir = cirrus_layer(rd, day);
        col = mix(col, cir.rgb, cir.a * horizon_fade);
        let cum = cumulus_layer(rd, sundot);
        col = mix(col, cum.rgb, cum.a * horizon_fade);
        let sto = storm_layer(rd, day);
        col = mix(col, sto.rgb, sto.a * horizon_fade);
    }

    return vec4<f32>(col, 1.0);
}
