pub mod car_info;
pub mod click_spawn_select_controls;
pub mod driving_plugin;

use bevy::{app::App, prelude::*};

use crate::plugins::{
    cars_driving::click_spawn_select_controls::spawn_car_request_event_observer,
    states::GameControlState,
};

pub struct CarsAndDrivingPlugin;

impl Plugin for CarsAndDrivingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            click_spawn_select_controls::handle_click_raycast_spawn_car
                .run_if(in_state(GameControlState::MapFreecam)),
        );
        app.add_observer(spawn_car_request_event_observer);
        app.add_plugins(DrivingPlugin::<GameControlState> { state: GameControlState::DrivingCar });
    }
}
