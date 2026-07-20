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
pub struct MainGamePlugin
impl MainGamePlugin
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
pub struct SkyParamsUniform
pub struct SkyDomeMaterial
pub struct PrecipOverlayMaterial
pub struct GroundShadowUniform
pub struct CloudGroundShadowMaterial
impl SkyDomeMaterial
impl PrecipOverlayMaterial
impl CloudGroundShadowMaterial
```

### src/plugins/cloud_sky/mod.rs
```
pub struct CloudSkyPlugin
impl CloudSkyPlugin
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
pub struct CloudSkySettings
impl CloudSkySettings
impl CloudSkySettings
  pub fn sun_dir_and_day_factor(&self) → (Vec3, f32)
  pub fn wind_vec(&self) → Vec2
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
fn kelvin_to_rgb(kelvin: f32) -> vec3<f32> {
```

### src/plugins/cloud_sky/systems.rs
```
pub struct CloudSkyDome
pub struct PrecipOverlayQuad
pub struct CloudGroundShadowQuad
pub fn make_sky_params(s: &CloudSkySettings) → SkyParamsUniform
pub fn setup_cloud_sky(mut commands: Commands, settings: Res<CloudSkySettings>, mut meshes: ResMut<Assets<Mesh>>, mut images: ResMut<Assets<Image>>, mut sky_mats: ResMut<Assets<SkyDomeMaterial>>, mut precip_mats: ResMut<Assets<PrecipOverlayMaterial>>, mut shadow_mats: ResMut<Assets<CloudGroundShadowMaterial>>,)
pub fn follow_camera(camera_q: Query<&GlobalTransform, With<MainCamera>>, mut dome_q: Query<&mut Transform, (With<CloudSkyDome>, Without<PrecipOverlayQuad>)
pub fn auto_sun_temperature(mut settings: ResMut<CloudSkySettings>)
pub fn sync_sky_uniforms(settings: Res<CloudSkySettings>, mut sky_mats: ResMut<Assets<SkyDomeMaterial>>, mut precip_mats: ResMut<Assets<PrecipOverlayMaterial>>, mut shadow_mats: ResMut<Assets<CloudGroundShadowMaterial>>,)
pub fn sync_sun_light(settings: Res<CloudSkySettings>, mut light_q: Query<(&mut Transform, &mut DirectionalLight)
pub fn generate_cloud_shadow_image() → Image
```

### src/plugins/cloud_sky/ui.rs
```
pub fn cloud_sky_window(mut contexts: EguiContexts, mut ui_state: ResMut<UiState>, mut settings: ResMut<CloudSkySettings>,)
```

### src/plugins/main_scene_plugin.rs
```
pub struct MainScenePlugin
impl MainScenePlugin
```

### src/plugins/map_plugin/map_material_edit.rs
```
pub struct MapMaterialEditPlugin
pub struct MapMaterialEditState
impl MapMaterialEditPlugin
impl MapMaterialEditState
```

### src/plugins/map_plugin/map_plugin_ui.rs
```
pub fn configure_map_extent_gizmo(mut store: ResMut<GizmoConfigStore>)
pub fn draw_tree_bboxes(_gizmos: Gizmos, _data_res: Res<MapTree>, _lod_state: Res<MapLODState>, _tiles_query: Query<&TreeMapTile>, _ui_state: Option<Res<crate::ui_egui::UiState>>,)
pub fn draw_map_extent_gizmo(mut gizmos: Gizmos<MapExtentGizmoGroup>, data_res: Res<MapTree>, ui_state: Option<Res<crate::ui_egui::UiState>>,)
pub fn tree_navigator_ui(mut contexts: EguiContexts, data_res: Res<MapTree>, mut lod_state: ResMut<MapLODState>, tiles_query: Query<&TreeMapTile>, ui_state: Option<ResMut<crate::ui_egui::UiState>>,)
pub fn draw_reference_points_gizmos(mut gizmos: Gizmos, data_res: Res<MapTree>, lod_state: Res<MapLODState>, camera_query: Query<&Transform, With<Camera>>,)
```

### src/plugins/visual_fx/materials.rs
```
pub struct BillboardParams
pub struct AdditiveFxMaterial
pub struct BlendFxMaterial
pub enum FxKind
impl AdditiveFxMaterial
impl BlendFxMaterial
```

### src/plugins/visual_fx/mod.rs
```
pub struct VisualFXPlugin
impl VisualFXPlugin
```

### src/plugins/visual_fx/settings.rs
```
pub struct VfxSettings
impl VfxSettings
```

### src/plugins/visual_fx/ui.rs
```
pub fn vfx_controls_window(mut contexts: EguiContexts, mut ui_state: ResMut<UiState>, mut s: ResMut<VfxSettings>,)
```

### src/ui_egui.rs
```
pub struct UiEguiPlugin
pub struct UiState
impl UiEguiPlugin
impl UiState
impl UiState
  pub fn with_physics_debug() → Self
impl UiState
pub fn web_set_loading_status(_show: bool, _message: &str)
```

### src/utils/setup_debug_scene.rs
```
pub struct SetupDebugScenePlugin
pub struct DebugSceneGroundComponent
impl SetupDebugScenePlugin
```
