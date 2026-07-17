We are using bevy 0.19 - there is no more `despawn_recursive()`, just `despawn()` - when in doubt, use `cargo doc into a temp dir` and read the documentation from disk.

Check the code builds by running `cargo check --package ...` from this directory. 

When working on a binary command, you can run it with `cd ... && bash timeout 15s cargo run --bin ... --package ...` from this directory, to verify the code does not crash.

This code is supposed to be cross-platform, to work on both browser and native hosts. That means:
- do not use std::Instant::now() as it panics on wasm
- do not use threads. Intead, we will declare API routes to be used in the web worker, see `crack_demo/web_worker` for the web implementation and `crack_demo/thread_worker` for the host implementation.
- do not do heavy computation in bevy; make an async task and call into the worker using a `declare_api_method_group!` declaration


## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## .

### index.clouds.html
```
title: Crack! - Clouds
```

## src

### src/main_game_plugin.rs
```
pub struct MainGamePlugin  :3-3
impl MainGamePlugin  :5-31
```

### src/plugins/cloud_sky/ground_shadow.wgsl
```
fn vertex(@location(0) pos: vec3<f32>,
let model = get_world_from_local(inst);
let world_pos = (model * vec4<f32>(pos, 1.0)).xyz;
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
let intensity = u.params.x;
let uv = in.world_pos.xz * u.params.y + u.wind.xy * globals.time;
let cloud = textureSample(shadow_tex, shadow_smp, uv).r;
let alpha = cloud * intensity;
```

### src/plugins/cloud_sky/materials.rs
```
pub struct SkyParamsUniform  :10-19
pub struct SkyDomeMaterial  :23-26
pub struct PrecipOverlayMaterial  :67-70
pub struct GroundShadowUniform  :106-111
pub struct CloudGroundShadowMaterial  :116-122
impl SkyDomeMaterial  :28-62
impl PrecipOverlayMaterial  :72-103
impl CloudGroundShadowMaterial  :124-149
```

### src/plugins/cloud_sky/mod.rs
```
pub struct CloudSkyPlugin  :19-19
impl CloudSkyPlugin  :21-39
```

### src/plugins/cloud_sky/precip_overlay.wgsl
```
fn vertex(@location(0) pos: vec3<f32>,
let model = get_world_from_local(inst);
let world_pos = (model * vec4<f32>(pos, 1.0)).xyz;
fn hash2(p: vec2<f32>) -> f32 {
fn rain(rd: vec3<f32>, intensity: f32, day: f32) -> f32 {
let yaw = atan2(rd.x, rd.z);
let slant = (u.wind.x + u.wind.y) * 6.0 + 0.15;
let p_yaw = yaw + rd.y * slant;
let cols = 70.0;
let col_id = floor(p_yaw * cols);
let rnd = hash2(vec2<f32>(col_id, 3.7));
let cx = abs(fract(p_yaw * cols) - 0.5);
let thin = smoothstep(0.10, 0.03, cx);
let speed = 2.5 + rnd * 2.0;
let span = 4.0 + rnd * 3.0;
```

### src/plugins/cloud_sky/settings.rs
```
pub struct CloudSkySettings  :5-37
impl CloudSkySettings  :39-57
impl CloudSkySettings  :59-83
  pub fn sun_dir_and_day_factor(&self) → (Vec3, f32)  :62-62
  pub fn wind_vec(&self) → Vec2  :79-79
```

### src/plugins/cloud_sky/skybox_clouds.wgsl
```
fn vertex(@location(0) pos: vec3<f32>,
let model = get_world_from_local(inst);
let world_pos = (model * vec4<f32>(pos, 1.0)).xyz;
fn hash2(p: vec2<f32>) -> f32 {
fn vnoise(p: vec2<f32>) -> f32 {
let i = floor(p);
let f = fract(p);
let a = hash2(i);
let b = hash2(i + vec2<f32>(1.0, 0.0));
let c = hash2(i + vec2<f32>(0.0, 1.0));
let d = hash2(i + vec2<f32>(1.0, 1.0));
let w = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
fn fbm(p_in: vec2<f32>, octaves: f32) -> f32 {
let rot = mat2x2<f32>(vec2<f32>(0.8, -0.6), vec2<f32>(0.6, 0.8));
fn sky_color(rd: vec3<f32>, sun_dir: vec3<f32>, day: f32, overcast: f32) -> vec3<f32> {
```

### src/plugins/cloud_sky/systems.rs
```
pub struct CloudSkyDome  :15-15
pub struct PrecipOverlayQuad  :18-18
pub struct CloudGroundShadowQuad  :21-21
pub fn make_sky_params(s: &CloudSkySettings) → SkyParamsUniform  :24-43
pub fn setup_cloud_sky(mut commands: Commands, settings: Res<CloudSkySettings>, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut sky_mats: ResMut<Assets<SkyDomeMaterial>>, mut precip_mats: ResMut<Assets<PrecipOverlayMaterial>>, mut shadow_mats: ResMut<Assets<CloudGroundShadowMaterial>>,)  :54-98
pub fn follow_camera(camera_q: Query<&GlobalTransform, With<MainCamera>>, mut dome_q: Query<&mut Transform, (With<CloudSkyDome>, Without<PrecipOverlayQuad>)  :101-118
pub fn sync_sky_uniforms(settings: Res<CloudSkySettings>, mut sky_mats: ResMut<Assets<SkyDomeMaterial>>, mut precip_mats: ResMut<Assets<PrecipOverlayMaterial>>, mut shadow_mats: ResMut<Assets<CloudGroundShadowMaterial>>,)  :121-141
pub fn sync_sun_light(settings: Res<CloudSkySettings>, mut light_q: Query<(&mut Transform, &mut DirectionalLight)  :145-161
pub fn generate_cloud_shadow_image() → Image  :205-248
```

### src/plugins/cloud_sky/ui.rs
```
pub fn cloud_sky_window(mut contexts: EguiContexts, mut ui_state: ResMut<UiState>, mut settings: ResMut<CloudSkySettings>,)  :9-68
```

### src/plugins/debug_picker.rs
```
pub struct DebugPickerPlugin  :24-24
pub struct DebugPickerState  :38-41
pub struct PickResult  :43-48
pub enum PickKind  :50-76
impl DebugPickerPlugin  :26-35
```

### src/plugins/main_scene_plugin.rs
```
pub struct MainScenePlugin  :6-6
impl MainScenePlugin  :8-20
```

### src/plugins/map_plugin/map_material_edit.rs
```
pub struct MapMaterialEditPlugin  :6-6
pub struct MapMaterialEditState  :29-41
impl MapMaterialEditPlugin  :8-26
impl MapMaterialEditState  :43-57
```

### src/plugins/map_plugin/map_plugin_ui.rs
```
pub fn configure_map_extent_gizmo  :11-15
pub fn draw_tree_bboxes  :17-25
pub fn draw_map_extent_gizmo  :62-77
pub fn tree_navigator_ui  :79-206
pub fn draw_reference_points_gizmos  :208-232
```

### src/plugins/traffic/despawn.rs
```
pub fn despawn_traffic_cars  :8-77
```

### src/plugins/visual_fx/materials.rs
```
pub struct BillboardParams  :17-26
pub struct AdditiveFxMaterial  :30-33
pub struct BlendFxMaterial  :64-67
pub enum FxKind  :7-14
impl AdditiveFxMaterial  :35-60
impl BlendFxMaterial  :69-94
```

### src/plugins/visual_fx/mod.rs
```
pub struct VisualFXPlugin  :27-27
impl VisualFXPlugin  :29-57
```

### src/plugins/visual_fx/settings.rs
```
pub struct VfxSettings  :4-25
impl VfxSettings  :27-49
```

### src/plugins/visual_fx/ui.rs
```
pub fn vfx_controls_window(mut contexts: EguiContexts, mut ui_state: ResMut<UiState>, mut s: ResMut<VfxSettings>,)  :6-53
```

### src/ui_egui.rs
```
pub struct UiEguiPlugin  :7-7
pub struct UiState  :23-44
impl UiEguiPlugin  :9-20
impl UiState  :45-70
impl UiState  :71-96
  pub fn with_physics_debug() → Self  :72-72
impl UiState  :98-102
pub fn web_set_loading_status(_show: bool, _message: &str)  :493-528
```

### src/utils/setup_debug_scene.rs
```
pub struct SetupDebugScenePlugin  :15-15
pub struct DebugSceneGroundComponent  :26-26
impl SetupDebugScenePlugin  :17-22
```
