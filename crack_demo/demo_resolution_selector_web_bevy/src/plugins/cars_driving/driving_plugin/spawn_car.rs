use crate::plugins::cars_driving::car_info::{get_car_asset, get_random_car_type, get_wheel_asset};
use crate::plugins::{
    cars_driving::driving_plugin::{
        CarDriveState, CarSpeculativeContactData, CarWheelsContactData, GamePhysicsLayer,
    },
    states::GameControlState,
};
use avian3d::{
    dynamics::ccd::SweptCcd,
    prelude::{
        AngularVelocity, ColliderConstructor, ColliderConstructorHierarchy, CollisionEventsEnabled,
        CollisionLayers, LinearVelocity, MassPropertiesBundle, RigidBody, SleepingDisabled,
    },
};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

#[derive(Event)]
pub struct SpawnCarRequestEvent {
    pub position: Vec3,
    pub car_type: String,
    pub rotation: Option<Quat>,
}

#[derive(Component)]
pub struct Car {
    pub _car_type: String,
}

pub fn spawn_physics_car(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    pos: Vec3,
    car_rot: Quat,
    car_type: &str,
) -> Entity {
    let default_drive_state = CarDriveState::default();
    let car_half_width = default_drive_state.car_half_width;
    let car_half_height = default_drive_state.car_half_height;
    let car_half_length = default_drive_state.car_half_length;

    let car_mass = default_drive_state.car_mass;

    let car_body_volume =
        (car_half_width * 2.0) * (car_half_height * 2.0) * (car_half_length * 2.0);

    let car_asset_handle = get_car_asset(car_type, asset_server);

    let car_entity = commands
        .spawn((
            (
                Transform::from_translation(pos).with_rotation(car_rot),
                RigidBody::Dynamic,
                LinearVelocity::ZERO,
                AngularVelocity::ZERO,
                MassPropertiesBundle::from_shape(
                    &Cuboid::new(
                        car_half_width * 2.0,
                        car_half_height * 2.0,
                        car_half_length * 2.0,
                    ),
                    car_mass / car_body_volume,
                ),
                WorldAssetRoot(car_asset_handle),
            ),
            (
                ColliderConstructorHierarchy::new(ColliderConstructor::ConvexDecompositionFromMesh)
                    .with_default_layers(CollisionLayers::new(
                        [GamePhysicsLayer::Car],
                        [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
                    )),
                CollisionLayers::new(
                    [GamePhysicsLayer::Car],
                    [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
                ),
                CollisionEventsEnabled,
                SleepingDisabled,
                SweptCcd::default(),
            ),
            (
                default_drive_state.clone(),
                CarWheelsContactData::default(),
                CarSpeculativeContactData::default(),
                NeedCarBoundsCompute,
                Visibility::default(),
                InheritedVisibility::default(),
            ),
            Car {
                _car_type: car_type.to_string(),
            },
        ))
        .id();

    let wheel_names = ["car-wheel_00003_", "car-wheel_00005_"];
    let selected_wheel_name = if rand::random::<bool>() {
        wheel_names[0]
    } else {
        wheel_names[1]
    };
    let wheel_handle = get_wheel_asset(selected_wheel_name, asset_server);

    for i in 0..4 {
        commands.spawn((
            bevy::world_serialization::WorldAssetRoot(wheel_handle.clone()),
            Transform::from_scale(Vec3::splat(0.6)),
            crate::plugins::cars_driving::driving_plugin::CosmeticWheel {
                wheel_idx: i,
                parent_car: car_entity,
                accumulated_rotation: 0.0,
                measured_radius: None,
            },
            Visibility::default(),
            InheritedVisibility::default(),
        ));
    }

    car_entity
}

pub fn spawn_car_request_event_observer(
    spawn_car_event: On<SpawnCarRequestEvent>,
    mut commands: Commands,
    current_state: Res<State<GameControlState>>,
    mut next_state: ResMut<NextState<GameControlState>>,
    spatial_query: avian3d::prelude::SpatialQuery,
    asset_server: Res<AssetServer>,
    q_active_cars: Query<Entity, With<ActivePlayerVehicle>>,
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

    let car_rot = spawn_car_event.rotation.unwrap_or_else(|| {
        let random_angle = rand::random::<f32>() * std::f32::consts::TAU;
        Quat::from_rotation_y(random_angle)
    });

    let car_type = get_random_car_type();
    let car_entity = spawn_physics_car(&mut commands, &asset_server, pos, car_rot, car_type);

    // Remove ActivePlayerVehicle from any existing cars
    for old_car in q_active_cars.iter() {
        commands.entity(old_car).remove::<ActivePlayerVehicle>();
    }

    // Mark as active player vehicle so camera follows and player can drive immediately
    commands.entity(car_entity).insert(ActivePlayerVehicle);

    next_state.set(GameControlState::DrivingCar);
}

#[derive(Component)]
pub struct NeedCarBoundsCompute;

#[derive(Component)]
pub struct ActivePlayerVehicle;

pub fn init_cars_system(
    mut commands: Commands,
    query: Query<(Entity, &NeedCarBoundsCompute, &Children)>,
    children_query: Query<&Children>,
    mesh_query: Query<&Mesh3d>,
    global_transform_query: Query<&GlobalTransform>,
    mut drive_state_query: Query<&mut CarDriveState>,
    meshes: Res<Assets<Mesh>>,
) {
    for (root_entity, _, children) in query.iter() {
        let mut mesh_entities = Vec::new();
        let mut queue: Vec<Entity> = children.to_vec();
        while let Some(ent) = queue.pop() {
            if let Ok(m) = mesh_query.get(ent) {
                mesh_entities.push((ent, m.0.clone()));
            }
            if let Ok(kids) = children_query.get(ent) {
                queue.extend(kids.iter());
            }
        }

        if mesh_entities.is_empty() {
            continue;
        }

        let mut all_meshes_loaded = true;
        for (_, handle) in &mesh_entities {
            if meshes.get(handle).is_none() {
                all_meshes_loaded = false;
                break;
            }
        }

        if !all_meshes_loaded {
            continue;
        }

        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        let mut found = false;

        let Ok(root_gt) = global_transform_query.get(root_entity) else {
            continue;
        };
        let root_inv = root_gt.to_matrix().inverse();

        for (ent, handle) in &mesh_entities {
            let Ok(mesh_gt) = global_transform_query.get(*ent) else {
                continue;
            };
            if let Some(mesh) = meshes.get(handle) {
                if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                {
                    for pos in positions {
                        let world_pos = mesh_gt.transform_point(Vec3::from(*pos));
                        let rel_pos = root_inv.transform_point3(world_pos);
                        min_y = min_y.min(rel_pos.y);
                        max_y = max_y.max(rel_pos.y);
                        found = true;
                    }
                }
            }
        }

        if found {
            if let Ok(mut drive_state) = drive_state_query.get_mut(root_entity) {
                let car_height = max_y - min_y;
                drive_state.ray_start_y_offset = min_y + (car_height * 0.10);
            }
        }

        commands
            .entity(root_entity)
            .remove::<NeedCarBoundsCompute>();
    }
}
