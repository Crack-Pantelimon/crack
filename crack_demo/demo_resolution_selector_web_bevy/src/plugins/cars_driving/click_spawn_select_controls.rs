use crate::plugins::{
    cars_driving::{
        car_info::{get_car_asset, get_random_car_type},
        driving_plugin::{CarDriveState, GamePhysicsLayer, Wheel, SuspensionJoint, SteeringJoint, AxleJoint},
    },
    states::GameControlState,
};
use bevy::prelude::*;
use bevy_egui::EguiContexts;
use avian3d::prelude::{
    RigidBody, Mass, CollisionLayers, SweptCcd, SleepingDisabled, PrismaticJoint, RevoluteJoint,
    AngularMotor, LinearMotor, MotorModel, Friction, CoefficientCombine, Restitution
};

pub fn handle_click_raycast_spawn_car(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: avian3d::prelude::SpatialQuery,
    mut contexts: EguiContexts,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
        return;
    }

    if mouse_button.just_pressed(MouseButton::Right) {
        let Ok(window) = window_query.single() else {
            return;
        };
        if let Some(cursor_pos) = window.cursor_position() {
            let Ok((camera, camera_transform)) = camera_query.single() else {
                return;
            };

            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if let Some(hit) = spatial_query.cast_ray(
                    ray.origin,
                    ray.direction,
                    10000.0,
                    true,
                    &avian3d::prelude::SpatialQueryFilter::default(),
                ) {
                    let hit_point = ray.origin + *ray.direction * hit.distance;
                    // lod_state.reference_points.push(hit_point);
                    info!("Spawn car at {:?}", hit_point);
                    commands.trigger(SpawnCarRequestEvent {
                        position: hit_point,
                        car_type: get_random_car_type().to_string(),
                    });
                }
            }
        }
    }
}

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
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    current_state: Res<State<GameControlState>>,
    mut next_state: ResMut<NextState<GameControlState>>,
    spatial_query: avian3d::prelude::SpatialQuery,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if current_state.get() != &GameControlState::MapFreecam {
        return;
    }
    let mut pos = spawn_car_event.position;

    // Raycast down from pos.y + 100.0 to find exact ground height
    let start_y = pos.y + 100.0;
    let ray_origin = Vec3::new(pos.x, start_y, pos.z);
    let filter = avian3d::prelude::SpatialQueryFilter::default();

    if let Some(hit) = spatial_query.cast_ray(ray_origin, bevy::prelude::Dir3::NEG_Y, 1000.0, true, &filter) {
        let ground_y = start_y - hit.distance;
        pos.y = ground_y + 9.0;
    } else {
        pos.y += 9.0;
    }

    let handle = get_car_asset(&spawn_car_event.car_type, &asset_server);

    let car_entity = commands.spawn((
        WorldAssetRoot(handle.clone()),
        Transform::from_translation(pos),
        Car {
            _car_type: spawn_car_event.car_type.clone(),
        },
        RigidBody::Dynamic,
        avian3d::prelude::ColliderConstructorHierarchy::new(
            avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
        )
        .with_default_layers(CollisionLayers::new(
            [GamePhysicsLayer::Car],
            [GamePhysicsLayer::Map],
        )),
        Restitution::ZERO
            .with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
        Mass(1200.0),
        CollisionLayers::new(
            [GamePhysicsLayer::Car],
            [GamePhysicsLayer::Map],
        ),
        SweptCcd::default(),
        CarDriveState::default(),
    )).id();

    // Wheel dimensions
    let wheel_radius = 0.35f32;
    let wheel_width = 0.25f32;

    // Meshes and materials for the wheels
    let wheel_mesh = meshes.add(Cylinder::new(wheel_radius, wheel_width));
    let wheel_material = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        perceptual_roughness: 0.8,
        ..default()
    });

    let half_width = 0.9f32;
    let half_length = 1.8f32;

    // Y-coordinate attach point is 0.3 relative to car center
    let wheels_offsets = [
        (Vec3::new(-half_width, 0.3, -half_length), true, true),   // FL
        (Vec3::new(half_width, 0.3, -half_length), true, false),  // FR
        (Vec3::new(-half_width, 0.3, half_length), false, true),  // RL
        (Vec3::new(half_width, 0.3, half_length), false, false), // RR
    ];

    for (offset, is_front, is_left) in wheels_offsets {
        let suspension_len = 0.3f32; // base rest length

        // 1. Spawn Strut (pure kinematic connector for vertical motion)
        let strut_entity = commands.spawn((
            Transform::from_translation(pos + offset),
            RigidBody::Dynamic,
            Mass(1.0),
            SleepingDisabled,
        )).id();
        commands.entity(car_entity).add_child(strut_entity);

        // 2. Suspension joint (Chassis <-> Strut)
        // Spring pushing chassis up by pushing strut down (along NEG_Y).
        let susp_joint_entity = commands.spawn((
            PrismaticJoint::new(car_entity, strut_entity)
                .with_local_anchor1(offset)
                .with_local_anchor2(Vec3::ZERO)
                .with_slider_axis(Vec3::NEG_Y)
                .with_limits(0.0, suspension_len)
                .with_motor(
                    LinearMotor::new(MotorModel::SpringDamper {
                        frequency: 4.0,
                        damping_ratio: 0.5,
                    })
                    .with_target_position(suspension_len)
                    .with_max_force(10000.0),
                ),
            SuspensionJoint {
                car_entity,
                is_front,
                is_left,
            },
        )).id();
        commands.entity(car_entity).add_child(susp_joint_entity);

        // 3. For Front wheels, we add a steering knuckle. For Rear wheels, the parent is the Strut.
        let wheel_parent_entity = if is_front {
            // Spawn Knuckle (pure dynamic connector for steering rotation)
            let knuckle_entity = commands.spawn((
                Transform::from_translation(pos + offset),
                RigidBody::Dynamic,
                Mass(1.0),
                SleepingDisabled,
            )).id();
            commands.entity(car_entity).add_child(knuckle_entity);

            // Steering joint (Strut <-> Knuckle)
            let steer_joint_entity = commands.spawn((
                RevoluteJoint::new(strut_entity, knuckle_entity)
                    .with_local_anchor1(Vec3::ZERO)
                    .with_local_anchor2(Vec3::ZERO)
                    .with_hinge_axis(Vec3::Y)
                    .with_angle_limits(-30.0f32.to_radians(), 30.0f32.to_radians())
                    .with_motor(
                        AngularMotor::new(MotorModel::SpringDamper {
                            frequency: 15.0,
                            damping_ratio: 1.0,
                        })
                        .with_target_position(0.0)
                        .with_max_torque(5000.0),
                    ),
                SteeringJoint {
                    car_entity,
                    is_left,
                },
            )).id();
            commands.entity(car_entity).add_child(steer_joint_entity);

            knuckle_entity
        } else {
            strut_entity
        };

        // 4. Spawn Wheel entity (cylinder collider, black visual mesh)
        // Position wheel at the bottom of the suspension (offset - NEG_Y * suspension_len)
        let wheel_pos = pos + offset - Vec3::Y * suspension_len;

        let wheel_entity = commands.spawn((
            Transform::from_translation(wheel_pos),
            RigidBody::Dynamic,
            Mass(120.0),
            CollisionLayers::new(
                [GamePhysicsLayer::Wheel],
                [GamePhysicsLayer::Map],
            ),
            Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
            Friction::new(0.8).with_combine_rule(CoefficientCombine::Max),
            SweptCcd::default(),
            SleepingDisabled,
            Wheel { is_front, is_left },
        ))
        .with_child((
            Mesh3d(wheel_mesh.clone()),
            MeshMaterial3d(wheel_material.clone()),
            avian3d::prelude::Collider::cylinder(wheel_radius, wheel_width),
            // Rotate visual and collider 90 degrees around Z so it rolls along local Z axis
            Transform::from_rotation(Quat::from_rotation_z(90.0f32.to_radians())),
        ))
        .id();
        commands.entity(car_entity).add_child(wheel_entity);

        // 5. Axle/Drive joint (Knuckle/Strut <-> Wheel)
        let axle_joint_entity = commands.spawn((
            RevoluteJoint::new(wheel_parent_entity, wheel_entity)
                .with_local_anchor1(Vec3::ZERO)
                .with_local_anchor2(Vec3::ZERO)
                .with_hinge_axis(Vec3::X)
                .with_motor(
                    AngularMotor::new(MotorModel::AccelerationBased {
                        stiffness: 0.0,
                        damping: 0.3,
                    })
                    .with_target_velocity(0.0)
                    .with_max_torque(500.0),
                ),
            AxleJoint {
                car_entity,
                is_front,
                is_left,
            },
        )).id();
        commands.entity(car_entity).add_child(axle_joint_entity);
    }

    next_state.set(GameControlState::DrivingCar);
}
