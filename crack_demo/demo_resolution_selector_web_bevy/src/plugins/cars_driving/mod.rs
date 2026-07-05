pub mod car_info;
pub mod click_spawn_select_controls;
pub mod driving_plugin;
pub mod spawn_random_cars;
use bevy::{app::App, prelude::*};

use crate::plugins::cars_driving::{
    driving_plugin::spawn_car::spawn_car_request_event_observer,
    driving_plugin::{DrivingPlugin, car_drive_observer},
};
use crate::plugins::states::GameControlState;

pub struct CarsAndDrivingPlugin;

impl Plugin for CarsAndDrivingPlugin {
    fn build(&self, app: &mut App) {
        // Right-clicking the map now opens a "spawn pedestrian / spawn car" popup
        // (see the pedestrian controller plugin) instead of spawning a car directly.
        let _ = click_spawn_select_controls::handle_click_raycast_spawn_car;
        app.add_systems(Update, spawn_random_cars::spawn_random_cars);
        app.add_observer(spawn_car_request_event_observer);
        app.add_observer(car_drive_observer);
        app.add_plugins(DrivingPlugin::<GameControlState> {
            state: GameControlState::DrivingCar,
        });
    }
}
