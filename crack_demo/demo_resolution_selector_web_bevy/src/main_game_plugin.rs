use bevy::prelude::*;

pub struct MainGamePlugin;

impl Plugin for MainGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::ui_egui::UiEguiPlugin)
            .add_plugins(crate::plugins::main_scene_plugin::MainScenePlugin)
            .add_plugins(crate::plugins::game_freecam::camera_controls::CameraControlsPlugin)
            .add_plugins(crate::plugins::physics_plugin::PhysicsPlugin)
            .add_plugins(crate::plugins::map_plugin::MapPlugin)
            .add_plugins(crate::plugins::geojson::GeoJsonPlugin)
            .add_plugins(crate::plugins::cars_driving::CarsAndDrivingPlugin)
            .add_plugins(
                crate::plugins::pedestrians::pedestrian_controller_plugin::PedestrianControllerPlugin,
            )
            .add_plugins(crate::plugins::states::GameStatesPlugin);
    }
}
