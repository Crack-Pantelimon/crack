use bevy::prelude::*;

pub struct MainGamePlugin;

impl Plugin for MainGamePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(crate::plugins::pedestrian_ai::faction::WarMatrix::gang_wars())
            .add_plugins(crate::ui_egui::UiEguiPlugin)
            .add_plugins(crate::plugins::main_scene_plugin::MainScenePlugin)
            .add_plugins(crate::plugins::game_freecam::camera_controls::CameraControlsPlugin)
            .add_plugins(crate::plugins::physics_plugin::PhysicsPlugin)
            .add_plugins(crate::plugins::crack_plugin::CrackPlugin)
            .add_plugins(crate::plugins::map_plugin::MapPlugin)
            .add_plugins(crate::plugins::geojson::GeoJsonPlugin)
            .add_plugins(crate::plugins::cars_driving::CarsAndDrivingPlugin)
            .add_plugins(crate::plugins::traffic::TrafficPlugin)
            .add_plugins(
                crate::plugins::pedestrians::pedestrian_controller_plugin::PedestrianControllerPlugin,
            )
            .add_plugins(crate::plugins::weapons::WeaponsPlugin)
            .add_plugins(crate::plugins::pedestrian_ai::PedestrianAiPlugin)
            .add_plugins(crate::plugins::audio::GameAudioPlugin)
            .add_plugins(crate::plugins::states::GameStatesPlugin);
    }
}
