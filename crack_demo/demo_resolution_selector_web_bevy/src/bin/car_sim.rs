use avian3d::math::*;
use avian3d::prelude::{
    Collider, CollisionLayers, DistanceJoint, LinearMotor, MassPropertiesBundle, MotorModel,
    PrismaticJoint, Restitution, RigidBody, SleepingDisabled, SubstepCount, Friction,
    Forces, LinearVelocity, WriteRigidBodyForces,
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
        .insert_resource(SubstepCount(50))
        .add_plugins(GameStatesPlugin)
        .add_systems(Startup, setup_scene)
        // .add_systems(Update, spawn_car_first_frame)
        .add_systems(PostStartup, spawn_funny_car)
        .insert_resource(FunnyCarControls::default())
        .add_systems(First, listen_for_wasd_update_controls)
        .add_systems(PreUpdate, apply_physics_for_funny_controls)
        .add_systems(PostUpdate, camera_look_at_car)
        .run();
}

#[derive(Resource, Default, Clone)]
pub struct FunnyCarControls {
    pub accelerate: f32,
    pub steer: f32,
}

fn listen_for_wasd_update_controls(
    mut controls: ResMut<FunnyCarControls>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    let controls2 = controls.clone();
    if keyboard_input.pressed(KeyCode::ArrowUp) || keyboard_input.pressed(KeyCode::KeyW) {
        controls.accelerate = 1.0;
    } else if keyboard_input.pressed(KeyCode::ArrowDown) || keyboard_input.pressed(KeyCode::KeyS) {
        controls.accelerate = -1.0;
    } else {
        controls.accelerate = 0.0;
    }
    if keyboard_input.pressed(KeyCode::ArrowLeft) || keyboard_input.pressed(KeyCode::KeyA) {
        controls.steer = -1.0;
    } else if keyboard_input.pressed(KeyCode::ArrowRight) || keyboard_input.pressed(KeyCode::KeyD) {
        controls.steer = 1.0;
    } else {
        controls.steer = 0.0;
    }
    controls.steer = (controls.steer + controls2.steer) / 2.0;
    controls.accelerate = (controls.accelerate + controls2.accelerate) / 2.0;
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
            Friction::new(0.9),
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
const SUSPENSION_REST: f32 = 0.4;
const SUSPENSION_STIFFNESS: f32 = 12.0;
const SUSPENSION_DAMPING: f32 = 0.8;

const CAR_MASS: f32 = 1200.0;
const WHEEL_MASS: f32 = 25.0;

const CAR_HALF_WIDTH: f32 = 0.9;
const CAR_HALF_LENGTH: f32 = 2.2;
const CAR_HALF_HEIGHT: f32 = 0.6;

const WHEEL_RADIUS: f32 = 0.45;
const WHEEL_WIDTH: f32 = 0.35;


#[derive(Component)]
struct CarBody;

#[derive(Component)]
struct SuspensionJoint;

#[derive(Component)]
struct Wheel {
    is_front: bool,
}

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
            CarBody,
            SleepingDisabled,
        ))
        .id();

    let wheel_offsets_and_steer = [
        // Front (steers normal)
        (
            Vec3::new(-CAR_HALF_WIDTH, -CAR_HALF_HEIGHT, -CAR_HALF_LENGTH),
            true,
        ), // Left
        (
            Vec3::new(CAR_HALF_WIDTH + 0.1, -CAR_HALF_HEIGHT, -CAR_HALF_LENGTH),
            true,
        ), // Right
        // Back
        (
            Vec3::new(-CAR_HALF_WIDTH, -CAR_HALF_HEIGHT, CAR_HALF_LENGTH),
            false,
        ), // Left
        (
            Vec3::new(CAR_HALF_WIDTH, -CAR_HALF_HEIGHT, CAR_HALF_LENGTH),
            false,
        ), // Right
    ];

    for (offset, is_front) in wheel_offsets_and_steer {
        let world_offset = car_rot * offset;
        let wheel_pos = car_pos + world_offset;

        // Wheel: standalone entity, connected to car body.
        let wheel_rot = car_rot * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        let wheel_volume = std::f32::consts::PI * WHEEL_RADIUS * WHEEL_RADIUS * WHEEL_WIDTH;
        let wheel = commands
            .spawn((
                Mesh3d(meshes.add(Cylinder::new(WHEEL_RADIUS, WHEEL_WIDTH))),
                MeshMaterial3d(materials.add(Color::srgb(0.15, 0.15, 0.15))),
                Transform::from_translation(wheel_pos).with_rotation(wheel_rot),
                RigidBody::Dynamic,
                MassPropertiesBundle::from_shape(
                    &Cylinder::new(WHEEL_RADIUS, WHEEL_WIDTH),
                    WHEEL_MASS / wheel_volume,
                ),
                Collider::cylinder(WHEEL_RADIUS, WHEEL_WIDTH),
                CollisionLayers::new([GamePhysicsLayer::Wheel], [GamePhysicsLayer::Map]),
                Friction::new(0.05).with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
                SleepingDisabled,
                Wheel { is_front },
            ))
            .id();

        commands.spawn((
            PrismaticJoint::new(car_body, wheel)
                .with_local_anchor1(Vector::new(offset.x, offset.y, offset.z))
                .with_slider_axis(Vector::NEG_Y) // slides downward relative to body
                .with_local_basis2(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2))
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
            DistanceJoint::new(car_body, wheel)
                .with_local_anchor1(Vector::new(offset.x, offset.y, offset.z))
                .with_limits(SUSPENSION_MIN, SUSPENSION_MAX),
        );
    }
}

fn apply_physics_for_funny_controls(
    controls: Res<FunnyCarControls>,
    car_query: Query<(&Transform, &LinearVelocity), With<CarBody>>,
    mut wheels_query: Query<(Entity, &Wheel, &Transform, &mut Friction)>,
    mut forces: Query<Forces, Without<CarBody>>,
    mut gizmos: Gizmos,
) {
    let Ok((car_transform, car_velocity)) = car_query.single() else {
        return;
    };

    let speed = car_velocity.length();
    let max_steer = 1.2 / (1.0 + 0.3 * speed);
    let steer_angle = controls.steer * max_steer;

    let steer_dir_world = car_transform.rotation * Vec3::new(steer_angle.sin(), 0.0, -steer_angle.cos());

    // Friction control
    let target_friction = if controls.accelerate < 0.0 { 0.9 } else { 0.05 };
    for (_, _, _, mut friction) in &mut wheels_query {
        friction.dynamic_coefficient = target_friction;
        friction.static_coefficient = target_friction;
    }

    // Force control
    let total_mass = CAR_MASS + 4.0 * WHEEL_MASS;

    // Lateral friction to prevent sliding/spinning
    let steer_side_world = Vec3::new(-steer_dir_world.z, 0.0, steer_dir_world.x).normalize_or_zero();
    let slide_speed = car_velocity.dot(steer_side_world);
    let total_lateral_force = -steer_side_world * (slide_speed * total_mass * 5.0);
    let lateral_force_per_wheel = total_lateral_force / 4.0;

    // Acceleration control / forward drive force
    let mut drive_force_per_wheel = Vec3::ZERO;
    if controls.accelerate > 0.0 {
        let target_speed = 120.0f32 / 3.6f32; // ~33.33 m/s
        let current_speed = car_velocity.dot(steer_dir_world);
        let acc = ((target_speed - current_speed) / 4.0f32).max(0.0f32);
        drive_force_per_wheel = steer_dir_world * (total_mass * acc / 2.0f32);
    }

    for (wheel_entity, wheel, _, _) in &wheels_query {
        if let Ok(mut wheel_forces) = forces.get_mut(wheel_entity) {
            let mut wheel_force = lateral_force_per_wheel;
            if wheel.is_front {
                wheel_force += drive_force_per_wheel;
            }
            wheel_forces.apply_force(wheel_force);
        }
    }

    // Steering visualization
    for (_, wheel, wheel_transform, _) in &wheels_query {
        if wheel.is_front {
            let start = wheel_transform.translation;
            let end = start + steer_dir_world * 1.5;
            gizmos.line(start, end, Color::srgb(0.0, 1.0, 0.0));
        }
    }
}

fn camera_look_at_car(
    mut camera_query: Query<&mut Transform, (With<Camera3d>, Without<CarBody>)>,
    car_query: Query<&Transform, (With<CarBody>, Without<Camera3d>)>,
) {
    let Ok(car_transform) = car_query.single() else {
        return;
    };
    for mut camera_transform in &mut camera_query {
        camera_transform.look_at(car_transform.translation, Vec3::Y);
    }
}
