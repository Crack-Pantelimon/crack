//! Shows how to iterate over combinations of query results.

use bevy::{
    prelude::*,
    render::{
        RenderPlugin,
        settings::{Backends, WgpuSettings},
    },
    window::WindowResolution,
};
pub fn main_bevy() {
    info!("exec main_bevy()...");
    #[cfg(feature = "web")]
    let backends = Backends::GL;
    #[cfg(not(feature = "web"))]
    let backends = Backends::PRIMARY;

    info!("backends: {:?}", backends);

    App::new()
        .add_plugins(
            DefaultPlugins
                .build()
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "SECONDARY PROLETARIAN ROUTINE".into(),
                        canvas: Some("#the-canvas".into()),
                        // resizable: true,
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: false,
                        resolution: WindowResolution::new(1280, 720)
                            .with_scale_factor_override(1.15),
                        ..default()
                    }),
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: bevy::render::settings::RenderCreation::Automatic(Box::new(
                        WgpuSettings {
                            backends: Some(backends),
                            ..default()
                        },
                    )),
                    ..default()
                })
                .set(bevy::asset::io::web::WebAssetPlugin {
                    silence_startup_warning: true,
                })
                .set(AssetPlugin {
                    meta_check: bevy::asset::AssetMetaCheck::Never,
                    ..default()
                }),
        )
        .insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::reactive(std::time::Duration::from_millis(20)),
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_millis(666),
            ),
        })
        .add_plugins(crate::ui_egui::UiEguiPlugin)
        .add_plugins(crate::plugins::main_scene_plugin::MainScenePlugin)
        .add_plugins(crate::plugins::camera_controls::CameraControlsPlugin)
        .add_plugins(crate::plugins::physics_plugin::PhysicsPlugin)
        .add_plugins(crate::plugins::map_plugin::MapPlugin)
        .add_plugins(crate::plugins::mission_plugin::MissionPlugin)
        .insert_resource(ClearColor(Color::BLACK))
        .add_systems(Update, log_dt)
        .run();
}

fn log_dt(time: Res<Time<Real>>, frames: Res<bevy::diagnostic::FrameCount>) {
    if (frames.0 < 120 && time.delta_secs() > 0.1) || time.delta_secs() > 2.0 {
        info!("slow frame: {} / dt: {}", frames.0, time.delta_secs());
    }
}
