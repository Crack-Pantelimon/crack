#import bevy_pbr::mesh_functions::get_world_from_local
#import bevy_pbr::mesh_view_bindings::view
#import bevy_render::globals::globals

struct Params {
    color: vec4<f32>,
    spawn_time: f32,
    lifetime: f32,
    start_radius: f32,
    end_radius: f32,
    seed: f32,
    kind: u32,
    _pad: f32,
};
@group(2) @binding(0) var<uniform> P: Params;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(@location(0) pos: vec3<f32>,
          @builtin(instance_index) inst: u32) -> VOut {
    // Entity translation = column 3 of the model matrix.
    let model = get_world_from_local(inst);
    let center = model[3].xyz;

    let age = clamp((globals.time - P.spawn_time) / P.lifetime, 0.0, 1.0);
    var world: vec3<f32>;

    if (P.kind == 5u) {
        // Tracer: stretched screen-facing ribbon from muzzle to impact.
        // model[0].xyz is local X axis scaled by the segment length.
        let shot_vector = model[0].xyz;
        let camera_dir = normalize(center - view.world_position);
        let shot_dir = normalize(shot_vector);
        let lateral = normalize(cross(shot_dir, camera_dir));
        let radius = mix(P.start_radius, P.end_radius, age);
        
        // pos.x is in [-0.5, 0.5], pos.y is in [-0.5, 0.5]
        world = center + shot_vector * pos.x + lateral * (pos.y * radius);
    } else {
        // Standard camera-facing billboard
        let radius = mix(P.start_radius, P.end_radius, age);
        let right = view.world_from_view[0].xyz;
        let up    = view.world_from_view[1].xyz;
        world = center + (right * pos.x + up * pos.y) * radius;
    }

    var out: VOut;
    out.clip = view.clip_from_world * vec4<f32>(world, 1.0);
    out.uv = pos.xy * 2.0; // quad is 1x1 -> uv in [-1,1]
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
fn fragment(in: VOut) -> @location(0) vec4<f32> {
    let age = clamp((globals.time - P.spawn_time) / P.lifetime, 0.0, 1.0);
    let r = length(in.uv);                    // 0 center -> ~1.4 corner
    let disk = smoothstep(1.0, 0.0, r);       // soft round mask
    if (disk <= 0.001) { discard; }

    let n = fbm(in.uv * 3.0 + vec2(P.seed, -age * 2.0));

    var rgb = P.color.rgb;
    var alpha = P.color.a;

    switch P.kind {
        case 0u: { // Fireball: hot core -> orange -> fade, turbulent edge
            let heat = disk * (0.6 + 0.4 * n);
            rgb = mix(vec3(1.0, 0.3, 0.0), vec3(1.0, 0.95, 0.4), heat);
            alpha = heat * (1.0 - age) * P.color.a;
        }
        case 1u, 2u: { // Smoke (light or black): billowy, fade in then out
            let puff = disk * (0.5 + 0.5 * n);
            let fade = smoothstep(0.0, 0.15, age) * (1.0 - age);
            alpha = puff * fade * P.color.a;
        }
        case 3u: { // Muzzle flash: sharp star, very short
            let star = pow(disk, 2.0) * (0.7 + 0.3 * n);
            alpha = star * (1.0 - age) * P.color.a;
        }
        case 4u: { // Spark burst: radial streaks
            let ang = atan2(in.uv.y, in.uv.x);
            let streak = pow(abs(sin(ang * 9.0 + P.seed * 6.28)), 8.0);
            alpha = streak * (1.0 - r) * (1.0 - age) * P.color.a;
        }
        case 5u: { // Tracer: lateral falloff
            let lateral = abs(in.uv.y);
            let intensity = smoothstep(1.0, 0.0, lateral);
            alpha = intensity * (1.0 - age) * P.color.a;
        }
        default: {}
    }

    return vec4<f32>(rgb, clamp(alpha, 0.0, 1.0));
}
