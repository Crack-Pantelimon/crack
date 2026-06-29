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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum GameControlState {
    #[default]
    MapFreecam,
    DrivingCar,
    // todo: walking, spectating, cutscene, etc.
}

pub struct GameStatesPlugin;

impl Plugin for GameStatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<InitialMapLoadFinished>();
        app.init_state::<OsmDatabaseLoadFinished>();
        app.init_state::<GameControlState>();
    }
}
