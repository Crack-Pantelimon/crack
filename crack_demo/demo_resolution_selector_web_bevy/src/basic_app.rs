//! Shows how to iterate over combinations of query results.

use bevy::{
    prelude::*,
    render::{
        RenderPlugin,
        settings::{Backends, WgpuSettings},
    },
    window::WindowResolution,
};

#[derive(Resource, Clone, Default)]
pub struct MemoryDir {
    pub dir: bevy::asset::io::memory::Dir,
}

/// Create a basic app where we override only the DefaultPlugin, render settings, window reactivity settings.
pub fn make_basic_app(title: &str) -> App {
    info!("exec main_bevy()...");
    #[cfg(feature = "web")]
    let backends = Backends::GL;
    #[cfg(not(feature = "web"))]
    let backends = Backends::PRIMARY;

    info!("backends: {:?}", backends);

    let mut app = App::new();

    let memory_dir = MemoryDir::default();
    let reader = bevy::asset::io::memory::MemoryAssetReader {
        root: memory_dir.dir.clone(),
    };
    app.register_asset_source(
        bevy::asset::io::AssetSourceId::new(Some("memory")),
        bevy::asset::io::AssetSourceBuilder::new(move || Box::new(reader.clone())),
    );
    app.insert_resource(memory_dir);

    app.add_plugins(
        DefaultPlugins
            .build()
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: format!("Crack! - {title}"),
                    canvas: Some("#the-canvas".into()),
                    // resizable: true,
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: true,
                    resolution: WindowResolution::new(1280, 720).with_scale_factor_override(1.15),
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
        focused_mode: bevy::winit::UpdateMode::reactive(std::time::Duration::from_millis(16)),
        unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
            std::time::Duration::from_millis(666),
        ),
    })
    .insert_resource(ClearColor(Color::BLACK))
    .add_systems(Update, log_dt);
    app
}

/// Log times for slow frames, when they happen.
fn log_dt(time: Res<Time<Real>>, frames: Res<bevy::diagnostic::FrameCount>) {
    if (frames.0 < 120 && time.delta_secs() > 0.1) || time.delta_secs() > 2.0 {
        info!("slow frame: {} / dt: {}", frames.0, time.delta_secs());
    }
}
