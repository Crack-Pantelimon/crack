use crate::plugins::cars_driving::car_info::{get_car_asset, get_random_car_type};
use crate::plugins::{
    cars_driving::driving_plugin::{CarDriveState, CarWheelsContactData, GamePhysicsLayer},
    states::GameControlState,
};
use avian3d::{
    dynamics::ccd::SweptCcd,
    prelude::{
        ColliderConstructor, ColliderConstructorHierarchy, CollisionLayers,
        MassPropertiesBundle, RigidBody, SleepingDisabled,
    },
};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

#[derive(Event)]
pub struct SpawnCarRequestEvent {
    pub position: Vec3,
    pub car_type: String,
}

#[derive(Component)]
pub struct Car {
    pub _car_type: String,
}

pub fn spawn_car_request_event_observer(
    spawn_car_event: On<SpawnCarRequestEvent>,
    mut commands: Commands,
    current_state: Res<State<GameControlState>>,
    mut next_state: ResMut<NextState<GameControlState>>,
    spatial_query: avian3d::prelude::SpatialQuery,
    asset_server: Res<AssetServer>,
) {
    if current_state.get() != &GameControlState::MapFreecam {
        return;
    }
    let mut pos = spawn_car_event.position;

    // Raycast down from pos.y + 100.0 to find exact ground height
    let start_y = pos.y + 100.0;
    let ray_origin = Vec3::new(pos.x, start_y, pos.z);
    let filter = avian3d::prelude::SpatialQueryFilter::default();

    if let Some(hit) = spatial_query.cast_ray(
        ray_origin,
        bevy::prelude::Dir3::NEG_Y,
        1000.0,
        true,
        &filter,
    ) {
        let ground_y = start_y - hit.distance;
        pos.y = ground_y + 3.0;
    } else {
        pos.y += 3.0;
    }

    let random_angle = rand::random::<f32>() * std::f32::consts::TAU;
    let car_rot = Quat::from_rotation_y(random_angle);

    let default_drive_state = CarDriveState {
        spawn_position: Some(pos),
        ..default()
    };

    let car_half_width = default_drive_state.car_half_width;
    let car_half_length = default_drive_state.car_half_length;
    let car_half_height = default_drive_state.car_half_height;
    let car_mass = default_drive_state.car_mass;

    let car_body_volume =
        (car_half_width * 2.0) * (car_half_height * 2.0) * (car_half_length * 2.0);

    let car_type = get_random_car_type();
    let car_asset_handle = get_car_asset(car_type, &asset_server);

    let car_entity = commands
        .spawn((
            Transform::from_translation(pos).with_rotation(car_rot),
            RigidBody::Dynamic,
            MassPropertiesBundle::from_shape(
                &Cuboid::new(
                    car_half_width * 2.0,
                    car_half_height * 2.0,
                    car_half_length * 2.0,
                ),
                car_mass / car_body_volume,
            ),
            WorldAssetRoot(car_asset_handle),
            ColliderConstructorHierarchy::new(ColliderConstructor::ConvexDecompositionFromMesh)
                .with_default_layers(CollisionLayers::new(
                    [GamePhysicsLayer::Car],
                    [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
                )),
            CollisionLayers::new(
                [GamePhysicsLayer::Car],
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
            SleepingDisabled,
            SweptCcd::default(),
            default_drive_state.clone(),
            CarWheelsContactData::default(),
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    commands.entity(car_entity).insert(Car {
        _car_type: spawn_car_event.car_type.clone(),
    });

    next_state.set(GameControlState::DrivingCar);
}
