use avian3d::math::*;
use avian3d::prelude::{
    AngularMotor, Collider, CollisionLayers, DistanceJoint, FixedJoint, JointBasis, LinearMotor,
    MassPropertiesBundle, MotorModel, PrismaticJoint, Restitution, RevoluteJoint, RigidBody,
    SleepingDisabled,
};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        settings::{Backends, WgpuSettings},
    },
    window::WindowResolution,
};
use demo_resolution_selector_web_bevy::{
    plugins::{
        cars_driving::driving_plugin::GamePhysicsLayer, physics_plugin::PhysicsPlugin,
        states::GameStatesPlugin,
    },
    ui_egui::UiState,
};

fn main() {
    #[cfg(feature = "web")]
    let backends = Backends::GL;
    #[cfg(not(feature = "web"))]
    let backends = Backends::PRIMARY;

    App::new()
        .add_plugins(
            DefaultPlugins
                .build()
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Car Sim - Native".into(),
                        resolution: WindowResolution::new(1280, 720),
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
                }),
        )
        .add_plugins(bevy_egui::EguiPlugin::default())
        .insert_resource(UiState::with_physics_debug()) // Satisfies PhysicsPlugin's sync_physics_debug_config
        .add_plugins(PhysicsPlugin)
        .add_plugins(GameStatesPlugin)
        .add_systems(Startup, setup_scene)
        // .add_systems(Update, spawn_car_first_frame)
        .add_systems(PostStartup, spawn_funny_car)
        .insert_resource(FunnyCarControls::default())
        .add_systems(First, listen_for_wasd_update_controls)
        .add_systems(PreUpdate, apply_physics_for_funny_controls)
        .run();
}

#[derive(Resource, Default)]
pub struct FunnyCarControls {
    pub accelerate: f32,
    pub steer: f32,
}

fn listen_for_wasd_update_controls(
    mut controls: ResMut<FunnyCarControls>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    if keyboard_input.pressed(KeyCode::ArrowUp) {
        controls.accelerate = 1.0;
    } else if keyboard_input.pressed(KeyCode::ArrowDown) {
        controls.accelerate = -1.0;
    } else {
        controls.accelerate = 0.0;
    }
    if keyboard_input.pressed(KeyCode::ArrowLeft) {
        controls.steer = -1.0;
    } else if keyboard_input.pressed(KeyCode::ArrowRight) {
        controls.steer = 1.0;
    } else {
        controls.steer = 0.0;
    }
}

fn create_grayscale_texture(gray1: u8, gray2: u8) -> Image {
    let mut texture_data = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let color = if (x / 4 + y / 4) % 2 == 0 {
                gray1
            } else {
                gray2
            };
            let offset = (y * 32 + x) * 4;
            texture_data[offset] = color;
            texture_data[offset + 1] = color;
            texture_data[offset + 2] = color;
            texture_data[offset + 3] = 255;
        }
    }
    let mut image = Image::new_fill(
        Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &texture_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: bevy::image::ImageAddressMode::Repeat,
        address_mode_v: bevy::image::ImageAddressMode::Repeat,
        ..default()
    });
    image
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 1. Spawning 4 ground cubes of size 500x500x500
    let cubes_info = [
        (Vec3::new(250.0, -250.0, 250.0), (50, 70)),
        (Vec3::new(-250.0, -250.0, 250.0), (90, 110)),
        (Vec3::new(250.0, -250.0, -250.0), (130, 150)),
        (Vec3::new(-250.0, -250.0, -250.0), (170, 190)),
    ];

    for (center, (gray1, gray2)) in cubes_info {
        let tile_repeat: f32 = 1.0 + rand::random::<f32>() * 2.0; // around 1 to 3 meters

        let mut mesh = Mesh::from(Cuboid::from_size(Vec3::new(500.0, 500.0, 500.0)));
        let repeat = 500.0 / tile_repeat;
        if let Some(bevy::render::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0)
        {
            for uv in uvs.iter_mut() {
                uv[0] *= repeat;
                uv[1] *= repeat;
            }
        }
        let mesh_handle = meshes.add(mesh);

        let texture = create_grayscale_texture(gray1, gray2);
        let texture_handle = images.add(texture);

        let material_handle = materials.add(StandardMaterial {
            base_color_texture: Some(texture_handle),
            perceptual_roughness: 0.9,
            ..default()
        });

        commands.spawn((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            Transform::from_translation(center),
            RigidBody::Static,
            Collider::cuboid(500.0, 500.0, 500.0),
            Restitution::ZERO.with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
            CollisionLayers::new(
                [GamePhysicsLayer::Map],
                [
                    GamePhysicsLayer::Map,
                    GamePhysicsLayer::Car,
                    GamePhysicsLayer::Wheel,
                ],
            ),
        ));
    }

    // 2. Spawn camera with AmbientLight component
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-4.0, 3.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
        AmbientLight {
            color: Color::srgb(0.8, 0.85, 1.0),
            brightness: 1000.0,
            ..default()
        },
    ));

    // 3. Spawn DirectionalLight (the sun)
    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(200.0, 400.0, 200.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

// fn spawn_car_first_frame(mut commands: Commands, mut run_once: Local<bool>) {
//     if !*run_once {
//         *run_once = true;
//         info!("Triggering SpawnCarRequestEvent at 0,0,0 with random car type");
//         commands.trigger(SpawnCarRequestEvent {
//             position: Vec3::ZERO,
//             car_type: get_random_car_type().to_string(),
//         });
//     }
// }

const SUSPENSION_MIN: f32 = 0.1;
const SUSPENSION_MAX: f32 = 0.5;
const SUSPENSION_REST: f32 = 0.3;
const SUSPENSION_STIFFNESS: f32 = 400.0;
const SUSPENSION_DAMPING: f32 = 2.0;

const CAR_MASS: f32 = 700.0;
const WHEEL_MASS: f32 = 150.0;
const HUB_MASS: f32 = 10.0;

const CAR_HALF_WIDTH: f32 = 0.9;
const CAR_HALF_LENGTH: f32 = 2.2;
const CAR_HALF_HEIGHT: f32 = 0.6;

const WHEEL_RADIUS: f32 = 0.35;
const WHEEL_WIDTH: f32 = 0.25;

const MAX_STEER_ANGLE: f32 = 0.5;
const MAX_WHEEL_SPEED: f32 = 50.0;
const MAX_WHEEL_TORQUE: f32 = 500000.0;

const WHEEL_MOTOR_STIFFNESS: f32 = 0.0;
const WHEEL_MOTOR_DAMPING: f32 = 1.0;

#[derive(Component)]
struct SuspensionJoint;

#[derive(Component)]
struct FrontSteering;

#[derive(Component)]
struct RearSteering;

#[derive(Component)]
struct NoSteering;

#[derive(Component)]
struct WheelMotorJoint;

fn spawn_funny_car(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let random_angle = rand::random::<f32>() * std::f32::consts::TAU;
    let car_rot = Quat::from_rotation_y(random_angle);
    let car_pos = Vec3::new(0.0, 4.0, 0.0);

    let car_body_mass = CAR_MASS;
    let car_body_volume =
        (CAR_HALF_WIDTH * 2.0) * (CAR_HALF_HEIGHT * 2.0) * (CAR_HALF_LENGTH * 2.0);

    let car_body = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(
                CAR_HALF_WIDTH * 2.0,
                CAR_HALF_HEIGHT * 2.0,
                CAR_HALF_LENGTH * 2.0,
            ))),
            MeshMaterial3d(materials.add(Color::srgb(0.2, 0.3, 0.8))),
            Transform::from_translation(car_pos).with_rotation(car_rot),
            RigidBody::Dynamic,
            MassPropertiesBundle::from_shape(
                &Cuboid::new(
                    CAR_HALF_WIDTH * 2.0,
                    CAR_HALF_HEIGHT * 2.0,
                    CAR_HALF_LENGTH * 2.0,
                ),
                car_body_mass / car_body_volume,
            ),
            avian3d::prelude::Collider::cuboid(
                CAR_HALF_WIDTH * 2.0,
                CAR_HALF_HEIGHT * 2.0,
                CAR_HALF_LENGTH * 2.0,
            ),
            CollisionLayers::new(
                [GamePhysicsLayer::Car],
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
            SleepingDisabled,
        ))
        .id();

    let wheel_offsets_and_steer = [
        // Front (steers normal)
        (
            Vec3::new(-CAR_HALF_WIDTH, -CAR_HALF_HEIGHT, CAR_HALF_LENGTH),
            true,
            false,
        ), // Left
        (
            Vec3::new(CAR_HALF_WIDTH + 0.1, -CAR_HALF_HEIGHT, CAR_HALF_LENGTH),
            true,
            false,
        ), // Right
        // Middle (no steer)
        (
            Vec3::new(-CAR_HALF_WIDTH - 0.1, -CAR_HALF_HEIGHT, 0.0),
            false,
            false,
        ), // Left
        (
            Vec3::new(CAR_HALF_WIDTH, -CAR_HALF_HEIGHT, 0.0),
            false,
            false,
        ), // Right
        // Back (steers inverted)
        (
            Vec3::new(-CAR_HALF_WIDTH, -CAR_HALF_HEIGHT, -(CAR_HALF_LENGTH)),
            false,
            true,
        ), // Left
        (
            Vec3::new(CAR_HALF_WIDTH, -CAR_HALF_HEIGHT, -(CAR_HALF_LENGTH)),
            false,
            true,
        ), // Right
    ];

    for (offset, is_front, is_rear) in wheel_offsets_and_steer {
        let world_offset = car_rot * offset;
        let hub_pos = car_pos + world_offset;

        let suspension_hub = commands
            .spawn((
                Transform::from_translation(hub_pos).with_rotation(car_rot),
                RigidBody::Dynamic,
                MassPropertiesBundle::from_shape(&Cuboid::from_length(0.1), HUB_MASS / 0.001),
                SleepingDisabled,
            ))
            .id();

        commands.spawn((
            PrismaticJoint::new(car_body, suspension_hub)
                .with_local_anchor1(Vector::new(offset.x, offset.y, offset.z))
                .with_slider_axis(Vector::NEG_Y) // slides downward relative to body
                .with_limits(SUSPENSION_MIN, SUSPENSION_MAX)
                .with_motor(
                    LinearMotor::new(MotorModel::SpringDamper {
                        frequency: SUSPENSION_STIFFNESS,
                        damping_ratio: SUSPENSION_DAMPING,
                    })
                    .with_target_position(SUSPENSION_REST)
                    .with_max_force(Scalar::MAX),
                ),
                
            SuspensionJoint,
        ));

        commands.spawn(
            DistanceJoint::new(car_body, suspension_hub)
                .with_local_anchor1(Vector::new(offset.x, offset.y, offset.z))
                .with_limits(SUSPENSION_MIN, SUSPENSION_MAX)
        );


        // Steering hub: same Z rotation as wheel so revolute joint axes align.
        let wheel_rot = car_rot * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let steering_hub = commands
            .spawn((
                Transform::from_translation(hub_pos).with_rotation(wheel_rot),
                RigidBody::Dynamic,
                MassPropertiesBundle::from_shape(&Cuboid::from_length(0.1), HUB_MASS / 0.001),
                SleepingDisabled,
            ))
            .id();

        // FixedJoint for steering: we change frame2.basis at runtime to steer.
        // The initial basis on body2 accounts for the Z rotation offset between
        // suspension_hub (car_rot) and steering_hub (car_rot * rot_z(PI/2)).
        let initial_steer_basis = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let mut steering_joint = commands.spawn(
            FixedJoint::new(suspension_hub, steering_hub)
                .with_local_basis2(initial_steer_basis),
        );

        if is_front {
            steering_joint.insert(FrontSteering);
        } else if is_rear {
            steering_joint.insert(RearSteering);
        } else {
            steering_joint.insert(NoSteering);
        }

        // Wheel: standalone entity, no parenting. Same rotation as steering_hub.
        let wheel_volume = std::f32::consts::PI * WHEEL_RADIUS * WHEEL_RADIUS * WHEEL_WIDTH;
        let wheel = commands
            .spawn((
                Mesh3d(meshes.add(Cylinder::new(WHEEL_RADIUS, WHEEL_WIDTH))),
                MeshMaterial3d(materials.add(Color::srgb(0.15, 0.15, 0.15))),
                Transform::from_translation(hub_pos).with_rotation(wheel_rot),
                RigidBody::Dynamic,
                MassPropertiesBundle::from_shape(
                    &Cylinder::new(WHEEL_RADIUS, WHEEL_WIDTH),
                    WHEEL_MASS / wheel_volume,
                ),
                Collider::cylinder(WHEEL_RADIUS, WHEEL_WIDTH),
                CollisionLayers::new([GamePhysicsLayer::Wheel], [GamePhysicsLayer::Map]),
                SleepingDisabled,
            ))
            .id();

        // Wheel spin: revolute joint around Y (cylinder's natural axis).
        // Both steering_hub and wheel share the same rotation, so Y aligns.
        commands.spawn((
            RevoluteJoint::new(steering_hub, wheel)
                .with_hinge_axis(Vector::Y)
                .with_motor(AngularMotor {
                    target_velocity: 0.0,
                    max_torque: MAX_WHEEL_TORQUE,
                    motor_model: MotorModel::AccelerationBased {
                        stiffness: WHEEL_MOTOR_STIFFNESS,
                        damping: WHEEL_MOTOR_DAMPING,
                    },
                    ..default()
                }),
            WheelMotorJoint,
        ));
    }
}

fn apply_physics_for_funny_controls(
    controls: Res<FunnyCarControls>,
    mut front_steer: Query<
        &mut FixedJoint,
        (
            With<FrontSteering>,
            Without<RearSteering>,
            Without<NoSteering>,
        ),
    >,
    mut rear_steer: Query<
        &mut FixedJoint,
        (
            With<RearSteering>,
            Without<FrontSteering>,
            Without<NoSteering>,
        ),
    >,
    mut no_steer: Query<
        &mut FixedJoint,
        (
            With<NoSteering>,
            Without<FrontSteering>,
            Without<RearSteering>,
        ),
    >,
    mut wheels: Query<&mut RevoluteJoint, With<WheelMotorJoint>>,
) {
    // The base Z rotation that aligns suspension_hub frame with the steering_hub frame.
    let base_rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);

    // Front wheels: steer normally
    for mut joint in &mut front_steer {
        let steer_rot = Quat::from_rotation_y(controls.steer * MAX_STEER_ANGLE);
        joint.frame2.basis = JointBasis::Local(base_rot * steer_rot);
    }
    // Rear wheels: steer inverted (forklift)
    for mut joint in &mut rear_steer {
        let steer_rot = Quat::from_rotation_y(-controls.steer * MAX_STEER_ANGLE);
        joint.frame2.basis = JointBasis::Local(base_rot * steer_rot);
    }
    // Middle wheels: no steer, keep base rotation
    for mut joint in &mut no_steer {
        joint.frame2.basis = JointBasis::Local(base_rot);
    }
    // All wheels: set drive speed (AWD)
    for mut joint in &mut wheels {
        joint.motor.target_velocity = controls.accelerate * MAX_WHEEL_SPEED;
    }
}
