use crate::plugins::{
    cars_driving::{
        car_info::{get_car_asset, get_random_car_type},
        driving_plugin::{
            AxleJoint, CarDriveState, GamePhysicsLayer, SuspensionJoint, Wheel, Strut,
        },
    },
    states::GameControlState,
};
use avian3d::prelude::{
    AngularMotor, CoefficientCombine, CollisionLayers, Friction, LinearMotor, Mass,
    MotorModel, PrismaticJoint, Restitution, RevoluteJoint, RigidBody, SleepingDisabled, SweptCcd,
};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

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
    pub physics_children: Vec<Entity>,
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

    if let Some(hit) = spatial_query.cast_ray(
        ray_origin,
        bevy::prelude::Dir3::NEG_Y,
        1000.0,
        true,
        &filter,
    ) {
        let ground_y = start_y - hit.distance;
        pos.y = ground_y + 9.0;
    } else {
        pos.y += 9.0;
    }

    let handle = get_car_asset(&spawn_car_event.car_type, &asset_server);

    let car_entity = commands
        .spawn((
            
            WorldAssetRoot(handle.clone()),
            Transform::from_translation(pos),
            RigidBody::Dynamic,
            avian3d::prelude::ColliderConstructorHierarchy::new(
                avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
            )
            .with_default_layers(CollisionLayers::new(
                [GamePhysicsLayer::Car],
                [GamePhysicsLayer::Map],
            )),
            Restitution::ZERO.with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
            Mass(1200.0),
            CollisionLayers::new([GamePhysicsLayer::Car], [GamePhysicsLayer::Map]),
            SweptCcd::default(),
            CarDriveState {
                spawn_position: Some(pos),
                ..default()
            },
        ))
        .id();

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
        (Vec3::new(-half_width, 0.3, -half_length), true, true), // FL
        (Vec3::new(half_width, 0.3, -half_length), true, false), // FR
        (Vec3::new(-half_width, 0.3, half_length), false, true), // RL
        (Vec3::new(half_width, 0.3, half_length), false, false), // RR
    ];

    let mut physics_children = Vec::new();

    for (offset, is_front, is_left) in wheels_offsets {
        let suspension_len = 0.3f32; // base rest length

        // 1. Spawn Strut (lightweight invisible intermediary — decouples wheel angular freedom from chassis)
        // Spawned as root-level entity using its initial global position
        let strut_pos = pos + offset - Vec3::Y * suspension_len;
        let strut_entity = commands
            .spawn((
                Transform::from_translation(strut_pos),
                RigidBody::Dynamic,
                Mass(1.0),
                SleepingDisabled,
                Strut { is_front, is_left },
            ))
            .id();
        physics_children.push(strut_entity);

        // 2. Suspension joint (Chassis <-> Strut)
        // PrismaticJoint constrains strut to slide vertically relative to chassis.
        // Spawned as root-level joint entity.
        let susp_joint_entity = commands
            .spawn((
                PrismaticJoint::new(car_entity, strut_entity)
                    .with_local_anchor1(offset)
                    .with_local_anchor2(Vec3::ZERO)
                    .with_slider_axis(Vec3::NEG_Y)
                    .with_limits(0.0, suspension_len)
                    .with_motor(
                        LinearMotor::new(MotorModel::SpringDamper {
                            frequency: 6.0,
                            damping_ratio: 0.6,
                        })
                        .with_target_position(suspension_len)
                        .with_max_force(200000.0),
                    ),
                Mass(1.0),
                SuspensionJoint {
                    car_entity,
                    is_front,
                    is_left,
                },
            ))
            .id();
        physics_children.push(susp_joint_entity);

        // 3. Spawn Wheel entity (dynamic rigid body with collider on child)
        // Spawned as root-level entity using its initial global position
        let wheel_entity = commands
            .spawn((
                Transform::from_translation(strut_pos),
                RigidBody::Dynamic,
                Mass(120.0),
                SleepingDisabled,
                Wheel { is_front, is_left },
            ))
            // Child has the collider + visual mesh + physics material params
            .with_child((
                Mesh3d(wheel_mesh.clone()),
                MeshMaterial3d(wheel_material.clone()),
                avian3d::prelude::Collider::cylinder(wheel_radius, wheel_width),
                CollisionLayers::new([GamePhysicsLayer::Wheel], [GamePhysicsLayer::Map]),
                Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
                Friction::new(0.8).with_combine_rule(CoefficientCombine::Max),
                Mass(120.0),
                SweptCcd::default(),
                // Rotate 90 degrees around Z so cylinder rolls along local X axis
                Transform::from_rotation(Quat::from_rotation_z(90.0f32.to_radians())),
            ))
            .id();
        physics_children.push(wheel_entity);

        // 4. Axle/Drive joint (Strut <-> Wheel) for spinning
        // RevoluteJoint allows the wheel to spin on its axle relative to the strut.
        // Spawned as root-level joint entity.
        let axle_joint_entity = commands
            .spawn((
                RevoluteJoint::new(strut_entity, wheel_entity)
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
                Mass(1.0),
                AxleJoint {
                    car_entity,
                    is_front,
                    is_left,
                },
            ))
            .id();
        physics_children.push(axle_joint_entity);
    }

    commands.entity(car_entity).insert(Car {
        _car_type: spawn_car_event.car_type.clone(),
        physics_children,
    });

    next_state.set(GameControlState::DrivingCar);
}
