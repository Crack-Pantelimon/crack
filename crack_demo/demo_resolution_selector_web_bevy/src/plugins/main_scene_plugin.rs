use bevy::core_pipeline::Skybox;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureViewDescriptor, TextureViewDimension};

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
        //         bevy::post_process::motion_blur::MotionBlur {
        //     shutter_angle: 1.0,
        //     samples: 2,
        // },
        Msaa::Off,
    ));

    // Spawn a directional light pointing at a 45-degree angle to all axes
    commands.spawn((
        DirectionalLight {
            illuminance: 3500.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(10.0, 20.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Load and spawn custom assets next to each other
    let base_url = crate::config::DATA_BASE_URL;

    // 1. Kebab Shop (Height = 3.0m, offset up by 1.5m, moved to Z = -20125.0)
    let handle_kebab = asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!(
        "{}/blender_generated/kebab_shop/kebab_shop.glb",
        base_url
    )));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_kebab),
        Transform::from_translation(Vec3::new(-1050.0, 3363.0, -20125.0)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));

    // 2. Superbet Shop (Height = 3.0m, offset up by 1.5m, moved to Z = -20135.0)
    let handle_superbet =
        asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!(
            "{}/blender_generated/superbet_shop/superbet_shop.glb",
            base_url
        )));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_superbet),
        Transform::from_translation(Vec3::new(-1050.0, 3363.0, -20135.0)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));

    // 3. Terasa Obor (Height = 3.5m, offset up by 1.75m, moved to Z = -20125.0)
    let handle_obor = asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!(
        "{}/blender_generated/terasa_obor/terasa_obor.glb",
        base_url
    )));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_obor),
        Transform::from_translation(Vec3::new(-1070.0, 3363.25, -20125.0)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
    ));

    // 4. Bus 335 (Height = 2.8m, offset up by 1.4m, moved to Z = -20135.0) - Kinematic + tagged with Bus335Marker
    let handle_bus = asset_server.load(bevy::gltf::GltfAssetLabel::Scene(0).from_asset(format!(
        "{}/blender_generated/bus_335/bus_335.glb",
        base_url
    )));
    commands.spawn((
        bevy::world_serialization::WorldAssetRoot(handle_bus),
        Transform::from_translation(Vec3::new(-1070.0, 3362.9, -20135.0)),
        avian3d::prelude::RigidBody::Kinematic,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        ),
        crate::plugins::geojson::Bus335Marker,
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
                                intensity: 200.0,
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
