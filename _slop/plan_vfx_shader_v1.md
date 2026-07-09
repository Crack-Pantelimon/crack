# VFX Shaders v1 — Technical Plan

**Goal:** Lightweight, WebGL2-safe visual effects for car explosions, gun fire, and map
clouds, all owned by a new `VisualFXPlugin` with a runtime debug UI. No compute shaders.

## 0. Hard constraints (read first)

| Fact | Source | Consequence |
|------|--------|-------------|
| Web build uses `Backends::GL` (WebGL2) | [basic_app.rs:34](../crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs#L34) | **No compute shaders, no storage buffers, no `bevy_hanabi`.** All particles are CPU-spawned + GPU-animated in vertex/fragment WGSL only. |
| Bevy `0.19.0`, `bevy_egui 0.40` | [Cargo.toml:10](../crack_demo/demo_resolution_selector_web_bevy/Cargo.toml#L10) | Use `Material` trait + `AsBindGroup`; egui UI must run in `EguiPrimaryContextPass`. |
| `bevy_hanabi` already commented out | [Cargo.toml:23](../crack_demo/demo_resolution_selector_web_bevy/Cargo.toml#L23) | Confirmed banned. Do not re-enable. |
| No `.wgsl`, no `assets/` shader dir, no `Material` impls exist yet | repo scan | We are adding the first custom-material infrastructure. |
| egui panels must render in `EguiPrimaryContextPass`, **not** `Update` | [notifications.rs:38](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/notifications.rs#L38) | UI in `Update` → panic ("no primary egui context"). |

### Why this architecture (the "one animated billboard per effect" doctrine)

The lightest possible GPU particle system on WebGL2 is **not** thousands of quads. It is
**one camera-facing quad per effect**, whose entire animation (expansion, turbulence,
fade) is computed in the fragment shader from a single `age = globals.time - spawn_time`
value. This gives us:

- **Zero per-frame CPU** after spawn (no per-particle integration loop like the current
  [`draw_bullet_sparks`](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L353)).
- **1 draw call per active effect**, a handful on screen at once.
- Cheap fragment math (radial falloff + 2–3 octaves of hash noise, no texture fetches).

`globals.time` is available in WGSL via `#import bevy_render::globals::Globals` so the
material is **static after spawn** — we never touch the uniform again; the shader derives
its own clock. CPU work reduces to: spawn entity on event, despawn entity when
`age > lifetime`.

References for the techniques used (all fragment-only, texture-free):
- Value/fBm noise & radial falloff: *The Book of Shaders* ch.11–13; Inigo Quilez, "value
  noise" / "fbm" articles (iquilezles.org).
- Camera-facing billboard in the vertex shader: GPU Gems 2 ch.10 (point sprites), Bevy
  example `examples/shader/shader_instancing.rs` and `mesh_view_bindings::view`.
- Soft additive fireball / smoke: standard "polygon soft particle" trick (NVIDIA soft
  particles), simplified to a single blended quad.
- Custom `Material`: Bevy `examples/shader/shader_material.rs` (0.19), which shows
  `AsBindGroup` + `Material` + `AlphaMode`.

---

## 1. Module layout

New plugin directory:

```
src/plugins/visual_fx/
  mod.rs               # VisualFXPlugin: registers material, events, systems, UI
  settings.rs          # VfxSettings resource (toggles + sliders)
  materials.rs         # BillboardFxMaterial (AsBindGroup) + FxKind enum
  spawn.rs             # helpers: spawn_billboard_fx(...), VfxLifetime, despawn system
  car_explosion.rs     # listens CarExplosionEvent -> fireball + smoke burst + gizmo flash
  smoke_emitter.rs     # persistent black smoke from wrecks
  gun_fx.rs            # muzzle flash, muzzle smoke, tracer, hit spark burst
  clouds.rs            # sky cloud plane
  ui.rs                # "VFX Shaders Controls" egui window (EguiPrimaryContextPass)

assets/shaders/
  billboard_fx.wgsl    # one über-shader, branches on material.kind
  clouds.wgsl          # sky cloud plane shader
```

Register in [plugins/mod.rs](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/mod.rs):
`pub mod visual_fx;` and add `.add_plugins(VisualFXPlugin)` wherever the app assembles
plugins (same place `WeaponsPlugin` / `PedestriansPlugin` are added).

> **Asset path note:** the app currently loads assets over HTTP (`DATA_BASE_URL`) and has
> no local `assets/`. Confirm how `AssetServer` resolves `shaders/billboard_fx.wgsl` —
> either (a) add a local `assets/` folder served by Trunk, or (b) embed the shader with
> `load_internal_asset!` / `embedded_asset!` (recommended for web: no extra fetch, no
> path config). **Recommendation: embed via `embedded_asset!`** to avoid a network round
> trip and keep the WGSL in the binary.

---

## 2. Core: the billboard material

`materials.rs`:

```rust
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};

#[repr(u32)]
#[derive(Clone, Copy)]
pub enum FxKind {
    Fireball = 0,
    SmokePuff = 1,     // explosion smoke (light grey, fast)
    BlackSmoke = 2,    // lingering wreck smoke (dark, slow)
    MuzzleFlash = 3,   // additive
    SparkBurst = 4,    // additive radial streaks
    Tracer = 5,        // stretched additive quad (see gun_fx)
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BillboardFxMaterial {
    #[uniform(0)]
    pub params: BillboardParams,
}

#[derive(Clone, Copy, ShaderType)]
pub struct BillboardParams {
    pub color: Vec4,      // base tint incl. alpha multiplier
    pub spawn_time: f32,  // globals.time at spawn
    pub lifetime: f32,    // seconds
    pub start_radius: f32,
    pub end_radius: f32,  // for expanding fireball/smoke
    pub seed: f32,        // per-instance noise offset
    pub kind: u32,        // FxKind
    pub _pad: f32,
}

impl Material for BillboardFxMaterial {
    fn fragment_shader() -> ShaderRef { "embedded://.../billboard_fx.wgsl".into() }
    fn vertex_shader()   -> ShaderRef { "embedded://.../billboard_fx.wgsl".into() }
    fn alpha_mode(&self) -> AlphaMode {
        // Fireball/muzzle/spark/tracer = Add (glow); smoke = Blend.
        // Simplest v1: return AlphaMode::Blend for smoke materials, AlphaMode::Add for
        // the rest. Since alpha_mode is per-material, we set it from kind at build time.
        AlphaMode::Blend
    }
}
```

> **Blend vs Add:** `alpha_mode()` is fixed per material *type*, not per instance. Two
> clean options:
> 1. **Two material structs** — `AdditiveFxMaterial` (glow: fire, muzzle, spark, tracer)
>    and `BlendFxMaterial` (smoke) — identical fields, different `alpha_mode()`. Register
>    two `MaterialPlugin`s. **Recommended**, trivial and cheap.
> 2. One material, force everything to `Blend`, and do additive-ish look by keeping alpha
>    low and color bright. Cheaper to write, slightly worse fire look.
>
> Go with option 1.

Register: `app.add_plugins(MaterialPlugin::<AdditiveFxMaterial>::default())` and same for
blend. Both share one WGSL file (branch on `kind`).

Shared mesh: a single `1×1` quad in the XY plane, created once in a `Startup` system and
stored in a resource `VfxMeshes { quad: Handle<Mesh> }`. Every effect reuses it.

### billboard_fx.wgsl (vertex = billboard, fragment = procedural)

```wgsl
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
    let radius = mix(P.start_radius, P.end_radius, age);

    // Camera basis from the view transform (world_from_view).
    let right = view.world_from_view[0].xyz;
    let up    = view.world_from_view[1].xyz;

    let world = center + (right * pos.x + up * pos.y) * radius;

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
    let a = hash2(i); let b = hash2(i + vec2(1.0,0.0));
    let c = hash2(i + vec2(0.0,1.0)); let d = hash2(i + vec2(1.0,1.0));
    let u = f*f*(3.0-2.0*f);
    return mix(mix(a,b,u.x), mix(c,d,u.x), u.y);
}
fn fbm(p: vec2<f32>) -> f32 {
    return 0.5*vnoise(p) + 0.25*vnoise(p*2.03) + 0.125*vnoise(p*4.01);
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
            rgb = mix(vec3(1.0,0.3,0.0), vec3(1.0,0.95,0.4), heat);
            alpha = heat * (1.0 - age) * P.color.a;
        }
        case 1u, 2u: { // Smoke (light or black): billowy, fade in then out
            let puff = disk * (0.5 + 0.5 * n);
            let fade = smoothstep(0.0, 0.15, age) * (1.0 - age);
            alpha = puff * fade * P.color.a;
        }
        case 3u: { // Muzzle flash: sharp star, very short
            let star = pow(disk, 2.0) * (0.7 + 0.3*n);
            alpha = star * (1.0 - age) * P.color.a;
        }
        case 4u: { // Spark burst: radial streaks
            let ang = atan2(in.uv.y, in.uv.x);
            let streak = pow(abs(sin(ang * 9.0 + P.seed*6.28)), 8.0);
            alpha = streak * (1.0 - r) * (1.0 - age) * P.color.a;
        }
        default: {}
    }

    return vec4<f32>(rgb, clamp(alpha, 0.0, 1.0));
}
```

> **Import-path caveat:** WGSL import paths (`mesh_functions`, `mesh_view_bindings`,
> `globals`) drift between Bevy versions. Before coding, open the installed Bevy 0.19
> source under `~/.cargo` (`bevy_pbr/src/render/*.wgsl`, `bevy_render/src/globals.wgsl`)
> and copy the exact module paths + the exact `view` field names (`clip_from_world`,
> `world_from_view` may be named differently). This is the #1 thing that will fail to
> compile; validate it first with a single fireball before building the rest.

`spawn.rs` helper:

```rust
#[derive(Component)]
pub struct VfxLifetime { pub despawn_at: f64 } // seconds, absolute

pub fn spawn_billboard_fx(
    commands: &mut Commands,
    mats: &mut Assets<AdditiveFxMaterial>, // or blend variant
    meshes: &VfxMeshes,
    time: &Time,
    pos: Vec3,
    params: BillboardParams,
) {
    let despawn_at = time.elapsed_secs_f64() + params.lifetime as f64 + 0.05;
    let mat = mats.add(AdditiveFxMaterial { params });
    commands.spawn((
        Mesh3d(meshes.quad.clone()),
        MeshMaterial3d(mat),
        Transform::from_translation(pos),
        VfxLifetime { despawn_at },
    ));
}

pub fn despawn_expired_fx(
    mut commands: Commands, time: Res<Time>,
    q: Query<(Entity, &VfxLifetime)>,
) {
    let now = time.elapsed_secs_f64();
    for (e, l) in &q { if now >= l.despawn_at { commands.entity(e).despawn(); } }
}
```

> Each effect allocates one material asset and frees it on despawn (strong handle
> dropped). At a few effects/sec this is negligible. If we ever spam thousands, switch to
> a small pool of pre-made materials keyed by kind — not needed for v1.

---

## 3. Car explosion

### Trigger
The "explosion" moment is when a car crosses `CAR_DISABLE_HP` in
[`disable_low_health_cars`](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/car_disable.rs#L17).
That system already runs exactly once per car at the death transition. Add an event emit
there (one line, no behavior change):

```rust
commands.trigger(CarExplosionEvent { position: car_gt.translation() });
```

`CarExplosionEvent { position: Vec3 }` is defined in `visual_fx` and registered as an
observer event. Also start a `SmokeEmitter` on the wreck entity for the lingering smoke.

### 3a. Gizmo flash (damage-radius spheres) — required, cheapest
A short-lived resource `ExplosionFlashes(Vec<{pos, t}>)`, drawn each frame in `Update`
via `Gizmos` for ~0.4s:

```rust
// red (inner kill radius) -> orange (mid) -> yellow (outer)
gizmos.sphere(pos, 2.0, Color::srgb(1.0, 0.1, 0.0));
gizmos.sphere(pos, 4.0, Color::srgb(1.0, 0.5, 0.0));
gizmos.sphere(pos, 6.0, Color::srgb(1.0, 0.9, 0.1));
```
Radii should match whatever explosion damage radius exists (there is none yet — these are
cosmetic-only for v1; wire to real radii when damage lands). GPU cost: 3 wireframe spheres
for 0.4s ≈ free.

### 3b. Fireball shader (kind=Fireball, additive)
Spawn 1 (or 2 offset) fireball billboards:
```rust
BillboardParams { color: vec4(1,0.6,0.1,1.0), spawn_time: now, lifetime: 0.6,
    start_radius: 1.0, end_radius: 4.0, seed: rand(), kind: Fireball as u32, _pad:0 }
```
Expands 1→4m over 0.6s, hot core fading out. **Cost:** 1 additive quad, ~0.6s.

### 3c. Explosion smoke shader (kind=SmokePuff, blend)
Spawn 3–4 light-grey puffs with random seeds/offsets, `lifetime: 1.5`,
`start_radius: 1.5 → end_radius: 5.0`, rising slightly (give the entity a small upward
velocity via an optional `VfxDrift(Vec3)` component + a tiny integrate system, or bake the
rise into the shader by offsetting center with `up * age`). **Cost:** ~4 blended quads,
1.5s.

### 3d. Lingering black smoke (kind=BlackSmoke, blend) — `smoke_emitter.rs`
`SmokeEmitter { next: f32, until: f32 }` on the wreck. A system every ~0.4s spawns one
dark puff at the wreck top that rises and fades over ~2.5s:
```rust
BillboardParams { color: vec4(0.15,0.15,0.15,0.6), lifetime: 2.5,
    start_radius: 0.8, end_radius: 3.0, kind: BlackSmoke as u32, .. }
```
Emitter lives ~15s then stops (or until wreck despawns). **Cost:** 1 new quad every 0.4s,
≤~6 alive at once.

---

## 4. Gun fire effects

Hook point: [`fire_gun_observer`](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L142)
already computes `muzzle`, `impact`, `dir`, and `is_person`. It currently pushes gizmo
tracers + physics sparks. Plan:

### 4a. Keep gizmos at alpha 0.3 (required)
In [`draw_shot_tracers`](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L333)
and [`draw_bullet_sparks`](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L353),
multiply every gizmo color's alpha so the **max** alpha is `0.3`. E.g. the tracer line
`Color::srgba(1.0, 0.9, 0.3, 0.3)`, spheres likewise; sparks already fade — clamp their
alpha to `0.3`. Gate this behind `VfxSettings.gun_gizmos` so it can be toggled. Gizmos stay
as the cheap always-available fallback.

### 4b. Bullet tracer shader (kind=Tracer, additive) — better than gizmo line
A **stretched** billboard: instead of the square quad, build the tracer quad in the vertex
shader as a screen-facing ribbon from `muzzle`→`impact`. Simplest implementation without a
new mesh: spawn the billboard at the segment midpoint, scale X = segment length, Y = ~4cm,
and orient the quad along the beam in the vertex shader (pass beam direction as a param).
For v1 the cheapest correct version: a thin **world-space quad** oriented by the beam
using a dedicated tiny mesh, or reuse the billboard but with anisotropic radius
(`start_radius` X vs Y). Recommendation: add a `TracerParams` path (beam endpoints in
uniform) so the vertex shader places the 4 corners directly:
```
corner = mix(muzzle, impact, uv.x01) + camera_right_perp * uv.y * width
```
`lifetime: 0.05`, bright core color `vec4(1,0.95,0.6,1)` fading. **Cost:** 1 additive quad
per shot, 0.05s. Visibly crisper than the gizmo line (soft edges, HDR core).

### 4c. Hit spark burst shader (kind=SparkBurst, additive) — replaces physics sparks
Replace the 3 CPU-simulated `BulletSpark` spheres per hit with **one** SparkBurst quad at
the impact point, `lifetime: 0.15`, `start_radius: 0.05 → end_radius: 0.5`, color yellow
for world / red (`is_person`) for flesh. The radial-streak fragment branch already draws
multiple spark rays. **Cost:** 1 additive quad, 0.15s, and it *removes* the per-frame
`draw_bullet_sparks` integration loop entirely (net CPU win). Keep the old physics sparks
behind the gizmo toggle as fallback.

### 4d. Muzzle fire sparks (kind=MuzzleFlash, additive)
One tiny additive quad at `muzzle`, `lifetime: 0.04`, `radius ~0.15`, bright white-yellow
star. Spawned every shot. **Cost:** trivial.

### 4e. Muzzle smoke (kind=SmokePuff, blend)
One small grey puff at `muzzle`, `lifetime: 0.5`, `start_radius 0.05 → 0.4`, low alpha,
drifting up. Optional per-shot; consider only spawning it every Nth shot to avoid clutter
on automatic fire (`VfxSettings.muzzle_smoke_every`). **Cost:** 1 blended quad, 0.5s.

All 4b–4e spawns go through a `GunFxEvent { muzzle, impact, is_person, is_miss }` emitted
from `fire_gun_observer`, handled by an observer in `gun_fx.rs`, each branch gated by its
`VfxSettings` toggle. Keeps `weapon_shooting.rs` clean (one event trigger) and all VFX in
`visual_fx`.

---

## 5. Map clouds (`clouds.rs`)

A single large horizontal **plane** (e.g. 4000×4000m) placed high above the map (cloud
altitude), with a custom unlit `CloudMaterial` (own `MaterialPlugin`, `AlphaMode::Blend`,
`cull_mode: None`) whose fragment shader is scrolling fBm:

```wgsl
@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let uv = in.world_position.xz * u.scale + globals.time * u.wind;
    var d = fbm(uv);                        // 3 octaves, defined as above
    d = smoothstep(u.coverage, 1.0, d);     // coverage slider carves clouds
    return vec4<f32>(u.tint.rgb, d * u.opacity);
}
```
Uniforms: `scale`, `wind: vec2`, `coverage`, `opacity`, `tint`. All driven by UI sliders.

**Why a plane, not a skybox/volumetrics:** one quad, one fragment pass, no raymarch, no
extra render target. The existing skybox already draws a static `skybox_clouds.png`
([main_scene_plugin.rs:30](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/main_scene_plugin.rs#L30));
this plane adds cheap *animated* clouds on top. fBm at 3 octaves is ~a dozen ALU ops/pixel;
on a large plane the cost is fill-rate bound but still one of the cheapest possible ways to
get moving clouds. If fill-rate is a concern on weak GPUs, render the plane at a smaller
size / higher altitude or drop to 2 octaves (slider-controllable).

**Alternative considered & rejected:** volumetric raymarched clouds (GPU Gems 3 / Horizon
approach) — far too heavy for WebGL2 weak machines. fBm plane is the right call.

---

## 6. `VfxSettings` + the "VFX Shaders Controls" window

`settings.rs`:
```rust
#[derive(Resource)]
pub struct VfxSettings {
    // category master toggles (default true)
    pub car_fireball: bool,
    pub car_smoke: bool,
    pub car_black_smoke: bool,
    pub gun_gizmos: bool,     // keep gizmos (alpha 0.3)
    pub gun_tracer: bool,
    pub gun_hit_sparks: bool,
    pub gun_muzzle_flash: bool,
    pub gun_muzzle_smoke: bool,
    pub clouds: bool,
    // sliders
    pub fireball_lifetime: f32,
    pub fireball_radius: f32,
    pub smoke_lifetime: f32,
    pub smoke_opacity: f32,
    pub tracer_width: f32,
    pub spark_count_scale: f32,
    pub cloud_coverage: f32,
    pub cloud_opacity: f32,
    pub cloud_wind: f32,
    pub cloud_scale: f32,
}
impl Default for VfxSettings { /* all bools true, sensible slider defaults */ }
```
Every spawn system reads this resource and early-returns / scales by it. Cloud sliders push
into the `CloudMaterial` uniform each frame (or only when changed).

`ui.rs` — **MUST run in `EguiPrimaryContextPass`** (mirror
[notifications.rs:38](../crack_demo/demo_resolution_selector_web_bevy/src/plugins/notifications.rs#L38)):
```rust
pub fn vfx_controls_window(mut ctx: EguiContexts, mut s: ResMut<VfxSettings>) {
    let Ok(ctx) = ctx.ctx_mut() else { return; };   // 0.40 returns Result
    egui::Window::new("VFX Shaders Controls")
        .default_open(false)
        .show(ctx, |ui| {
            ui.collapsing("Car explosion", |ui| {
                ui.checkbox(&mut s.car_fireball, "Fireball");
                ui.checkbox(&mut s.car_smoke, "Explosion smoke");
                ui.checkbox(&mut s.car_black_smoke, "Wreck black smoke");
                ui.add(egui::Slider::new(&mut s.fireball_radius, 1.0..=8.0).text("Fireball radius"));
                ui.add(egui::Slider::new(&mut s.smoke_opacity, 0.0..=1.0).text("Smoke opacity"));
            });
            ui.collapsing("Gun", |ui| {
                ui.checkbox(&mut s.gun_gizmos, "Gizmos (alpha 0.3)");
                ui.checkbox(&mut s.gun_tracer, "Tracer shader");
                ui.checkbox(&mut s.gun_hit_sparks, "Hit spark burst");
                ui.checkbox(&mut s.gun_muzzle_flash, "Muzzle flash");
                ui.checkbox(&mut s.gun_muzzle_smoke, "Muzzle smoke");
                ui.add(egui::Slider::new(&mut s.tracer_width, 0.01..=0.2).text("Tracer width"));
            });
            ui.collapsing("Clouds", |ui| {
                ui.checkbox(&mut s.clouds, "Enabled");
                ui.add(egui::Slider::new(&mut s.cloud_coverage, 0.0..=1.0).text("Coverage"));
                ui.add(egui::Slider::new(&mut s.cloud_opacity, 0.0..=1.0).text("Opacity"));
                ui.add(egui::Slider::new(&mut s.cloud_wind, 0.0..=0.05).text("Wind"));
            });
        });
}
```
Register: `app.add_systems(EguiPrimaryContextPass, vfx_controls_window);`

---

## 7. `VisualFXPlugin::build` wiring

```rust
impl Plugin for VisualFXPlugin {
    fn build(&self, app: &mut App) {
        app
          .init_resource::<VfxSettings>()
          .add_plugins(MaterialPlugin::<AdditiveFxMaterial>::default())
          .add_plugins(MaterialPlugin::<BlendFxMaterial>::default())
          .add_plugins(MaterialPlugin::<CloudMaterial>::default())
          .init_resource::<ExplosionFlashes>()
          .add_event::<CarExplosionEvent>()   // or observer form
          .add_event::<GunFxEvent>()
          .add_observer(car_explosion_observer)
          .add_observer(gun_fx_observer)
          .add_systems(Startup, (setup_vfx_meshes, setup_clouds))
          .add_systems(Update, (
              despawn_expired_fx,
              tick_smoke_emitters,
              draw_explosion_flashes,   // gizmos
              tick_vfx_drift,           // optional rising motion
              sync_cloud_uniforms,
          ))
          .add_systems(EguiPrimaryContextPass, vfx_controls_window);
        // embed shaders
        embedded_asset!(app, "shaders/billboard_fx.wgsl");
        embedded_asset!(app, "shaders/clouds.wgsl");
    }
}
```

---

## 8. Performance budget summary

| Effect | Draw calls | Blend | Lifetime | Fragment cost | Notes |
|--------|-----------|-------|----------|---------------|-------|
| Explosion gizmo spheres | 3 (lines) | — | 0.4s | ~0 | wireframe |
| Fireball | 1–2 | Add | 0.6s | fbm 3-oct | expands |
| Explosion smoke | 3–4 | Blend | 1.5s | fbm 3-oct | rising |
| Wreck black smoke | ≤6 alive | Blend | 2.5s ea | fbm 3-oct | 1 new / 0.4s |
| Tracer | 1/shot | Add | 0.05s | trivial | ribbon |
| Hit spark burst | 1/hit | Add | 0.15s | streaks | replaces physics sparks (CPU win) |
| Muzzle flash | 1/shot | Add | 0.04s | trivial | |
| Muzzle smoke | ~1/shot | Blend | 0.5s | fbm | throttle on auto fire |
| Clouds | 1 plane | Blend | ∞ | fbm 3-oct | fill-rate bound; reducible to 2 oct |

Worst realistic on-screen concurrent load: one explosion (≈10 quads) + a few tracers +
clouds = well under ~20 transparent quads. Every quad is unlit, texture-free, ≤~12 ALU
ops/pixel. This is orders of magnitude below a GPU-compute particle system and safe for
WebGL2 on weak laptops.

---

## 9. Build order (do it in this sequence to de-risk)

1. **Validate the WGSL import paths + billboard math** with ONE hardcoded fireball on
   keypress. This is where compile failures live (step 2 caveat). Nothing else until a
   quad renders and expands.
2. `spawn.rs` helper + `despawn_expired_fx` + `VfxMeshes`.
3. Car explosion (gizmo flash + fireball + smoke + black smoke emitter).
4. Gun fx event + tracer/spark/muzzle; dim existing gizmos to 0.3.
5. Cloud plane + material.
6. `VfxSettings` + egui window; thread toggles/sliders through every system.

Steps 3–5 are independent and can be parallelized once step 1–2 land.
