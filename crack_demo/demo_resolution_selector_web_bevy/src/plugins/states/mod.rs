use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum InitialMapLoadFinished {
    #[default]
    Loading,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum OsmDatabaseLoadFinished {
    #[default]
    Loading,
    MapFinished,
    OsmFinished,
}

/// The exclusive top-level control mode. `DrivingCar` and `ControllingPedestrian` are mutually
/// exclusive since they are values of the same state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum GameControlState {
    #[default]
    MapFreecam,
    DrivingCar,
    ControllingPedestrian,
    // todo: spectating, cutscene, etc.
}

pub struct GameStatesPlugin;

impl Plugin for GameStatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<InitialMapLoadFinished>();
        app.init_state::<OsmDatabaseLoadFinished>();
        app.init_state::<GameControlState>();
        // Load the pedestrian manifest + animation catalog as part of app startup, so the
        // pedestrian models and animations are ready whenever the player spawns one.
        app.add_plugins(crate::plugins::pedestrians::PedestriansPlugin);
    }
}
