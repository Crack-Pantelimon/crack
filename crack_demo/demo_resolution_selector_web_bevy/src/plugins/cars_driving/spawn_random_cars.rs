//! Scatters a handful of non-drivable "prop" cars (mesh + collider only) across the map.

use avian3d::prelude::{
    ColliderConstructor, ColliderConstructorHierarchy, CollisionLayers, MassPropertiesBundle,
    RigidBody, SpatialQuery, SpatialQueryFilter,
};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use crate::plugins::{
    cars_driving::{car_info::get_car_asset, driving_plugin::GamePhysicsLayer},
    map_plugin::MapTree,
};

/// How many prop cars to scatter.
const NUM_RANDOM_CARS: usize = 8;
/// Approximate car body extents (matches `CarDriveState` defaults) used for the mass density.
const CAR_SIZE: Vec3 = Vec3::new(1.8, 1.0, 3.04);
const CAR_MASS: f32 = 1200.0;

/// Spawns [`NUM_RANDOM_CARS`] prop cars once the map is parsed.
pub fn spawn_random_cars(
    mut commands: Commands,
    map: Res<MapTree>,
    spatial_query: SpatialQuery,
    asset_server: Res<AssetServer>,
    mut done: Local<bool>,
) {
    if *done || !map.parsed {
        return;
    }
    *done = true;

    let volume = CAR_SIZE.x * CAR_SIZE.y * CAR_SIZE.z;
    let density = CAR_MASS / volume;

    for _ in 0..NUM_RANDOM_CARS {
        let x = rand::random::<f32>() * (map.bbox.max.x - map.bbox.min.x) + map.bbox.min.x;
        let z = rand::random::<f32>() * (map.bbox.max.z - map.bbox.min.z) + map.bbox.min.z;

        // Find the ground height, then drop the car from 3m above it.
        let start_y = map.bbox.max.y + 1.0;
        let ground_y = spatial_query
            .cast_ray(
                Vec3::new(x, start_y, z),
                Dir3::NEG_Y,
                (map.bbox.max.y - map.bbox.min.y) + 10.0,
                true,
                &SpatialQueryFilter::default(),
            )
            .map(|hit| start_y - hit.distance)
            .unwrap_or(map.bbox.min.y);

        let pos = Vec3::new(x, ground_y + 3.0, z);
        let rot = Quat::from_rotation_y(rand::random::<f32>() * std::f32::consts::TAU);
        let car_asset = get_car_asset(crate::plugins::cars_driving::car_info::get_random_car_type(), &asset_server);

        commands.spawn((
            Name::new("PropCar"),
            Transform::from_translation(pos).with_rotation(rot),
            RigidBody::Dynamic,
            MassPropertiesBundle::from_shape(
                &Cuboid::new(CAR_SIZE.x, CAR_SIZE.y, CAR_SIZE.z),
                density,
            ),
            WorldAssetRoot(car_asset),
            ColliderConstructorHierarchy::new(ColliderConstructor::ConvexDecompositionFromMesh)
                .with_default_layers(CollisionLayers::new(
                    [GamePhysicsLayer::Car],
                    [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
                )),
            CollisionLayers::new(
                [GamePhysicsLayer::Car],
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
            Visibility::default(),
            InheritedVisibility::default(),
        ));
    }

    info!("Spawned {NUM_RANDOM_CARS} random prop cars.");
}
