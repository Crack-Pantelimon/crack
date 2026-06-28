use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::core_pipeline::Skybox;
use bevy::render::render_resource::{TextureViewDescriptor, TextureViewDimension};
use bevy::prelude::*;

pub struct MainScenePlugin;

impl Plugin for MainScenePlugin {
    fn build(&self, app: &mut App) {
        info!("loading: MainScenePlugin...");
        crate::ui_egui::web_set_loading_status(true, "Loading MainScenePlugin...");
        app.add_systems(
            Startup,
            (setup_camera_and_load, || {
                crate::ui_egui::web_set_loading_status(false, "");
            }),
        );
        app.add_systems(Update, convert_and_apply_skybox);
        info!("done loading: MainScenePlugin");
    }
}

#[derive(Resource)]
pub struct SkyboxState {
    pub handle: Handle<Image>,
    pub loaded: bool,
}

fn setup_camera_and_load(mut commands: Commands, asset_server: Res<AssetServer>) {
    let skybox_texture_url = format!("{}skybox_clouds.png", crate::config::DATA_BASE_URL);
    let skybox_handle = asset_server.load(skybox_texture_url);
    
    commands.insert_resource(SkyboxState {
        handle: skybox_handle,
        loaded: false,
    });

    // Keep only default camera spawning with Skybox component
    commands.spawn((
        Transform::from_xyz(0.0, 10.5, -30.0).looking_at(Vec3::ZERO, Vec3::Y),
        Camera {
            clear_color: Color::BLACK.into(),
            ..default()
        },
        Camera3d::default(),
        Tonemapping::None,
        Skybox {
            image: None,
            brightness: 1000.0,
            ..default()
        },
        AmbientLight {
            color: Color::srgb(0.75, 0.85, 1.0),
            brightness: 1000.0,
            ..default()
        },
    ));

    // Spawn a weak directional light pointing at a 45-degree angle to all axes
    commands.spawn((
        DirectionalLight {
            illuminance: 2000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(100.0, 100.0, 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn convert_and_apply_skybox(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    state: Option<ResMut<SkyboxState>>,
    mut q_camera: Query<(Entity, &mut Skybox), With<Camera3d>>,
) {
    let Some(mut state) = state else {
        return;
    };
    
    if state.loaded {
        return;
    }
    
    if let Some(load_state) = asset_server.get_load_state(&state.handle) {
        if matches!(load_state, bevy::asset::LoadState::Loaded) {
            if let Some(mut image) = images.get_mut(&state.handle) {
                let layers = image.height() / image.width();
                if layers == 6 {
                    if let Err(err) = image.reinterpret_stacked_2d_as_array(layers) {
                        error!("Failed to reinterpret skybox image: {:?}", err);
                    } else {
                        image.texture_view_descriptor = Some(TextureViewDescriptor {
                            dimension: Some(TextureViewDimension::Cube),
                            ..default()
                        });
                        
                        info!("Skybox cubemap configured successfully! Applying to camera...");
                        for (entity, mut skybox) in &mut q_camera {
                            skybox.image = Some(state.handle.clone());
                            
                            // Insert EnvironmentMapLight so that the skybox influences the lighting
                            commands.entity(entity).insert(EnvironmentMapLight {
                                diffuse_map: state.handle.clone(),
                                specular_map: state.handle.clone(),
                                intensity: 2000.0,
                                ..default()
                            });
                        }
                        state.loaded = true;
                    }
                } else {
                    error!(
                        "Skybox image has invalid aspect ratio for cubemap (layers: {}, width: {}, height: {})",
                        layers,
                        image.width(),
                        image.height()
                    );
                }
            }
        }
    }
}
