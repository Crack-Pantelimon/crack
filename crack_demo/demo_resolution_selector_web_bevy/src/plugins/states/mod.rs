use bevy::prelude::*;

/// initial map load finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum InitialMapLoadFinished {
/// loading variant.
    #[default]
    Loading,
/// finished variant.
    Finished,
}

/// osm database load finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum OsmDatabaseLoadFinished {
/// loading variant.
    #[default]
    Loading,
/// map finished variant.
    MapFinished,
/// osm finished variant.
    OsmFinished,
}

/// Tracks whether the sound-fx manifest has finished loading its list of clip paths.
/// Flipped to `Finished` by [`crate::plugins::audio::AudioDemoPlugin`] once the manifest
/// text has been fetched and parsed into the sound resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum SoundManifestLoadFinished {
/// loading variant.
    #[default]
    Loading,
/// finished variant.
    Finished,
}

/// The exclusive top-level control mode. `DrivingCar` and `ControllingPedestrian` are mutually
/// exclusive since they are values of the same state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum GameControlState {
/// map freecam variant.
    #[default]
    MapFreecam,
/// driving car variant.
    DrivingCar,
/// controlling pedestrian variant.
    ControllingPedestrian,
    // todo: spectating, cutscene, etc.
}

/// Whether the p2p network (global matchmaker / chat) has connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum NetworkConnectionState {
/// connecting variant.
    #[default]
    Connecting,
/// connected variant.
    Connected,
}

/// mouse capture state.
#[derive(Resource, Default)]
pub struct MouseCaptureState {
/// is captured field.
    pub is_captured: bool,
}

/// update mouse capture.
pub fn update_mouse_capture(
    mut capture_state: ResMut<MouseCaptureState>,
    state: Res<State<GameControlState>>,
    mut q_window: Query<
        (&mut Window, &mut bevy::window::CursorOptions),
        With<bevy::window::PrimaryWindow>,
    >,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: bevy_egui::EguiContexts,
) {
    // No primary window (e.g. the headless test app) — nothing to grab/show.
    let Ok((mut window, mut cursor_options)) = q_window.single_mut() else {
        return;
    };

    let current_state = *state.get();
    let is_grab_mode = matches!(
        current_state,
        GameControlState::ControllingPedestrian | GameControlState::DrivingCar
    );

    if state.is_changed() {
        if is_grab_mode {
            capture_state.is_captured = true;
        } else {
            capture_state.is_captured = false;
        }
    }

    let egui_wants_pointer = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
    } else {
        false
    };

    let egui_wants_keyboard = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_keyboard_input()
    } else {
        false
    };

    if is_grab_mode {
        // If Escape is pressed (and not focused in UI), release capture
        if keys.just_pressed(KeyCode::Escape) && !egui_wants_keyboard {
            if capture_state.is_captured {
                capture_state.is_captured = false;
            }
        }
        // If clicking outside UI, capture again
        if mouse_buttons.just_pressed(MouseButton::Left) && !egui_wants_pointer {
            capture_state.is_captured = true;
        }

        // Apply grab and visibility to the primary window
        if capture_state.is_captured {
            let grab_mode = bevy::window::CursorGrabMode::Locked;
            if cursor_options.grab_mode != grab_mode {
                cursor_options.grab_mode = grab_mode;
            }
            if cursor_options.visible {
                cursor_options.visible = false;
            }
            let width = window.width();
            let height = window.height();
            window.set_cursor_position(Some(Vec2::new(width / 2.0, height / 2.0)));
        } else {
            let grab_mode = bevy::window::CursorGrabMode::None;
            if cursor_options.grab_mode != grab_mode {
                cursor_options.grab_mode = grab_mode;
            }
            if !cursor_options.visible {
                cursor_options.visible = true;
            }
        }
    } else {
        // Not in a grab state
        capture_state.is_captured = false;
        let grab_mode = bevy::window::CursorGrabMode::None;
        if cursor_options.grab_mode != grab_mode {
            cursor_options.grab_mode = grab_mode;
        }
        if !cursor_options.visible {
            cursor_options.visible = true;
        }
    }
}

/// game states plugin.
pub struct GameStatesPlugin;

impl Plugin for GameStatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<InitialMapLoadFinished>();
        app.init_state::<OsmDatabaseLoadFinished>();
        app.init_state::<GameControlState>();
        app.init_resource::<MouseCaptureState>();
        app.add_systems(Update, update_mouse_capture);
        // Load the pedestrian manifest + animation catalog as part of app startup, so the
        // pedestrian models and animations are ready whenever the player spawns one.
        app.add_plugins(crate::plugins::pedestrians::PedestriansPlugin);
    }
}
