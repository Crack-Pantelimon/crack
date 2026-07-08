//! Audio engine plugin.
//!
//! Loads the sound-fx manifest (a plain-text list of `.ogg` / `.mp3` clip paths relative to the
//! manifest's own folder), preloads every clip as a [`bevy::audio::AudioSource`], and exposes them
//! through the [`SoundManifest`] resource. Once the manifest is parsed the
//! [`SoundManifestLoadFinished`](crate::plugins::states::SoundManifestLoadFinished) state flips to
//! `Finished`.

pub mod audio_fx;

use bevy::audio::{PlaybackMode, PlaybackSettings, SpatialListener, SpatialScale, Volume};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::plugins::pedestrians::manifest::{TextAsset, TextAssetLoader};
use crate::plugins::states::SoundManifestLoadFinished;

/// Minimum time between two triggered sounds; rapid clicks in a shorter window are ignored.
pub const SOUND_DEBOUNCE_SECS: f32 = 0.15;

/// One entry from the sound-fx manifest.
#[derive(Clone, Debug)]
pub struct SoundEntry {
    /// Path as written in the manifest, e.g. `"car-sounds/car-engine-1.mp3"`. Used as the label.
    pub name: String,
    /// Fully-resolved asset URL used to load the clip.
    pub url: String,
    /// Preloaded audio clip handle.
    pub handle: Handle<AudioSource>,
    /// Hand-picked attenuation distance.
    pub attenuation: f32,
    /// Base volume multiplier parsed from manifest.
    pub volume: f32,
}

/// Global resource holding every clip listed in the manifest.
#[derive(Resource, Default)]
pub struct SoundManifest {
    pub sounds: Vec<SoundEntry>,
    /// True once the manifest text has been fetched and parsed.
    pub loaded: bool,
}

impl SoundManifest {
    /// Returns the sound entry matching name (linear scan).
    pub fn get(&self, name: &str) -> Option<&SoundEntry> {
        self.sounds.iter().find(|s| s.name == name)
    }
}

/// Internal bootstrap: handle to the manifest text + the folder used to resolve relative paths.
#[derive(Resource)]
struct SoundManifestBootstrap {
    folder: String,
    manifest_handle: Handle<TextAsset>,
}

/// Fired whenever a sound needs to play. Carries everything the playback
/// observer needs to spawn a 3D emitter (one-shot or looping/parented).
#[derive(Event, Clone)]
pub struct PlaySoundEvent {
    pub handle: Handle<AudioSource>,
    /// World-space location the sound plays at (ignored if follow is Some).
    pub position: Vec3,
    /// Linear volume multiplier.
    pub volume: f32,
    /// Playback speed / pitch multiplier.
    pub speed: f32,
    /// Attenuation distance.
    pub attenuation: f32,
    /// Entity to attach the emitter to as a child (for looping sounds).
    pub follow: Option<Entity>,
    /// Whether the sound is looping.
    pub looped: bool,
}

/// Core audio plugin for real gameplay audio playback.
pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        // Guard against double-init of TextAsset loader
        if !app.world().contains_resource::<Assets<TextAsset>>() {
            app.init_asset::<TextAsset>()
                .init_asset_loader::<TextAssetLoader>();
        }
        app.init_state::<SoundManifestLoadFinished>()
            .init_resource::<SoundManifest>()
            .add_observer(play_sound_observer)
            .add_observer(audio_fx::audio_fx_observer)
            .add_systems(Startup, (start_sound_manifest_load, setup_spatial_listener))
            .add_systems(
                Update,
                (
                    load_sound_manifest_system,
                    add_spatial_listener_to_new_cameras,
                    audio_fx::spawn_car_engine_sounds,
                    audio_fx::manage_car_engine_sound_pitch_volume,
                    audio_fx::manage_footsteps_system,
                ),
            );
    }
}

/// Attach a [`SpatialListener`] to the scene camera so 3D sounds have a set of ears.
fn setup_spatial_listener(mut commands: Commands, cameras: Query<Entity, With<Camera3d>>) {
    for cam in cameras.iter() {
        let mut listener = SpatialListener::new(0.25);
        listener.left_ear_offset = Vec3::new(0.125, 0.0, 0.0);
        listener.right_ear_offset = Vec3::new(-0.125, 0.0, 0.0);
        commands.entity(cam).insert(listener);
    }
}

/// Automatically attach a [`SpatialListener`] to any newly spawned `Camera3d`.
fn add_spatial_listener_to_new_cameras(
    mut commands: Commands,
    cameras: Query<Entity, (Added<Camera3d>, Without<SpatialListener>)>,
) {
    for cam in cameras.iter() {
        let mut listener = SpatialListener::new(0.25);
        listener.left_ear_offset = Vec3::new(0.125, 0.0, 0.0);
        listener.right_ear_offset = Vec3::new(-0.125, 0.0, 0.0);
        commands.entity(cam).insert(listener);
    }
}

/// Kick off loading of the manifest text file.
fn start_sound_manifest_load(mut commands: Commands, asset_server: Res<AssetServer>) {
    let base = crate::config::DATA_BASE_URL.trim_end_matches('/');
    let folder = format!("{}/sound_data/sound-fx2/", base);
    let manifest_url = format!("{}manifest.txt", folder);
    info!("Loading sound manifest: {}", manifest_url);
    let manifest_handle = asset_server.load::<TextAsset>(manifest_url);
    commands.insert_resource(SoundManifestBootstrap {
        folder,
        manifest_handle,
    });
}

/// Parse the manifest text (once available) into [`SoundManifest`] and preload every clip.
fn load_sound_manifest_system(
    asset_server: Res<AssetServer>,
    bootstrap: Option<Res<SoundManifestBootstrap>>,
    text_assets: Res<Assets<TextAsset>>,
    mut manifest: ResMut<SoundManifest>,
    mut next_state: ResMut<NextState<SoundManifestLoadFinished>>,
) {
    if manifest.loaded {
        return;
    }
    let Some(bootstrap) = bootstrap else {
        return;
    };
    let Some(text_asset) = text_assets.get(&bootstrap.manifest_handle) else {
        return;
    };

    let mut sounds = Vec::new();
    for line in text_asset.text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(',');
        let name = parts.next().unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let attenuation = parts
            .next()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(1.0);
        let volume = parts
            .next()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .unwrap_or(1.0);

        let url = format!("{}{}", bootstrap.folder, name);
        let handle = asset_server.load::<AudioSource>(url.clone());
        sounds.push(SoundEntry {
            name,
            url,
            handle,
            attenuation,
            volume,
        });
    }

    info!("Parsed sound manifest: {} clips.", sounds.len());
    manifest.sounds = sounds;
    manifest.loaded = true;
    next_state.set(SoundManifestLoadFinished::Finished);
}

/// Spawn a spatial audio emitter (one-shot or looping/parented).
fn play_sound_observer(trigger: On<PlaySoundEvent>, mut commands: Commands) {
    let ev = trigger.event();
    let mode = if ev.looped {
        PlaybackMode::Loop
    } else {
        PlaybackMode::Despawn
    };

    let scale_factor = 1.0 / ev.attenuation.max(0.001);
    let playback_settings = PlaybackSettings {
        mode,
        volume: Volume::Linear(ev.volume),
        speed: ev.speed,
        spatial: true,
        spatial_scale: Some(SpatialScale(Vec3::splat(scale_factor))),
        ..default()
    };

    let mut emitter = commands.spawn((
        Name::new("SoundEmitter"),
        AudioPlayer::new(ev.handle.clone()),
        playback_settings,
    ));

    if let Some(parent_entity) = ev.follow {
        emitter.insert((ChildOf(parent_entity), Transform::IDENTITY));
    } else {
        emitter.insert(Transform::from_translation(ev.position));
    }
}

/// Demo UI + interaction state.
#[derive(Resource)]
pub struct AudioDemoState {
    /// Index into [`SoundManifest::sounds`] of the currently selected clip.
    pub selected: usize,
    /// Linear volume (slider).
    pub volume: f32,
    /// Playback speed (slider).
    pub speed: f32,
    /// Distance between the listener's ears in meters (slider).
    pub ear_gap: f32,
    /// `Time::elapsed_secs` of the last played sound, for debouncing.
    pub last_played: f32,
    /// Last picked ground position, drawn as a gizmo until the next click.
    pub last_pick: Option<Vec3>,
}

impl Default for AudioDemoState {
    fn default() -> Self {
        Self {
            selected: 0,
            volume: 1.0,
            speed: 1.0,
            ear_gap: 0.25,
            last_played: -1.0,
            last_pick: None,
        }
    }
}

/// Demo-specific UI plugin.
pub struct AudioDemoPlugin;

impl Plugin for AudioDemoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioDemoState>()
            .add_systems(
                Update,
                (
                    update_listener_ears,
                    scroll_select_sound,
                    click_ground_to_play,
                    draw_pick_gizmo,
                ),
            )
            .add_systems(EguiPrimaryContextPass, audio_demo_ui);
    }
}

/// Keep the listener's ear offsets in sync with the ear-distance slider.
fn update_listener_ears(state: Res<AudioDemoState>, mut listeners: Query<&mut SpatialListener>) {
    if !state.is_changed() {
        return;
    }
    for mut listener in listeners.iter_mut() {
        listener.left_ear_offset = Vec3::X * state.ear_gap / 2.0;
        listener.right_ear_offset = Vec3::X * state.ear_gap / -2.0;
    }
}

/// Mouse-wheel up/down cycles the selected clip.
fn scroll_select_sound(
    mut wheel: MessageReader<MouseWheel>,
    manifest: Res<SoundManifest>,
    mut state: ResMut<AudioDemoState>,
    mut contexts: EguiContexts,
) {
    if manifest.sounds.is_empty() {
        wheel.clear();
        return;
    }
    // Don't steal the wheel while the pointer is over the egui panel (it scrolls the list there).
    if let Ok(ctx) = contexts.ctx_mut() {
        if ctx.is_pointer_over_egui() {
            wheel.clear();
            return;
        }
    }

    let len = manifest.sounds.len();
    for ev in wheel.read() {
        if ev.y > 0.0 {
            state.selected = (state.selected + len - 1) % len;
        } else if ev.y < 0.0 {
            state.selected = (state.selected + 1) % len;
        }
    }
}

/// Left-click the ground: raycast, drop a gizmo and fire a [`PlaySoundEvent`] (debounced).
fn click_ground_to_play(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: avian3d::prelude::SpatialQuery,
    manifest: Res<SoundManifest>,
    mut state: ResMut<AudioDemoState>,
    mut contexts: EguiContexts,
) {
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }
    // Ignore clicks that land on the egui panel.
    if let Ok(ctx) = contexts.ctx_mut() {
        if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
            return;
        }
    }
    // Debounce rapid clicks.
    let now = time.elapsed_secs();
    if now - state.last_played < SOUND_DEBOUNCE_SECS {
        return;
    }

    let Some(entry) = manifest.sounds.get(state.selected).cloned() else {
        return;
    };
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };
    let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        10000.0,
        true,
        &avian3d::prelude::SpatialQueryFilter::default(),
    ) else {
        return;
    };

    let hit_point = ray.origin + *ray.direction * hit.distance;
    state.last_played = now;
    state.last_pick = Some(hit_point);

    info!(
        "Play '{}' at {:?} (vol {:.2}, speed {:.2})",
        entry.name, hit_point, state.volume, state.speed
    );
    commands.trigger(PlaySoundEvent {
        handle: entry.handle,
        position: hit_point,
        volume: state.volume,
        speed: state.speed,
        attenuation: entry.attenuation,
        follow: None,
        looped: false,
    });
}

/// Draw a marker at the last-picked ground location.
fn draw_pick_gizmo(state: Res<AudioDemoState>, mut gizmos: Gizmos) {
    let Some(p) = state.last_pick else {
        return;
    };
    let color = Color::srgb(0.2, 1.0, 0.6);
    gizmos.sphere(p, 0.3, color);
    gizmos.line(p, p + Vec3::Y * 1.5, color);
}

/// The demo control panel: volume/speed/ear-distance sliders and the scrollable clip list.
fn audio_demo_ui(
    mut contexts: EguiContexts,
    manifest: Res<SoundManifest>,
    mut state: ResMut<AudioDemoState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("🔊 Audio Demo")
        .default_width(320.0)
        .show(ctx, |ui| {
            if !manifest.loaded {
                ui.label("Loading sound manifest…");
                return;
            }

            ui.label(
                egui::RichText::new("Left-click the ground to play the selected sound in 3D.")
                    .size(11.0),
            );
            ui.separator();

            ui.add(
                egui::Slider::new(&mut state.volume, 0.0..=2.0)
                    .text("Volume")
                    .step_by(0.01),
            );
            ui.add(
                egui::Slider::new(&mut state.speed, 0.25..=3.0)
                    .text("Speed")
                    .step_by(0.01),
            );
            ui.add(
                egui::Slider::new(&mut state.ear_gap, 0.15..=0.35)
                    .text("Ear distance (m)")
                    .step_by(0.005),
            );

            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "Sounds ({}) — scroll wheel to cycle",
                    manifest.sounds.len()
                ))
                .strong(),
            );

            let selected = state.selected;
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (i, entry) in manifest.sounds.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let is_sel = i == selected;
                            if ui
                                .add_enabled(
                                    !is_sel,
                                    egui::Button::new(if is_sel { "✔" } else { "Select" }),
                                )
                                .clicked()
                            {
                                state.selected = i;
                            }
                            let label = egui::RichText::new(&entry.name);
                            let label = if is_sel {
                                label.color(egui::Color32::from_rgb(0, 220, 255)).strong()
                            } else {
                                label
                            };
                            ui.label(label);
                        });
                    }
                });
        });
}
