use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

pub struct MainScenePlugin;

impl Plugin for MainScenePlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MainScenePlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MainScenePlugin...");
        app
            .add_systems(
                Startup,
                (setup_camera_and_load, || {
                    crate::ui_egui::web_set_loading_status(false, "");
                }),
            );
        info!("done loading: MainScenePlugin");
    }
}


fn setup_camera_and_load(mut commands: Commands) {
    // Keep only default camera spawning
    commands.spawn((
        Transform::from_xyz(0.0, 10.5, -30.0).looking_at(Vec3::ZERO, Vec3::Y),
        Camera {
            clear_color: Color::BLACK.into(),
            ..default()
        },
        Camera3d::default(),
        Tonemapping::None,
    ));

    // Spawn directional light (sun)
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            ..default()
        },
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

}
