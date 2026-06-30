use crate::plugins::{
    cars_driving::driving_plugin::{
        CarDriveState, GamePhysicsLayer, Wheel,
        SuspensionPrismaticJoint, SuspensionDistanceJoint,
    },
    states::GameControlState,
};
use avian3d::prelude::{
    Collider, CollisionLayers, DistanceJoint, LinearMotor,
    MassPropertiesBundle, MotorModel, PrismaticJoint, RigidBody,
    SleepingDisabled, Friction, ColliderConstructorHierarchy, ColliderConstructor,
};
use avian3d::math::{Scalar, Vector};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use crate::plugins::cars_driving::car_info::{get_car_asset, get_random_car_type};

#[derive(Event)]
pub struct SpawnCarRequestEvent {
    pub position: Vec3,
    pub car_type: String,
}

#[derive(Component)]
pub struct Car {
    pub _car_type: String,
    pub physics_children: Vec<Entity>,
}

pub fn spawn_car_request_event_observer(
    spawn_car_event: On<SpawnCarRequestEvent>,
    mut commands: Commands,
    current_state: Res<State<GameControlState>>,
    mut next_state: ResMut<NextState<GameControlState>>,
    spatial_query: avian3d::prelude::SpatialQuery,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
    let wheel_radius = default_drive_state.wheel_radius;
    let wheel_width = default_drive_state.wheel_width;
    let car_mass = default_drive_state.car_mass;
    let wheel_mass = default_drive_state.wheel_mass;
    let suspension_min = default_drive_state.suspension_min;
    let suspension_max = default_drive_state.suspension_max;
    let suspension_rest = default_drive_state.suspension_rest;
    let suspension_stiffness = default_drive_state.suspension_stiffness;
    let suspension_damping = default_drive_state.suspension_damping;
    let wheel_y_offset = default_drive_state.wheel_y_offset;

    let car_body_volume = (car_half_width * 2.0) * (car_half_height * 2.0) * (car_half_length * 2.0);

    let mut physics_children = Vec::new();

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
            default_drive_state,
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        // .with_children(|parent| {
        //     parent.spawn((
        //         Transform::IDENTITY,
        //         Visibility::default(),
        //         InheritedVisibility::default(),
        //     ));
        // })
        .id();

    // Wheel offsets:
    // (offset, is_front, is_left, is_rear)
    let wheel_offsets_and_steer = [
        // Front (steers normal)
        (
            Vec3::new(-car_half_width, -car_half_height, -car_half_length),
            true,
            true,
            false,
        ), // FL
        (
            Vec3::new(car_half_width + 0.1, -car_half_height, -car_half_length),
            true,
            false,
            false,
        ), // FR
        // Back (no steer)
        (
            Vec3::new(-car_half_width, -car_half_height, car_half_length),
            false,
            true,
            false,
        ), // RL
        (
            Vec3::new(car_half_width, -car_half_height, car_half_length),
            false,
            false,
            false,
        ), // RR
    ];

    for (offset, is_front, is_left, _is_rear) in wheel_offsets_and_steer {
        let mut adjusted_offset = offset;
        adjusted_offset.y += wheel_y_offset;
        let world_offset = car_rot * adjusted_offset;
        let wheel_pos = pos + world_offset;

        // Wheel: standalone entity, connected to car body.
        let wheel_rot = car_rot * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let wheel_volume = std::f32::consts::PI * wheel_radius * wheel_radius * wheel_width;
        let wheel = commands
            .spawn((
                Mesh3d(meshes.add(Cylinder::new(wheel_radius, wheel_width))),
                MeshMaterial3d(materials.add(Color::srgb(0.15, 0.15, 0.15))),
                Transform::from_translation(wheel_pos).with_rotation(wheel_rot),
                RigidBody::Dynamic,
                MassPropertiesBundle::from_shape(
                    &Cylinder::new(wheel_radius, wheel_width),
                    wheel_mass / wheel_volume,
                ),
                Collider::cylinder(wheel_radius, wheel_width),
                CollisionLayers::new([GamePhysicsLayer::Wheel], [GamePhysicsLayer::Map]),
                Friction::new(0.05).with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
                SleepingDisabled,
                Wheel { is_front, is_left },
            ))
            .id();
        physics_children.push(wheel);

        let anchor_y = offset.y + wheel_y_offset;

        // Prismatic suspension joint connecting wheel directly to body
        let prismatic_joint = commands.spawn((
            PrismaticJoint::new(car_entity, wheel)
                .with_local_anchor1(Vector::new(offset.x, anchor_y, offset.z))
                .with_slider_axis(Vector::NEG_Y)
                .with_local_basis2(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2))
                .with_limits(suspension_min, suspension_max)
                .with_motor(
                    LinearMotor::new(MotorModel::SpringDamper {
                        frequency: suspension_stiffness,
                        damping_ratio: suspension_damping,
                    })
                    .with_target_position(suspension_rest)
                    .with_max_force(Scalar::MAX),
                ),
            SuspensionPrismaticJoint { is_front, is_left },
        )).id();
        physics_children.push(prismatic_joint);

        // Distance joint
        let distance_joint = commands.spawn((
            DistanceJoint::new(car_entity, wheel)
                .with_local_anchor1(Vector::new(offset.x, anchor_y, offset.z))
                .with_limits(suspension_min, suspension_max),
            SuspensionDistanceJoint { is_front, is_left },
        )).id();
        physics_children.push(distance_joint);
    }

    commands.entity(car_entity).insert(Car {
        _car_type: spawn_car_event.car_type.clone(),
        physics_children,
    });

    next_state.set(GameControlState::DrivingCar);
}
