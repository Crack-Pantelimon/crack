//! Traffic simulation test binary with debug driving controls.

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use std::collections::BTreeMap;

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        audio::GameAudioPlugin,
        cars_driving::CarsAndDrivingPlugin,
        geojson::{FeatureGeometry, GeoJsonDatabase, GeoJsonFeature},
        pedestrian_ai::PedestrianAiPlugin,
        pedestrians::pedestrian_controller_plugin::PedestrianControllerPlugin,
        physics_plugin::PhysicsPlugin,
        states::GameStatesPlugin,
        traffic::TrafficPlugin,
        weapons::WeaponsPlugin,
    },
    ui_egui::UiState,
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("Traffic Test")
        .add_plugins(bevy_egui::EguiPlugin::default())
        .insert_resource(UiState {
            show_traffic_debug: true,
            ..UiState::with_physics_debug()
        })
        .init_resource::<GeoJsonDatabase>()
        .add_plugins(PhysicsPlugin)
        .add_plugins(GameStatesPlugin)
        .add_plugins(CarsAndDrivingPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(TrafficPlugin)
        .add_plugins(PedestrianControllerPlugin)
        .add_plugins(WeaponsPlugin)
        .add_plugins(PedestrianAiPlugin)
        .add_plugins(GameAudioPlugin)
        .add_systems(
            Startup,
            (inject_hardcoded_intersection, force_loaded_states),
        )
        .add_systems(Update, simple_camera_movement)
        .run();
}

fn force_loaded_states(
    mut next_osm: ResMut<
        NextState<demo_resolution_selector_web_bevy::plugins::states::OsmDatabaseLoadFinished>,
    >,
    mut next_map: ResMut<
        NextState<demo_resolution_selector_web_bevy::plugins::states::InitialMapLoadFinished>,
    >,
) {
    next_osm.set(
        demo_resolution_selector_web_bevy::plugins::states::OsmDatabaseLoadFinished::OsmFinished,
    );
    next_map
        .set(demo_resolution_selector_web_bevy::plugins::states::InitialMapLoadFinished::Finished);
}

fn inject_hardcoded_intersection(mut database: ResMut<GeoJsonDatabase>) {
    let mut categories = BTreeMap::new();

    // 1. North-South Road: from (0, 0.5, -200) to (0, 0.5, 200) every 10m
    let mut ns_points = Vec::new();
    for i in -20..=20 {
        ns_points.push(Vec3::new(0.0, 0.5, i as f32 * 10.0));
    }

    // 2. East-West Road: from (-200, 0.5, 0) to (200, 0.5, 0) every 10m
    let mut ew_points = Vec::new();
    for i in -20..=20 {
        ew_points.push(Vec3::new(i as f32 * 10.0, 0.5, 0.0));
    }

    let ns_feature = GeoJsonFeature {
        id: Some(1),
        osm_type: "way".to_string(),
        name: Some("NS Road".to_string()),
        tags: {
            let mut t = BTreeMap::new();
            t.insert("highway".to_string(), "residential".to_string());
            t
        },
        geometry: FeatureGeometry::LineString(ns_points),
        center: Vec3::new(0.0, 0.5, 0.0),
        bbox_min: Vec3::new(-0.5, 0.5, -200.0),
        bbox_max: Vec3::new(0.5, 0.5, 200.0),
    };

    let ew_feature = GeoJsonFeature {
        id: Some(2),
        osm_type: "way".to_string(),
        name: Some("EW Road".to_string()),
        tags: {
            let mut t = BTreeMap::new();
            t.insert("highway".to_string(), "residential".to_string());
            t
        },
        geometry: FeatureGeometry::LineString(ew_points),
        center: Vec3::new(0.0, 0.5, 0.0),
        bbox_min: Vec3::new(-200.0, 0.5, -0.5),
        bbox_max: Vec3::new(200.0, 0.5, 0.5),
    };

    categories.insert("roads".to_string(), vec![ns_feature, ew_feature]);

    database.categories = categories;
    database.parsed = true;
    info!("Hardcoded intersection injected into GeoJsonDatabase");
}

fn simple_camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
    time: Res<Time>,
) {
    let Some(mut transform) = camera_query.iter_mut().next() else {
        return;
    };

    // Rotate
    if mouse_button.pressed(MouseButton::Left) {
        let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let sensitivity = 0.003;
        for event in mouse_motion.read() {
            yaw -= event.delta.x * sensitivity;
            pitch -= event.delta.y * sensitivity;
        }
        pitch = pitch.clamp(-89.9f32.to_radians(), 89.9f32.to_radians());
        transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    } else {
        for _ in mouse_motion.read() {}
    }

    // Translate
    let speed = 20.0;
    let mut forward = *transform.forward();
    forward.y = 0.0;
    let forward = forward.normalize_or_zero();
    let mut right = *transform.right();
    right.y = 0.0;
    let right = right.normalize_or_zero();

    if keyboard.pressed(KeyCode::KeyW) {
        transform.translation += forward * speed * time.delta_secs();
    }
    if keyboard.pressed(KeyCode::KeyS) {
        transform.translation -= forward * speed * time.delta_secs();
    }
    if keyboard.pressed(KeyCode::KeyA) {
        transform.translation -= right * speed * time.delta_secs();
    }
    if keyboard.pressed(KeyCode::KeyD) {
        transform.translation += right * speed * time.delta_secs();
    }
    if keyboard.pressed(KeyCode::Space) {
        transform.translation.y += speed * time.delta_secs();
    }
    if keyboard.pressed(KeyCode::ControlLeft) {
        transform.translation.y -= speed * time.delta_secs();
    }
}
