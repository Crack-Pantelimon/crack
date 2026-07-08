bugs.md#1-14
- when in pedestrian control and in car control mode, we capture the mouse. when we are in the mode that we must capture the mouse, we try to recenter the mouse also to the center of the screen (to avoid runoff) and the camera is controlled by mouse without having to click-drag on the camera. In any other modes remaining (freecam mode, the debug scene modes) we do not capture the camera. Even in these modes, when the camera is captured, pressing escape will release it. Clicking on a game area (outside of the menu and uis) will capture it again. 

- 3d audio left and right is wrong, switch them around

- ui: chat window and other ui elements do not block keyboard actions from happening in the main game. all keyboard and mouse events should check we are not inside a ui window. 

- death animation for player doesn't play, their thing is just despawned. we should spawn another pedestrian glb there of the same type in the same transform as the dead one we despawned but with the death animation on it, with no looping, and keep the mesh in the last animation position (dead on the ground) for 10 more seconds and only then despawn this death prop. 

- melee attacks don't work online - flash on screen for 0.1s a yellow gizmo showing the attack area of the melee attack (a high cube of 1x1x2m in front of the character's hips that goes from under their feet to above their head) and do this for all types of pedestrians: player controlled, traffic controlled, multiplayer controlled.  and then to register melee hit, get physics intersection between the collider for that cube in that position, and the enemy we could hit (use the spatial query feature to get only things that we can hit - pedestrians and cars -  at most 2 cube sides away from the cube center)

- traffic pedestrian ai - with melee, it wants to climb on top of the thing it wants to hit (pedestrian or car) instead of hitting at a comfortable distance. When the target is near, it should aim at the center of the target and start hitting with melee weapon until target is dead, not climb on top of it. 

- use SQL api in the bevy app to save and load the random UserSecretKey and pass that to all our network managers. multiple tabs will use differnet NodeSecretKey randomly every time, but the same UserSecretKey will be used for all their sessions.
This is a set of 7 distinct bugs from _slop/bugs.md. Before I plan, let me explore the relevant systems in parallel to understand how each area currently works.

Agent:Explore input/mouse/UI systems
IN
Explore the Bevy game codebase at /home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/ to understand these systems. I need thorough findings with exact file paths and line numbers.

1. MOUSE CAPTURE & CAMERA CONTROL: How is the mouse cursor grab/capture currently handled? Search for `CursorGrabMode`, `cursor_options`, `grab`, mouse capture. How does the camera get controlled by the mouse (look for mouse motion reading, `MouseMotion`, camera pitch/yaw)? Is it click-drag based or captured? What are the game control states (look at GameControlState enum in src/plugins/states/mod.rs — Freecam, pedestrian control, car/driving control, debug scenes). Which states currently capture the mouse?

2. UI BLOCKING INPUT: Find all places where keyboard (`ButtonInput<KeyCode>`, `keys.pressed`, `keys.just_pressed`) and mouse (`ButtonInput<MouseButton>`) are read for gameplay actions. Do any of them check whether an egui UI window has focus / wants pointer or keyboard input (look for `ctx.wants_keyboard_input()`, `wants_pointer_input()`, `is_pointer_over_area()`, EguiContexts usage)? Is there a shared UiState or helper for "is mouse over UI"? Look at src/ui_egui.rs and how egui contexts are queried.

Report: exact systems (function names + file paths), how input is currently gated (or not), the GameControlState variants, and where mouse grab is set. Show representative code snippets.
Agent:Explore combat/death/audio systems
IN
Explore the Bevy game codebase at /home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/ to understand these systems. I need thorough findings with exact file paths and line numbers.

1. DEATH ANIMATION: How do pedestrians/players die? Look at src/plugins/pedestrian_ai/combat.rs (apply_damage_observer, start_ai_death_animation, tick_dying), src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs (player_death_to_freecam), and the Dying component in src/plugins/pedestrian_ai/faction.rs. For the PLAYER specifically: what happens when the controlled character dies? Is the mesh despawned immediately without a death animation playing? How are pedestrian GLB models spawned (SpawnPedestrianEvent in src/plugins/pedestrians/spawn_pedestrian.rs) and how are animations played (src/plugins/pedestrians/animation.rs — TargetAnimation, one-shot vs looping, ManualAnimation, ActiveOneShot)? How would one spawn a "death prop" — a pedestrian glb of a given type with a non-looping death animation frozen at the last frame?

2. MELEE ATTACKS: Look at src/plugins/weapons/weapon_shooting.rs (PendingMeleeHit, tick_pending_melee_hits, fire_gun_observer) and src/plugins/weapons/weapon_attach.rs. How is a melee attack currently registered/detected? Does it work over multiplayer/online? Look at src/plugins/pedestrian_ai/combat.rs ai_combat for how AI does melee. How are hits registered — spatial query, raycast, distance? What is WeaponKind::is_melee. How does DamageEvent work?

3. TRAFFIC PEDESTRIAN AI MELEE: Look at src/plugins/pedestrian_ai/movement_ai.rs (ai_movement) and how AI approaches targets. Why would a melee AI climb on top of its target instead of stopping at a comfortable distance? Find the movement/steering logic and stopping distance for melee vs ranged.

4. 3D AUDIO: Search for spatial audio, `SpatialListener`, `PlaybackSettings`, audio panning, `SpatialScale`, left/right ear. Where is 3D positional audio set up? Find the listener and emitter setup.

Report exact function names, file paths, line numbers, and representative code snippets for each.
Bash Search egui input gating
IN
cd /home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/ && grep -rn "wants_keyboard_input\|wants_pointer_input\|is_pointer_over_area\|EguiContexts\|EguiContext" --include="*.rs" src/ 2>/dev/null

OUT
src/ui_egui.rs:90:    mut contexts: EguiContexts,
src/plugins/map_plugin/map_plugin_ui.rs:2:use bevy_egui::{EguiContexts, egui};
src/plugins/map_plugin/map_plugin_ui.rs:17:    mut contexts: EguiContexts,
src/plugins/map_plugin/map_material_edit.rs:3:use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
src/plugins/map_plugin/map_material_edit.rs:61:    mut contexts: EguiContexts,
src/plugins/game_freecam/camera_controls.rs:6:use bevy_egui::EguiContexts;
src/plugins/game_freecam/camera_controls.rs:83:    mut contexts: EguiContexts,
src/plugins/game_freecam/camera_controls.rs:101:        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
src/plugins/cars_driving/driving_plugin/speedometer_ui.rs:2:use bevy_egui::{EguiContexts, egui};
src/plugins/cars_driving/driving_plugin/speedometer_ui.rs:10:    mut contexts: EguiContexts,
src/egui_theme.rs:8:use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
src/egui_theme.rs:28:fn setup_egui_theme(mut contexts: EguiContexts, mut done: Local<bool>) {
src/plugins/cars_driving/click_spawn_select_controls.rs:6:use bevy_egui::EguiContexts;
src/plugins/cars_driving/click_spawn_select_controls.rs:14:    mut contexts: EguiContexts,
src/plugins/cars_driving/click_spawn_select_controls.rs:19:    if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:12:use bevy_egui::EguiContexts;
src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:66:    mut contexts: EguiContexts,
src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:137:        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:5:use bevy_egui::{EguiContexts, egui};
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:31:    mut contexts: EguiContexts,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:38:        if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:70:    mut contexts: EguiContexts,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:537:    mut contexts: EguiContexts,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:660:    mut contexts: EguiContexts,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:777:    mut contexts: EguiContexts,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:788:        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:817:    mut contexts: EguiContexts,
src/plugins/traffic/debug_ui.rs:2:use bevy_egui::{EguiContexts, egui};
src/plugins/traffic/debug_ui.rs:11:    mut contexts: EguiContexts,
src/plugins/notifications.rs:2:use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
src/plugins/notifications.rs:89:fn render_notifications(mut contexts: EguiContexts, active: Res<ActiveNotifications>) {
src/bin/pedestrian_v2.rs:9:use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
src/bin/pedestrian_v2.rs:128:    mut contexts: EguiContexts,
src/bin/pedestrian_v2.rs:131:        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
src/bin/pedestrian_v2.rs:237:    mut contexts: EguiContexts,
src/plugins/cars_driving/driving_plugin/camera_follow.rs:4:use bevy_egui::EguiContexts;
src/plugins/cars_driving/driving_plugin/camera_follow.rs:12:    mut contexts: EguiContexts,
src/plugins/cars_driving/driving_plugin/camera_follow.rs:28:        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
src/plugins/pedestrian_ai/debug_ui.rs:4:use bevy_egui::EguiContexts;
src/plugins/pedestrian_ai/debug_ui.rs:19:    mut contexts: EguiContexts,
src/plugins/network/global_chat_ui.rs:3:use bevy_egui::{EguiContexts, egui};
src/plugins/network/global_chat_ui.rs:43:    mut contexts: EguiContexts,
src/plugins/audio/mod.rs:14:use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
src/plugins/audio/mod.rs:288:    mut contexts: EguiContexts,
src/plugins/audio/mod.rs:322:    mut contexts: EguiContexts,
src/plugins/audio/mod.rs:329:        if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
src/plugins/audio/mod.rs:395:    mut contexts: EguiContexts,
src/plugins/geojson.rs:3:use bevy_egui::{EguiContexts, egui};
src/plugins/geojson.rs:109:    mut contexts: EguiContexts,
src/plugins/geojson.rs:304:    mut contexts: EguiContexts,
src/plugins/geojson.rs:974:fn osm_overlay_ui_system(mut contexts: EguiContexts, mut osm_overlay: ResMut<OsmOverlayState>) {
src/plugins/network/multiplayer_plugin.rs:3:use bevy_egui::{EguiContexts, egui};
src/plugins/network/multiplayer_plugin.rs:1566:    mut contexts: EguiContexts,
src/plugins/network/multiplayer_plugin.rs:1674:    mut contexts: EguiContexts,
src/bin/turf_war.rs:15:use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
src/bin/turf_war.rs:136:    mut contexts: EguiContexts,
Agent:Explore network identity & SQL storage
IN
Explore the codebase at /home/p/VIDOEGAME/crack/ to understand how network identity keys and SQL storage work. I need thorough findings with exact file paths and line numbers.

1. USER SECRET KEY / NETWORK IDENTITY: In packages/net_crackpipe/src/, find UserIdentitySecrets (src/user_identity.rs), UserIdentity, NodeIdentity, and how the network managers (GlobalMatchmaker in src/global_matchmaker.rs, MainNode in src/main_node.rs) are initialized with a secret key. Look for `UserIdentitySecrets::generate()`, `SecretKey`, and how these are created/passed when the Bevy app starts the network. Where in the bevy app (crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/) is the GlobalMatchmaker or network runtime created? Look at src/plugins/network/mod.rs (NetworkRuntime, NetworkPlugin) and src/plugins/network/multiplayer_plugin.rs. How is the UserSecretKey currently generated (randomly each session?) and passed to network managers? Distinguish UserSecretKey (should persist across sessions/tabs) vs NodeSecretKey (random per session/tab).

2. SQL API: Find the SQL/storage API available in the bevy app. Look at packages/storage_crackhouse/src/ (api.rs, impl_rusqulite.rs, models.rs, types.rs) and packages/api_asscrack/. How does the bevy app call SQL — is there a client (CrackClient in src/plugins/crack_plugin/mod.rs)? What API methods exist for key/value or generic storage? Look at packages/net_crackpipe/src/api/api_declarations.rs and how methods like MatchRow2/GameStateRow2 are declared and called via ClientApiManager (src/api/client_api_manager.rs, call_method). Is there an existing generic key-value store or settings table? How would one save/load a small secret key blob? Does the storage work in WASM/browser (localStorage?) or only native sqlite?

Report exact struct/function names, file paths, line numbers, how identity secret keys flow into network managers, and what SQL/storage APIs are available to persist a small blob. Show representative code snippets.
Read ui_egui.rs
Read weapon_shooting.rs
Bash Pedestrian controller input and scheduling
IN
cd /home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/ && grep -rn "orbit_camera_input\|follow_camera\|add_systems\|keys.pressed\|keyboard.pressed\|keys.just_pressed\|keyboard.just_pressed\|ButtonInput<KeyCode>\|ButtonInput<MouseButton>" src/plugins/pedestrians/pedestrian_controller_plugin/ 2>/dev/null

OUT
src/plugins/pedestrians/pedestrian_controller_plugin/locomotion.rs:29:        app.add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/locomotion.rs:43:        .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs:36:pub fn orbit_camera_input(
src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs:37:    mouse_buttons: Res<ButtonInput<MouseButton>>,
src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs:52:pub fn follow_camera(
src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs:152:    keys: Res<ButtonInput<KeyCode>>,
src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs:157:    if !keys.just_pressed(KeyCode::Escape) {
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:27:    mouse_button: Res<ButtonInput<MouseButton>>,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:178:    keys: Res<ButtonInput<KeyCode>>,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:193:    if !keys.just_pressed(KeyCode::KeyF) {
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:561:    keys: Res<ButtonInput<KeyCode>>,
src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:565:    if !keys.just_pressed(KeyCode::KeyF) {
src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:63:    mouse: Res<ButtonInput<MouseButton>>,
src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:64:    keys: Res<ButtonInput<KeyCode>>,
src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:303:    let reload_pressed = keys.just_pressed(KeyCode::KeyR);
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:41:use camera::{follow_camera, orbit_camera_input};
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:314:            .add_systems(Update, (print_animation_catalog, equip_on_new_character))
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:315:            .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:319:            .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:324:            .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:329:            .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:333:                    orbit_camera_input,
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:334:                    follow_camera,
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:344:            .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:350:            .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs:358:            .add_systems(
src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs:14:    keys: Res<ButtonInput<KeyCode>>,
src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs:63:    modifiers.crouch = keys.pressed(KeyCode::KeyC);
src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs:402:    keys: Res<ButtonInput<KeyCode>>,
src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs:426:    if !keys.just_pressed(KeyCode::Space) {
Read weapon_attach.rs
Read user_identity.rs
I've launched three parallel Explore agents covering: (1) mouse capture + UI input blocking, (2) death animation + melee + traffic AI + 3D audio, (3) network identity keys + SQL storage. I'll wait for their findings before designing the plan.

Bash Car driving input and egui gating
IN
cd /home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy/ && grep -rn "keyboard.pressed\|keys.pressed\|keyboard.just_pressed\|keys.just_pressed\|ButtonInput<KeyCode>\|ButtonInput<MouseButton>\|egui_wants\|is_pointer_over" src/plugins/cars_driving/ 2>/dev/null

OUT
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:9:    keyboard: Res<ButtonInput<KeyCode>>,
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:33:    if keyboard.just_pressed(KeyCode::Escape) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:51:        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:53:        } else if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:59:    if keyboard.just_pressed(KeyCode::Space) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:75:        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:78:        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:83:        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:86:        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:91:    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
src/plugins/cars_driving/driving_plugin/keybinds_control.rs:94:    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
src/plugins/cars_driving/click_spawn_select_controls.rs:10:    mouse_button: Res<ButtonInput<MouseButton>>,
src/plugins/cars_driving/click_spawn_select_controls.rs:19:    if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
src/plugins/cars_driving/driving_plugin/camera_follow.rs:10:    mouse_button: Res<ButtonInput<MouseButton>>,
src/plugins/cars_driving/driving_plugin/camera_follow.rs:28:        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
Read multiplayer_plugin.rs
Bash
IN
cd /home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy && grep -rn "SpatialListener\|SpatialScale\|spatial\|panning\|left\|right ear\|SPEED_OF" src/plugins/audio/ | head -50

OUT
src/plugins/audio/mod.rs:11:use bevy::audio::{PlaybackMode, PlaybackSettings, SpatialListener, SpatialScale, Volume};
src/plugins/audio/mod.rs:92:            .add_systems(Startup, (start_sound_manifest_load, setup_spatial_listener))
src/plugins/audio/mod.rs:97:                    add_spatial_listener_to_new_cameras,
src/plugins/audio/mod.rs:106:/// Attach a [`SpatialListener`] to the scene camera so 3D sounds have a set of ears.
src/plugins/audio/mod.rs:107:fn setup_spatial_listener(mut commands: Commands, cameras: Query<Entity, With<Camera3d>>) {
src/plugins/audio/mod.rs:109:        commands.entity(cam).insert(SpatialListener::new(0.25));
src/plugins/audio/mod.rs:113:/// Automatically attach a [`SpatialListener`] to any newly spawned `Camera3d`.
src/plugins/audio/mod.rs:114:fn add_spatial_listener_to_new_cameras(
src/plugins/audio/mod.rs:116:    cameras: Query<Entity, (Added<Camera3d>, Without<SpatialListener>)>,
src/plugins/audio/mod.rs:119:        commands.entity(cam).insert(SpatialListener::new(0.25));
src/plugins/audio/mod.rs:191:/// Spawn a spatial audio emitter (one-shot or looping/parented).
src/plugins/audio/mod.rs:205:        spatial: true,
src/plugins/audio/mod.rs:206:        spatial_scale: Some(SpatialScale(Vec3::splat(scale_factor))),
src/plugins/audio/mod.rs:273:fn update_listener_ears(state: Res<AudioDemoState>, mut listeners: Query<&mut SpatialListener>) {
src/plugins/audio/mod.rs:278:        listener.left_ear_offset = Vec3::X * state.ear_gap / -2.0;
src/plugins/audio/mod.rs:319:    spatial_query: avian3d::prelude::SpatialQuery,
src/plugins/audio/mod.rs:354:    let Some(hit) = spatial_query.cast_ray(
Read lib.rs
Read multiplayer_plugin.rs (lines 271-330)
Read keybinds_control.rs (lines 2-41)
Read client_api_manager.rs
Bash
IN
cd /home/p/VIDOEGAME/crack/crack_demo/demo_resolution_selector_web_bevy && grep -n "Melee\|PlayerEventMsg\|Jump\|Roll" src/plugins/network/multiplayer_plugin.rs | head -60

OUT
19:    Rolling,
53:    pub events: Vec<PlayerEventMsg>,
86:pub enum PlayerEventMsg {
94:    Jump,
96:    Roll,
97:    Melee,
134:pub struct OutboundEvents(pub Vec<PlayerEventMsg>);
174:    pub pending_events: Vec<PlayerEventMsg>,
282:    q_rolling: Query<&Rolling, Added<Rolling>>,
284:        &crate::plugins::weapons::weapon_shooting::PendingMeleeHit,
285:        Added<crate::plugins::weapons::weapon_shooting::PendingMeleeHit>,
294:    // Jump
298:                outbound.0.push(PlayerEventMsg::Jump);
308:        outbound.0.push(PlayerEventMsg::ClimbStart);
311:    // Roll
313:        outbound.0.push(PlayerEventMsg::Roll);
316:    // Melee
318:        outbound.0.push(PlayerEventMsg::Melee);
346:    outbound.0.push(PlayerEventMsg::Shoot {
1203:                PlayerEventMsg::Shoot {
1306:                PlayerEventMsg::Reload => {
1313:                PlayerEventMsg::Jump => {
1324:                                            select_node(&anims, &["Jump_Start", "Jump_Loop"])
1349:                PlayerEventMsg::ClimbStart => {
1365:                                            select_node(&anims, &["Roll", "Jump_Loop"])
1390:                PlayerEventMsg::Roll => {
1401:                                            select_node(&anims, &["Roll"])
1426:                PlayerEventMsg::Melee => {
1428:                        fx: crate::plugins::audio::audio_fx::AudioFxEventType::MeleeWhoosh {
I have a thorough picture. Here are my findings.

1. MOUSE CAPTURE & CAMERA CONTROL
Mouse is NEVER captured/grabbed
There is no CursorGrabMode, cursor_options, CursorOptions, or any cursor grab/visibility manipulation anywhere in the codebase (confirmed by grepping the entire repo excluding target/). The cursor is always free and visible. All camera look control is left-mouse-button click-drag based, not pointer-lock/captured-mouse based. On web, the only cursor-related call is canvas_element.focus() in src/ui_egui.rs:423.

GameControlState enum
Defined in src/plugins/states/mod.rs:31-37:


pub enum GameControlState {
    #[default]
    MapFreecam,
    DrivingCar,
    ControllingPedestrian,
    // todo: spectating, cutscene, etc.
}
(There is no separate "debug scene" state here — debug scenes are separate binaries in src/bin/, e.g. traffic_test.rs, pedestrian_v2.rs, turf_war.rs, car_sim.rs.) State registered at src/plugins/states/mod.rs:53.

Camera control per state (all click-drag, none capture the mouse)
MapFreecam — camera_movement_system in src/plugins/game_freecam/camera_controls.rs:74-201 (registered line 22-26, run_if(in_state(GameControlState::MapFreecam))).

Rotation only while mouse_button.pressed(MouseButton::Left) (line 107), reading MouseMotion events (lines 111-114) into yaw/pitch (EulerRot::YXZ, pitch clamped ±89.9°).
WASD/Space/Ctrl movement, Shift = 5x speed, mouse-wheel = vertical.
ControllingPedestrian — orbit follow camera in src/plugins/pedestrians/pedestrian_controller_plugin/camera.rs.

orbit_camera_input (lines 36-50): returns early unless mouse_buttons.pressed(MouseButton::Left) (line 41); uses Res<AccumulatedMouseMotion> delta to update CameraRig { yaw, pitch } (pitch clamped to CAM_PITCH_MIN/MAX).
follow_camera (lines 52-102): low-pass-follows the character position, applies orbit yaw/pitch instantly.
CameraRig resource defined lines 13-33.
DrivingCar — camera_follows_car in src/plugins/cars_driving/driving_plugin/camera_follow.rs:6-83.

Drag active only when !egui_focused && mouse_button.pressed(MouseButton::Left) (line 46); reads MouseMotion into orbit yaw/pitch stored in a Local<Option<(f32,f32)>> (lines 49-53). Auto-recenters behind the car above 1 m/s (lines 59-70).
All three drain unread MouseMotion events when drag is inactive to prevent buildup.

2. UI BLOCKING INPUT
The egui "is-mouse-over-UI" helper
There is no shared UiState helper method for this. Each system inlines the same idiom directly on the egui context:


let egui_focused = if let Ok(ctx) = contexts.ctx_mut() {
    ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
} else { false };
UiState (src/ui_egui.rs:21-59) holds only graphics/debug-window toggles — it does not track pointer-over-UI. The methods used are egui_wants_pointer_input() and is_pointer_over_egui() (bevy_egui's wrappers). Note: nothing uses egui_wants_keyboard_input() anywhere — keyboard gameplay input is never gated against egui text focus.

Systems that DO gate on egui (pointer only)
camera_movement_system — src/plugins/game_freecam/camera_controls.rs:100-104, used to gate rotation (line 107), WASD movement (line 137), and scroll (line 173).
camera_follows_car — src/plugins/cars_driving/driving_plugin/camera_follow.rs:27-31,46.
click_spawn_select_controls (right-click spawn car / select) — src/plugins/cars_driving/click_spawn_select_controls.rs:19 returns early if egui wants pointer.
drive_character_animation combat firing — src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:134-140; lmb/rmb are AND-ed with !over_ui.
Pedestrian interaction/HUD systems — src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs:38,788.
Other UI systems querying the same idiom: audio/mod.rs:329, pedestrian_v2.rs:131, turf_war.rs.
Gameplay input that is NOT gated against egui (potential bugs)
Pedestrian movement character_input — src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs:13-65 reads WASD/C/Shift with no egui check. Registered in PreUpdate at mod.rs:324-328.
Pedestrian camera orbit orbit_camera_input — camera.rs:36-50 checks only MouseButton::Left, no egui check (unlike the freecam and car cameras). Registered mod.rs:333.
Car driving keybinds_control_car — src/plugins/cars_driving/driving_plugin/keybinds_control.rs:8-94 reads Escape (line 33), W/S throttle, A/D steer, Space handbrake with no egui check.
Enter-car detect_car_interaction (F key) — interaction_ui.rs:177-195, no egui/keyboard-focus check (just keys.just_pressed(KeyCode::KeyF)).
Escape-to-freecam / spawn in spawn.rs:152-157 (Escape) and controller.rs:402,426 (Space jump/climb) — no egui check.
Weapon reload animation.rs:303 (keys.just_pressed(KeyCode::KeyR)) — no egui keyboard-focus check.
Summary of the gating gap
The || is_pointer_over_egui() pattern is applied consistently to mouse/pointer gameplay in the freecam camera, car camera, car-spawn click, and pedestrian combat firing. However:

No system checks egui_wants_keyboard_input() — so typing in an egui text field (e.g. the global chat box in src/plugins/network/global_chat_ui.rs) will still drive WASD movement, car controls, F/R/Escape actions.
Pedestrian WASD movement, pedestrian camera orbit-drag, and all car driving keybinds are not gated at all against egui pointer or keyboard focus.
Bash Find storage usages and any settings/kv table
IN
cd /home/p/VIDOEGAME/crack; echo "=== declare_model_group usages ==="; grep -rn "declare_model_group" --include=*.rs packages crack_demo | grep -v "macro_rules\|models.rs"; echo "=== StorageCrackhouseApiGroup / ExecuteSQL usages ==="; grep -rn "StorageCrackhouseApiGroup\|ExecuteSQL2\|ExecuteSQLParams\|execute_sql" --include=*.rs packages crack_demo; echo "=== settings / key-value / kv table ==="; grep -rni "settings\|key_value\|kv_store\|localstorage" --include=*.rs packages crack_demo | head -30

OUT
Exit code 1
=== declare_model_group usages ===
(eval):1: no matches found: --include=*.rs
=== StorageCrackhouseApiGroup / ExecuteSQL usages ===
(eval):1: no matches found: --include=*.rs
Read multiplayer_plugin.rs (lines 1427-1496)
Read multiplayer_plugin.rs (lines 1204-1308)
Bash Search storage wiring and settings tables
IN
cd /home/p/VIDOEGAME/crack; echo "=== declare_model_group usages ==="; grep -rn --include='*.rs' "declare_model_group" packages crack_demo 2>/dev/null | grep -v "macro_rules\|models.rs"; echo "=== StorageCrackhouse / ExecuteSQL / execute_sql usages ==="; grep -rn --include='*.rs' "StorageCrackhouseApiGroup\|ExecuteSQL2\|ExecuteSQLParams\|execute_sql\|storage_crackhouse" packages crack_demo 2>/dev/null; echo "=== make_api_mapping / api group registration ==="; grep -rn --include='*.rs' "make_api_mapping\|ApiGroupImpls\|run_migrate_tables\|RunGameMigrations" packages crack_demo 2>/dev/null; echo "=== settings/kv/localstorage ==="; grep -rni --include='*.rs' "settings\|key_value\|kv_store\|localstorage\|local_storage" packages crack_demo 2>/dev/null | head -20

OUT
=== declare_model_group usages ===
=== StorageCrackhouse / ExecuteSQL / execute_sql usages ===
packages/storage_crackhouse/src/api.rs:8:    StorageCrackhouseApiGroup,
packages/storage_crackhouse/src/api.rs:10:        (ExecuteSQLParams, SQLAndParams, SqlResultSet),
packages/storage_crackhouse/src/api.rs:11:        (ExecuteSQL2, String, SqlResultSet),
packages/storage_crackhouse/src/api.rs:16:    StorageCrackhouseApiGroup,
packages/storage_crackhouse/src/api.rs:18:        (ExecuteSQLParams, execute_sql_params),
packages/storage_crackhouse/src/api.rs:19:        (ExecuteSQL2, execute_sql2),
packages/storage_crackhouse/src/api.rs:23:pub async fn execute_sql2(sql: String) -> anyhow::Result<SqlResultSet> {
packages/storage_crackhouse/src/api.rs:31:pub async fn execute_sql_params(req: SQLAndParams) -> anyhow::Result<SqlResultSet> {
packages/web_serviceworker_crackslave/src/lib.rs:19:        storage_crackhouse::install_opfs_sahpool()
crack_demo/web_worker/src/lib.rs:6:use crack::storage_crackhouse::api::StorageCrackhouseApiGroup;
crack_demo/web_worker/src/lib.rs:33:            Arc::new(StorageCrackhouseApiGroup),
crack_demo/thread_worker/src/main.rs:6:use crack::storage_crackhouse::api::ExecuteSQL2;
crack_demo/thread_worker/src/main.rs:26:        let ret2 = c.call::<ExecuteSQL2>(sql.clone()).await;
crack_demo/thread_worker/src/lib.rs:6:use crack::storage_crackhouse::api::StorageCrackhouseApiGroup;
crack_demo/thread_worker/src/lib.rs:13:        Arc::new(StorageCrackhouseApiGroup),
crack_demo/web_frontend/src/components/display_table.rs:1:use crack::storage_crackhouse::types::{DbValue, SqlResultSet};
crack_demo/web_frontend/src/components/db_table_list.rs:2:use crack::storage_crackhouse::{api::ExecuteSQL2, types::DbValue};
crack_demo/web_frontend/src/components/db_table_list.rs:14:        api.call::<ExecuteSQL2>(sql.to_string()).await
crack_demo/web_frontend/src/components/db_table_content.rs:1:use crack::storage_crackhouse::{
crack_demo/web_frontend/src/components/db_table_content.rs:2:    api::{ExecuteSQL2, ExecuteSQLParams},
crack_demo/web_frontend/src/components/db_table_content.rs:24:                api.call::<ExecuteSQLParams>(SQLAndParams {
crack_demo/web_frontend/src/components/db_table_content.rs:29:                api.call::<ExecuteSQL2>(select_star.clone()).await,
crack_demo/web_frontend/src/components/db_sql_repl.rs:1:use crack::storage_crackhouse::api::ExecuteSQL2;
crack_demo/web_frontend/src/components/db_sql_repl.rs:21:            let r = api.call::<ExecuteSQL2>(sql).await;
crack_demo/game_logic/src/worker/models.rs:2:use storage_crackhouse::declare_model_group;
crack_demo/game_logic/src/worker/models.rs:13:    storage_crackhouse::models::run_migrate_tables(
crack_demo/game_logic/src/worker/models.rs:14:        vec![Arc::new(GameLogicModels) as Arc<dyn storage_crackhouse::models::ModelGroup>]
=== make_api_mapping / api group registration ===
packages/storage_crackhouse/src/models.rs:175:pub async fn run_migrate_tables(
packages/storage_crackhouse/src/models.rs:340:        models::{ModelGroup, ModelSerial, run_migrate_tables},
packages/storage_crackhouse/src/models.rs:365:        let _r = run_migrate_tables(vec![Arc::new(ModelGroup1) as Arc<dyn ModelGroup>].into_iter())
packages/api_asscrack/src/api/api_method_macros.rs:11:pub trait ApiGroupImpls: ApiGroupMethods {
packages/api_asscrack/src/api/api_method_macros.rs:240:        impl $crate::api::api_method_macros::ApiGroupImpls for $name {
packages/api_asscrack/src/crack_worker/api_worker.rs:7:    api::api_method_macros::{ApiGroupImpls, ApiMethodImpl, ApiMethodInfo},
packages/api_asscrack/src/crack_worker/api_worker.rs:16:pub fn make_api_mapping(groups: Vec<Arc<dyn ApiGroupImpls>>) -> Arc<ApiImplMapping> {
packages/api_asscrack/src/crack_worker/api_worker.rs:82://         .get_or_init(move || async move { Arc::new(make_api_mapping()) })
packages/api_asscrack/src/crack_worker/api_worker.rs:103:fn _get_infos2(grps: Vec<Arc<dyn ApiGroupImpls>>) -> Vec<ApiMethodInfo> {
packages/api_asscrack/src/crack_worker/api_worker.rs:112:fn _get_impls2(grps: Vec<Arc<dyn ApiGroupImpls>>) -> Vec<ApiMethodImpl> {
packages/thread_crackworker/src/lib.rs:64:        api::api_worker_declarations::WorkerApiGroup2, crack_worker::api_worker::make_api_mapping,
packages/thread_crackworker/src/lib.rs:74:            impl_mapping: make_api_mapping(vec![]),
packages/thread_crackworker/src/lib.rs:97:            impl_mapping: make_api_mapping(vec![Arc::new(WorkerApiGroup2), Arc::new(TestApiGroup)]),
crack_demo/web_worker/src/lib.rs:4:    api::api_worker_declarations::*, crack_worker::api_worker::make_api_mapping,
crack_demo/web_worker/src/lib.rs:32:        let _r = web_worker_registration(make_api_mapping(vec![
crack_demo/thread_worker/src/lib.rs:3:use crack::api_asscrack::crack_worker::api_worker::{ApiImplMapping, make_api_mapping};
crack_demo/thread_worker/src/lib.rs:11:    make_api_mapping(vec![
crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/mod.rs:129:        .call::<game_logic::api::RunGameMigrations>(())
crack_demo/game_logic/src/api.rs:13:    (RunGameMigrations, (), ()),
crack_demo/game_logic/src/worker/mod.rs:17:    (RunGameMigrations, models::run_game_migrations),
crack_demo/game_logic/src/worker/models.rs:13:    storage_crackhouse::models::run_migrate_tables(
=== settings/kv/localstorage ===
crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs:8:        settings::{Backends, WgpuSettings},
crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs:30:/// Create a basic app where we override only the DefaultPlugin, render settings, window reactivity settings.
crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs:68:                render_creation: bevy::render::settings::RenderCreation::Automatic(Box::new(
crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs:69:                    WgpuSettings {
crack_demo/demo_resolution_selector_web_bevy/src/basic_app.rs:89:    .insert_resource(bevy::winit::WinitSettings {
crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs:26:    pub show_settings: bool,
crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs:45:            show_settings: false,
crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs:66:            show_settings: false,
crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs:154:    if ui_state.show_settings {
crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs:159:                    ui.heading("Graphics Settings");
crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs:162:                            ui_state.show_settings = false;
crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs:239:                    ui_state.show_settings = !ui_state.show_settings;
crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_material_edit.rs:31:    // Material settings
crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_material_edit.rs:37:    // Lighting settings
crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_material_edit.rs:82:            ui.heading("Material Settings");
crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_material_edit.rs:92:            ui.heading("Lighting Settings");
crack_demo/demo_resolution_selector_web_bevy/src/plugins/map_plugin/map_material_edit.rs:112:                info!("Updating all loaded materials and lighting settings...");
crack_demo/demo_resolution_selector_web_bevy/src/plugins/geojson.rs:944:// OSM Overlays Debug Settings
crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/manifest.rs:235:    type Settings = ();
crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/manifest.rs:241:        _settings: &Self::Settings,
I now have a complete picture of all four systems. Here are my findings.

1. DEATH ANIMATION
Damage → Death pipeline
DamageEvent is the central death trigger. Defined at src/plugins/pedestrian_ai/combat.rs:46-52, handled by apply_damage_observer (combat.rs:57-96):

Subtracts amount from Health::current.
On current <= 0: triggers a DeathThud audio fx, writes a PedestrianDied message, and inserts Dying { timer: DEATH_ANIM_TIME } on the target (combat.rs:92-94).
The Dying component is defined in src/plugins/pedestrian_ai/faction.rs:121-125 (just a countdown timer: f32). DEATH_ANIM_TIME = 2.5 seconds (faction.rs:9).

AI pedestrian death
start_ai_death_animation (combat.rs:101-111) runs on Added<Dying> for entities that have an AiModel. It triggers a PedestrianAnimationControlEvent { animation: "Death01", speed: 1.0 }. Because the shared animation system loops everything (see below), the AI corpse plays "Death01" looped until despawn.

tick_dying (combat.rs:115-131) zeroes LinearVelocity each frame (freeze in place), decrements the timer, and despawn()s at zero.

PLAYER death — the key difference
The player controller has no AiModel, so start_ai_death_animation does not touch it. Two things happen when the player's controller gets Dying:

player_death_to_freecam (src/plugins/pedestrians/pedestrian_controller_plugin/spawn.rs:128-148): runs on Added<Dying> for controlled.controller. It immediately clears ControlledCharacter and calls commands.entity(controller).despawn(), then switches state to MapFreecam.
This is the answer to your player question: the player's mesh is despawned immediately. The DEATH_ANIM_TIME timer and the death clip never get a chance to play — player_death_to_freecam despawns the controller (and its child model) on the very same frame the Dying component is added. The comment at spawn.rs:126-127 explicitly says "The corpse is despawned here (matching Escape's behavior)."

The player's animation driver does have death-clip handling that would otherwise fire: in src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs:201-222, when dying is true it plays "Death01" once with no .repeat() ("play once and hold on the final (downed) frame", animation.rs:216). But since the controller is despawned immediately by player_death_to_freecam, this branch is effectively dead code for the player. (The player model is flagged ManualAnimation at spawn_pedestrian.rs:207, so it's driven by this manual driver, not the shared looping system.)
How pedestrian GLB models are spawned
SpawnPedestrianEvent (src/plugins/pedestrians/spawn_pedestrian.rs:15-21): { url, position, controller, parent }.

spawn_pedestrian_observer (spawn_pedestrian.rs:67-107): kicks off an async RPC fetch of the GLB bytes (FetchPedestrianModel), spawns a placeholder child of parent with a PendingPedestrianGlbFetch.
poll_pedestrian_glb_fetches (spawn_pedestrian.rs:109-183): when the fetch completes, writes bytes into MemoryDir, loads via memory:// URL, inserts ModelRoot, PedestrianGltf { handle }, NeedAlignment, ModelController, and spawns a WorldAssetRoot child (the actual scene).
link_pedestrian_model (spawn_pedestrian.rs:185-216): on Added<ModelRoot>, if the controller is an AI ped it inserts AiModel(model_ent); if it's the local player it inserts ManualAnimation on the model.
init_pedestrians_system (spawn_pedestrian.rs:234-358): once meshes load, computes bounds and classifies the skeleton (PedestrianSkeleton).
Animation system (one-shot vs looping)
src/plugins/pedestrians/animation.rs:

PedestrianAnimationControlEvent { ped, animation, speed } (animation.rs:43-48) — drives a ped's animation. Handled by pedestrian_animation_control_observer (animation.rs:78-99), which just writes/updates a TargetAnimation { name, speed } component.
TargetAnimation (animation.rs:65-69): desired clip. CurrentPlayingAnimation (animation.rs:72-76): what's actually playing, to avoid restarts.
ManualAnimation (animation.rs:53-54): opt-out marker; play_animations_system skips these (query filter Without<ManualAnimation> at animation.rs:122). The player uses this.
ActiveOneShot { node, name } (animation.rs:57-61): tracks a transient one-shot. In play_animations_system (animation.rs:136-146), if an ActiveOneShot is present and its animation !is_finished(), the system skips the locomotion update (holds the one-shot); once finished it removes the marker.
Crucially, the shared system always calls .repeat() (animation.rs:204: player.play(node_index).repeat().set_speed(...)). So every clip driven through TargetAnimation loops — including "Death01" on AI corpses. There is no "freeze on last frame" path in the shared system; the only non-looping/hold-last-frame playback is in the player's manual driver (pedestrian_controller_plugin/animation.rs:217, which plays Death01 without .repeat()).
How to spawn a "death prop" (frozen-last-frame corpse)
Combining the above, a death prop = a pedestrian GLB with a non-looping death clip held on its final frame. Two viable approaches:

Reuse the player manual-driver pattern: spawn a pedestrian via SpawnPedestrianEvent, mark the ModelRoot with ManualAnimation (so the shared looping system ignores it), then drive its AnimationPlayer yourself with player.play(death_node).set_speed(...) without .repeat() — exactly as pedestrian_controller_plugin/animation.rs:210-220 does. Bevy's AnimationPlayer holds the last frame when a non-repeating clip finishes.
Or add a new "hold last frame" flag to the shared system: the shared play_animations_system currently hard-codes .repeat() at animation.rs:204; a death-prop variant would need to branch off that (e.g. an ActiveOneShot/FrozenAnimation marker) so it plays once and stops. The ActiveOneShot infrastructure (animation.rs:57-61, 136-146) already models "play once, don't loop back to locomotion," but it doesn't set the player clip to non-repeating — it just guards locomotion, so the underlying player.play still came from a .repeat() call. You'd want a genuinely non-repeating player.play(node).
The clip name to use is "Death01" (referenced at combat.rs:106 and pedestrian_controller_plugin/animation.rs:210).

2. MELEE ATTACKS
Player melee registration
Player melee is a two-stage, delayed forward raycast, not an instant hit:

Stage 1 — input, in src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs, on LMB (lmb computed at animation.rs:139):

Melee weapon branch (animation.rs:322-334): plays "Sword_Attack", inserts PendingMeleeHit { timer: 0.25, is_melee: true } on the controller, triggers a MeleeWhoosh audio fx (volume 1.0).
Unarmed/punch branch (animation.rs:335-352): plays Punch_Jab/Punch_Cross, inserts PendingMeleeHit { timer: 0.25, is_melee: false }, MeleeWhoosh (volume 0.4).
Stage 2 — PendingMeleeHit is defined at src/plugins/weapons/weapon_shooting.rs:318-322 and processed by tick_pending_melee_hits (weapon_shooting.rs:324-461). After the 0.25s timer elapses (to sync with the swing animation), it does:

Raycast from origin = translation + Y*0.5, direction = the entity's forward (gt.rotation() * Vec3::Z), range 1.8m (weapon_shooting.rs:342-345).
Uses is_person_entity (weapon_shooting.rs:58-81) to classify the hit by walking parents looking for CharacterController/ModelRoot/PedestrianSkeleton/DriverMesh.
If it hits a person: plays MeleeHitMeat (melee) or PunchHit (unarmed) audio, resolves up to the CharacterController with Health, and triggers DamageEvent with SWORD_DAMAGE (35.0) or PUNCH_DAMAGE (12.0) (weapon_shooting.rs:386-423).
If it hits a car: damages CarHealth, plays MeleeClash (weapon_shooting.rs:424-456).
Removes PendingMeleeHit (weapon_shooting.rs:458).
So the player's melee is: timed forward raycast, 1.8m, resolved on a delay — a genuine spatial hit test (unlike AI melee, see below).

Does it work over multiplayer/online?
Partially — melee does NOT deal damage remotely.

Outbound: collect_outbound_events (src/plugins/network/multiplayer_plugin.rs:283-319) detects Added<PendingMeleeHit> on the local controller and pushes PlayerEventMsg::Melee (multiplayer_plugin.rs:316-318). It carries no direction, position, or damage — contrast PlayerEventMsg::Shoot { origin, dir, damage }.
Inbound: the remote handler for PlayerEventMsg::Melee (multiplayer_plugin.rs:1426-1434) only plays a MeleeWhoosh sound. There is no raycast and no DamageEvent.
Compare gun Shoot handling (multiplayer_plugin.rs:1203-1305): it re-raycasts on the receiver and applies victim-authoritative DamageEvent when hit_root == local_controller (multiplayer_plugin.rs:1280-1294).
Conclusion: a remote player's melee swing is cosmetic only (whoosh sound). It cannot damage other players/peds across the network, because the Melee message carries no hit data and the receiver never raycasts or applies damage. The local player's own melee raycast in tick_pending_melee_hits is single-client authoritative and won't hit remote avatars unless they carry the person-marker components and colliders locally.

How AI does melee — ai_combat
src/plugins/pedestrian_ai/combat.rs:139-458. AI melee is distance-based, no raycast:

Melee (sword) branch (combat.rs:386-417): gated by perception.target_dist > MELEE_RANGE where MELEE_RANGE = 2.0 (combat.rs:30). If in range, directly triggers DamageEvent { amount: SWORD_DAMAGE=35 } against target_entity (or CarHealth), plays MeleeWhoosh and Sword_Attack. Cooldown SWING_INTERVAL = 0.8 (combat.rs:32).
Punch branch (combat.rs:418-456): gated by PUNCH_RANGE = 1.5 (combat.rs:33), PUNCH_DAMAGE = 12, PUNCH_INTERVAL = 0.6. Same pattern — direct DamageEvent, no spatial check.
AI gun fire (combat.rs:225-385) does raycast (spatial_query.cast_ray, combat.rs:275).
So there are two different melee-hit mechanisms: player = delayed forward raycast (tick_pending_melee_hits, 1.8m); AI = pure distance check against its perceived target (ai_combat, 2.0m sword / 1.5m punch).

WeaponKind::is_melee / WeaponId
WeaponKind enum (Gun/Melee) is at src/plugins/weapons/weapon_attach.rs:50-54 (used for model transform/orientation, not hit detection).
The logic predicate is WeaponId::is_melee at src/plugins/weapons/weapon_manifest.rs:31-33 (matches!(self, WeaponId::Melee(_))); also is_gun (:28-30) and is_unarmed (:25-27). WeaponId enum at weapon_manifest.rs:18-22 (Unarmed/Melee(String)/Gun(GunInfo)).
DamageEvent
Defined combat.rs:46-52 (target, amount, source), handled by apply_damage_observer (combat.rs:57-96). Also adds source to the victim's Enemies grudge list (combat.rs:68-72, Enemies at faction.rs:107-116). It's triggered from: player gun (weapon_shooting.rs:210), player melee (weapon_shooting.rs:418), AI combat (combat.rs:344, 398, 430), and multiplayer remote shots (multiplayer_plugin.rs:1283).

3. TRAFFIC PEDESTRIAN AI MELEE — climbing on top of target
src/plugins/pedestrian_ai/movement_ai.rs, ai_movement (:31-277). The steering direction is chosen per AiState. For melee/unarmed in AiState::Hunt (movement_ai.rs:150-169):


} else {
    // Melee/unarmed: sprint straight at target.
    modifiers.sprint = true;
    let to_target = (perception.target_pos - my_pos).with_y(0.0);
    let dir = to_target.normalize_or_zero();
    // ... knee-height obstacle check -> input.jump = true ...
    dir
}
This is the root cause: there is no stopping distance for melee AI. Unlike the gun branch (movement_ai.rs:84-149) which maintains a standoff band GUN_MIN = 12.0..GUN_MAX = 30.0 (movement_ai.rs:18-19) — backing away when dist < GUN_MIN — the melee branch unconditionally sets sprint = true and drives straight at perception.target_pos with no minimum-distance check. The AI keeps pushing full-speed into the target's capsule until physics resolution shoves it upward/over the target, which reads as "climbing on top."

Contributing factor: the knee-height obstacle probe (movement_ai.rs:156-166) casts a 1.0m ray at my_pos + Y*0.3; when the target body blocks it, the AI sets input.jump = true, actively hopping onto/over the target.

The distance gating that would stop melee only exists on the attack side (combat.rs:388 MELEE_RANGE = 2.0, combat.rs:420 PUNCH_RANGE = 1.5), not on the movement side. To fix, the melee branch in ai_movement needs a comfortable-stop check, e.g. zero out dir (or back off) when to_target.length() is below a melee stand distance (~1.2–1.8m), mirroring the gun branch's GUN_MIN logic.

4. 3D AUDIO
All in src/plugins/audio/mod.rs, imports at mod.rs:11 (PlaybackMode, PlaybackSettings, SpatialListener, SpatialScale, Volume).

Listener (the "ears")
setup_spatial_listener (mod.rs:107-111): at Startup, inserts SpatialListener::new(0.25) (0.25m ear gap) on every Camera3d.
add_spatial_listener_to_new_cameras (mod.rs:114-121): Update system, attaches SpatialListener::new(0.25) to any Added<Camera3d> without one.
update_listener_ears (mod.rs:273-281): drives listener.left_ear_offset = Vec3::X * ear_gap / -2.0 and right_ear_offset = Vec3::X * ear_gap / 2.0 from the AudioDemoState.ear_gap slider (0.15–0.35m, mod.rs:428). This is the left/right-ear panning setup.
Emitter
PlaySoundEvent (mod.rs:61-76): carries handle, position, volume, speed, attenuation, follow: Option<Entity>, looped.
play_sound_observer (mod.rs:192-221) builds PlaybackSettings at mod.rs:201-208:

PlaybackSettings {
    mode,                       // Loop or Despawn
    volume: Volume::Linear(ev.volume),
    speed: ev.speed,
    spatial: true,
    spatial_scale: Some(SpatialScale(Vec3::splat(scale_factor))),
    ..default()
}
where scale_factor = 1.0 / attenuation.max(0.001) (mod.rs:200). It spawns an entity with AudioPlayer::new(handle) + those settings. If follow is Some, it parents the emitter to that entity (ChildOf) so the sound tracks a moving object (looping sounds); otherwise it sets a static Transform at position (mod.rs:216-220).
Registration & higher-level fx
Plugin wiring: GameAudioPlugin::build (mod.rs:81-104) registers the play_sound_observer and audio_fx::audio_fx_observer, plus the listener setup systems.
Per-clip attenuation/volume come from the manifest (SoundEntry.attenuation, .volume, parsed in load_sound_manifest_system, mod.rs:165-172).
The gameplay-facing layer is src/plugins/audio/audio_fx.rs (AudioFxEvent / AudioFxEventType used throughout combat, e.g. GunShot, DeathThud, MeleeWhoosh, BulletImpact) — the audio_fx_observer ultimately emits PlaySoundEvents with a world position, which become spatial emitters via the path above.
Note: AudioDemoState (mod.rs:224-251) and AudioDemoPlugin (mod.rs:256-270) are a demo/testing harness (click-ground-to-play, ear-gap slider); the actual game audio path is GameAudioPlugin + audio_fx.

Read api.rs
Agent 1 done. Waiting on agents 2 and 3.

Bash Check bevy deps and KV entry usage
IN
cd /home/p/VIDOEGAME/crack; echo "=== bevy app Cargo deps ==="; grep -nE "storage_crackhouse|game_logic|net_crackpipe|api_asscrack|^thread_worker|web_serviceworker" crack_demo/demo_resolution_selector_web_bevy/Cargo.toml; echo "=== web_worker registration (WASM) ==="; sed -n '25,45p' crack_demo/web_worker/src/lib.rs 2>/dev/null; grep -rn --include='*.rs' "GameKvEntry" crack_demo packages 2>/dev/null

OUT
=== bevy app Cargo deps ===
36:net_crackpipe = { path = "../../packages/net_crackpipe" }
40:game_logic = { path = "../../crack_demo/game_logic" }
41:api_asscrack = { path = "../../packages/api_asscrack" }
46:thread_worker = { path = "../../crack_demo/thread_worker" }
50:web_serviceworker_crackloader = { path = "../../packages/web_serviceworker_crackloader" }
=== web_worker registration (WASM) ===
        }
    });

    spawn_local(async move {
        tracing::info!("Web Worker : spawned...");

        tracing::info!("Web Worker : web_worker_registration()...");
        let _r = web_worker_registration(make_api_mapping(vec![
            Arc::new(StorageCrackhouseApiGroup),
            Arc::new(WorkerApiGroup2),
            Arc::new(game_logic::api::GameLogicApiGroup),
        ]))
        .await;
        match _r {
            Err(e) => {
                tracing::error!("web_worker_registration ERROR! {:#?}. WORKER IS DEAD", e);
            }
            _ => {
                tracing::info!(
                    "init_worker / web_worker_registration() finished! WORKER IS RUNNING!!!"
                );
crack_demo/game_logic/src/worker/models.rs:6:    pub struct GameKvEntry {
I have a complete picture. Here are my findings.

Findings
PART 1 — User Secret Key / Network Identity
Identity types — packages/net_crackpipe/src/user_identity.rs
UserIdentity (lines 6-33): wraps a PublicKey (user_id). Derives nickname (nickname() line 11) and color from the pubkey. This is the persistent public identity.
UserIdentitySecrets (lines 35-64): holds _user_private_key: SecretKey + user_identity: UserIdentity.
secret_key() (line 52) → &SecretKey
user_identity() (line 49) → &UserIdentity
UserIdentitySecrets::generate() (lines 55-63): SecretKey::generate(rand::thread_rng()) — random every call. This is the UserSecretKey that should persist across sessions/tabs but currently does NOT.
It derives serde::Serialize/Deserialize (line 35), so the whole struct (incl. the SecretKey) is directly serializable to bytes for storage.
NodeIdentity (lines 66-117): { user_identity, node_id: PublicKey, bootstrap_idx: Option<u32> }. Ties a per-node public key to the user identity. Constructed via NodeIdentity::new(...) (line 106).
The NodeSecretKey (random per session/tab) — distinct from UserSecretKey
In GlobalMatchmaker::new_with_config — packages/net_crackpipe/src/global_matchmaker.rs:267-292:
Line 274: let own_private_key = Arc::new(SecretKey::generate(&mut rand::thread_rng())); — this is the NodeSecretKey, freshly random on every launch (looped up to 3 times, lines 273-290). It is intentionally ephemeral and separate from the user identity.
GlobalMatchmaker struct (lines 38-48) stores user_secrets: Arc<UserIdentitySecrets> and own_public_key: Arc<PublicKey> (derived from own_private_key). See fresh() at lines 179-221 where own_public_key: Arc::new(own_private_key.public()) (line 186) and the node is spawned via MainNode::spawn(...) (line 209) receiving both own_private_key and user.clone() (the user secrets).
Bootstrap nodes use fixed well-known keys, not random: SecretKey::from_bytes(&BOOTSTRAP_SECRET_KEYS[...]) (lines 459, 528; also network_manager.rs:228).
So: UserIdentitySecrets = stable identity (should persist); own_private_key inside GlobalMatchmaker = per-session node key (correctly ephemeral).

How the secret flows into the network managers
Chain: UserIdentitySecrets → Arc → NetworkManager::init → GlobalMatchmaker::new_with_config → MainNode::spawn.

NetworkManager::init(secrets: Arc<UserIdentitySecrets>, config) — packages/net_crackpipe/src/network_manager.rs:70-79 calls GlobalMatchmaker::new_with_config(secrets, config) (line 74).
GlobalMatchmaker::new(user_identity_secrets: Arc<UserIdentitySecrets>) at global_matchmaker.rs:264, and new_with_config at 267.
Where the Bevy app creates the network (THE KEY SPOT TO CHANGE)
crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/mod.rs:

NetworkPlugin (lines 53-72) registers start_network as a Startup system (line 68) and, native-only, inserts NetworkRuntime(Arc<tokio::runtime::Runtime>) (lines 59-66, struct at 22-24).
start_network (native lines 74-117, WASM lines 119-158) both call UserIdentitySecrets::generate() at line 80 (native) and line 121 (WASM). This is exactly where a random identity is minted every session/tab.
The secrets are moved into chat_main_task(secrets, ...) (native line 108, WASM line 149).
chat_main_task (lines 160-447) at line 177 calls:

let network = NetworkManager::init(
    Arc::new(secrets),
    game_logic::network::network_manager_config(),
).await
Then network.matchmaker() (line 189), global chat + gameplay room join.
To make the user key persist, you would load-or-generate the UserIdentitySecrets here (before line 80/121) instead of unconditionally calling generate().

Note: multiplayer_plugin.rs contains only the gameplay/replication systems (MultiplayerPlugin, lines 238-272). It does not touch identity/secret keys — the identity is entirely owned by mod.rs/GlobalMatchmaker.

PART 2 — SQL / Storage API
Low-level SQL engine — packages/storage_crackhouse/
src/impl_rusqulite.rs: sql_query(sql: SQLAndParams) -> anyhow::Result<SqlResultSet> (line 23). A single global connection (lazy_static CONN, lines 19-21). Connection target _new_connection() (lines 7-17):
WASM: "file:/assets/scripts/post3.db?vfs=opfs-sahpool" (line 10) — real SQLite persisted via OPFS, not localStorage.
Native: "post3.db" (line 14).
Params bound positionally (lines 43-46).
src/lib.rs: WASM VFS installers install_opfs_sahpool() (line 7) and install_relaxed_idb() (line 20). OPFS is installed by the web service worker (packages/web_serviceworker_crackslave/src/lib.rs:19).
src/types.rs:
SQLAndParams { sql: String, params: Vec<DbValue> } (lines 3-7).
DbValue enum (lines 30-42): Null | Integer(i64) | Real(f64) | Text(String) | Blob(Vec<u8>) — Blob(Vec<u8>) is exactly what you'd use for a small secret-key blob. From<Vec<u8>> (line 67) and TryFrom<DbValue> for Vec<u8> (line 114) conversions exist.
SqlResultSet { column_names, rows: Vec<SqlResultRow> } (lines 167-176).
src/models.rs: an ORM-ish layer. declare_model_group! macro (lines 228-331) generates typed tables. ModelSerial provides sql_for_upsert_row() (line 97, INSERT OR REPLACE), sql_for_insert_row_or_ignore() (line 72), sql_for_delete_row() (line 50). run_migrate_tables(...) (line 175) drops+recreates tables. Table names are "{grp}_{table}". Columns support Vec<u8> → DbValueType::Blob (line 219).
The API layer that the Bevy app actually uses
SQL is exposed as worker RPC methods — packages/storage_crackhouse/src/api.rs:
declare_api_group2! { StorageCrackhouseApiGroup, [ (ExecuteSQLParams, SQLAndParams, SqlResultSet), (ExecuteSQL2, String, SqlResultSet) ] } (lines 7-13).
ExecuteSQL2 = raw SQL string (no params); ExecuteSQLParams = SQL + Vec<DbValue> params (use this for a blob).
CrackClient — crack_demo/demo_resolution_selector_web_bevy/src/plugins/crack_plugin/mod.rs:
pub struct CrackClient(pub ApiClient) (line 11), a Bevy Resource.
Initialized async in init_client() (lines 110-133): builds an ApiClient::new(factory) (line 122) — native uses thread_worker::spawn_in_process_worker() (line 112), WASM uses WebWorkerFactory (lines 115-120). Then it calls WorkerPing and game_logic::api::RunGameMigrations (lines 124-130), proving the client can invoke any registered API method.
ApiClient::call::<T>() — packages/api_asscrack/src/api/api_client.rs:63-104: postcard-serializes the arg, dispatches to the worker over a pipe, awaits the typed reply. This is the generic call path (T: ApiMethodDecl).
Worker registration — SQL group IS registered on both targets
Native: crack_demo/thread_worker/src/lib.rs:10-16 — make_api_mapping(vec![WorkerApiGroup2, StorageCrackhouseApiGroup, GameLogicApiGroup]).
WASM: crack_demo/web_worker/src/lib.rs:32-36 — same three groups registered. So ExecuteSQL2 / ExecuteSQLParams are callable from the Bevy CrackClient on both native and browser. Confirmed working example: crack_demo/thread_worker/src/main.rs:26 calls c.call::<ExecuteSQL2>(sql); the web frontend uses ExecuteSQL2/ExecuteSQLParams in crack_demo/web_frontend/src/components/db_*.rs.
Note the separate second RPC/manager layer in net_crackpipe (ClientApiManager::call_method in packages/net_crackpipe/src/api/client_api_manager.rs:85, with MatchRow2/GameStateRow2 declared in api/api_declarations.rs) is the peer-to-peer game-server API over gossip chat — unrelated to local storage. Local persistence goes through CrackClient/ApiClient → worker → storage_crackhouse.

Existing key/value store
There already is a generic KV table: crack_demo/game_logic/src/worker/models.rs:4-10:

declare_model_group! { GameLogicModels,
    #[db_table(pk(id))]
    pub struct GameKvEntry { pub id: i64, pub val: Option<String> }
}
This creates table GameLogicModels_GameKvEntry(id INTEGER PK, val TEXT). It is migrated by run_game_migrations (lines 12-19) → invoked via RunGameMigrations (game_logic/src/api.rs:13, game_logic/src/worker/mod.rs:17) which the Bevy app already runs at startup (crack_plugin/mod.rs:129).
Caveat: it is keyed by i64 and stores String (not blob). No string-keyed "settings" table exists.
Bevy app dependency reality (relevant for wiring persistence)
crack_demo/demo_resolution_selector_web_bevy/Cargo.toml depends on net_crackpipe, game_logic, api_asscrack, thread_worker — but NOT storage_crackhouse directly. So to persist a blob you have two options:

Add storage_crackhouse as a dep and call CrackClient.0.call::<ExecuteSQLParams>(SQLAndParams { sql, params: vec![DbValue::Blob(key_bytes)] }).
(Cleaner, matches existing pattern) add a new method to GameLogicApiGroup (like RunGameMigrations) e.g. SaveUserKey/LoadUserKey, backed by a new string-keyed KV/settings model in game_logic/src/worker/models.rs, and call it via the already-present game_logic dep.
How to save/load a small secret-key blob
UserIdentitySecrets is Serialize/Deserialize — serialize with postcard to Vec<u8>, store as DbValue::Blob.
Save: ExecuteSQLParams with INSERT OR REPLACE INTO <kv_table>(k, v) VALUES (?, ?), params [DbValue::Text("user_identity"), DbValue::Blob(bytes)] (or use ModelSerial::sql_for_upsert_row).
Load: ExecuteSQLParams SELECT v FROM <kv_table> WHERE k = ?; read rows[0].cols[0] as DbValue::Blob → Vec<u8> → postcard-deserialize; if absent, UserIdentitySecrets::generate() then save.
Persistence across sessions/tabs works on both targets: native post3.db file, browser OPFS-backed SQLite (shared across tabs of the same origin) — not localStorage.
One cross-tab caveat worth flagging
The SQLite connection is a single global Mutex<Connection> opened once (impl_rusqulite.rs:19-21) against an OPFS SAHPool VFS. OPFS SAHPool typically takes an exclusive lock per file, so multiple simultaneously-open browser tabs may contend for the same DB file. That affects whether "persist across tabs" means "shared live DB" vs "same value read at startup." Persisting the user key (read once at launch) is fine; concurrent multi-tab writes may need consideration.

Agent 2 done — comprehensive. Waiting on agent 3 (network keys + SQL).