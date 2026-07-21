//! Shows how to iterate over combinations of query results.

use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    render::{
        RenderPlugin,
        settings::{Backends, WgpuSettings},
    },
    window::WindowResolution,
};

/// Keep our own logs at INFO but silence the extremely chatty P2P/iroh
/// networking stack (and the usual wgpu/naga render spam). Without this the
/// gossip/relay/discovery crates flood the console once networking is up.
/// EnvFilter matches by target prefix, so `iroh=error` also covers
/// `iroh_gossip`, `iroh_relay`, etc.
const LOG_FILTER: &str = "info,wgpu=error,naga=warn,\
iroh=error,iroh_gossip=error,iroh_relay=error,iroh_quinn=error,\
iroh_net_report=error,iroh_base=error,\
netwatch=error,portmapper=error,net_report=error,swarm_discovery=error,\
quinn=error,quinn_proto=error,quinn_udp=error,\
hickory_proto=error,hickory_resolver=error,pkarr=error,mainline=error";

/// memory dir.
#[derive(Resource, Clone, Default)]
pub struct MemoryDir {
/// dir field.
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
            })
            .set(LogPlugin {
                level: Level::INFO,
                filter: LOG_FILTER.to_string(),
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

/// Create a headless variant of [`make_basic_app`]: same memory asset source,
/// asset meta-check, logging and clear color, but no window, no winit event
/// loop and no GPU backend (`backends: None` keeps the render types and the
/// `AssetServer` around without initializing a device). Used by the headless
/// smoke tests; do NOT insert `WinitSettings` here — it needs an event loop.
pub fn make_headless_app(title: &str) -> App {
    info!("exec make_headless_app({title})...");

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
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                render_creation: WgpuSettings {
                    backends: None,
                    ..default()
                }
                .into(),
                ..default()
            })
            .set(bevy::asset::io::web::WebAssetPlugin {
                silence_startup_warning: true,
            })
            .set(AssetPlugin {
                meta_check: bevy::asset::AssetMetaCheck::Never,
                ..default()
            })
            .set(LogPlugin {
                level: Level::INFO,
                filter: LOG_FILTER.to_string(),
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    )
    .insert_resource(ClearColor(Color::BLACK))
    .add_systems(Update, log_dt);

    // avian3d's collider-tree/spatial-query systems unconditionally require
    // these diagnostics resources (normally inserted alongside the render
    // sub-app, which doesn't exist here since `backends: None` skips
    // creating it). They're plain `Default` timing counters, so pre-inserting
    // them is harmless and lets physics run headless.
    app.init_resource::<avian3d::collider_tree::ColliderTreeDiagnostics>();
    app.init_resource::<avian3d::spatial_query::SpatialQueryDiagnostics>();

    app
}

/// Log times for slow frames, when they happen.
fn log_dt(time: Res<Time<Real>>, frames: Res<bevy::diagnostic::FrameCount>) {
    if (frames.0 < 120 && time.delta_secs() > 0.1) || time.delta_secs() > 2.0 {
        info!("slow frame: {} / dt: {}", frames.0, time.delta_secs());
    }
}
