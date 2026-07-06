use avian3d::prelude::{
    CoefficientCombine, Collider, CollisionEventsEnabled, CollisionLayers, LinearVelocity,
    MassPropertiesBundle, Restitution, RigidBody,
};
use bevy::prelude::*;
use demo_resolution_selector_web_bevy::plugins::cars_driving::car_info::get_car_asset;
use demo_resolution_selector_web_bevy::plugins::cars_driving::driving_plugin::GamePhysicsLayer;
use demo_resolution_selector_web_bevy::plugins::cars_driving::driving_plugin::{
    CarDriveState, CarWheelsContactData, SimState,
};
use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app, plugins::cars_driving::driving_plugin::spawn_car::Car,
    utils::setup_debug_scene::SetupDebugScenePlugin,
};
use demo_resolution_selector_web_bevy::{
    plugins::{
        cars_driving::CarsAndDrivingPlugin, cars_driving::car_info::get_random_car_type,
        cars_driving::driving_plugin::spawn_car::SpawnCarRequestEvent,
        physics_plugin::PhysicsPlugin, states::GameStatesPlugin,
    },
    ui_egui::UiState,
};

const CAR_SIZE: Vec3 = Vec3::new(1.8, 1.0, 3.04);
const CAR_MASS: f32 = 1200.0;

#[derive(Resource)]
struct SimLogTimer {
    total_time: f32,
    last_log_time: f32,
}

impl Default for SimLogTimer {
    fn default() -> Self {
        Self {
            total_time: 0.0,
            last_log_time: 0.0,
        }
    }
}

fn main() {
    make_basic_app("Car Sim")
        .add_plugins(bevy_egui::EguiPlugin::default())
        .insert_resource(UiState::with_physics_debug()) // Satisfies PhysicsPlugin's sync_physics_debug_config
        .insert_resource(SimLogTimer::default())
        .insert_resource(SimState {
            is_sim: true,
            ..default()
        })
        .add_plugins(PhysicsPlugin)
        // .insert_resource(SubstepCount(50))
        .add_plugins(GameStatesPlugin)
        .add_plugins(CarsAndDrivingPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_systems(
            Startup,
            (
                spawn_bumpy_heightmap,
                spawn_physics_cubes,
                spawn_random_cars,
            ),
        )
        .add_systems(
            Update,
            (update_sim_control, log_car_state, set_car_initial_speed),
        )
        .run();
}

/// A bumpy heightmap laid over the flat debug ground: heights in [-0.25, 0.25] so the
/// bumps and the original ground cubes (tops at y = 0) intertwine — peaks stick out,
/// valleys sink below the flat floor. Rendered and collidable (trimesh from the same
/// vertices, so visuals == physics).
fn spawn_bumpy_heightmap(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    const AMPLITUDE: f32 = 0.4;
    let n = 151usize; // vertices per side
    let size = 300.0f32; // meters, centered at origin
    let step = size / (n - 1) as f32;
    let half = size / 2.0;

    // ~7m wavelength, amplitude 0.4m
    let hfun = |x: f32, z: f32| -> f32 { AMPLITUDE * (x * 0.9).sin() * (z * 0.9).sin() };

    let mut positions = Vec::with_capacity(n * n);
    let mut uvs = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n {
            let x = -half + i as f32 * step;
            let z = -half + j as f32 * step;
            positions.push([x, hfun(x, z), z]);
            uvs.push([i as f32 * 0.5, j as f32 * 0.5]);
        }
    }
    let mut indices: Vec<u32> = Vec::with_capacity((n - 1) * (n - 1) * 6);
    for i in 0..n - 1 {
        for j in 0..n - 1 {
            let a = (i * n + j) as u32;
            let b = (i * n + j + 1) as u32;
            let c = ((i + 1) * n + j) as u32;
            let d = ((i + 1) * n + j + 1) as u32;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    let mut mesh = Mesh::new(
        bevy::render::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::render::mesh::Indices::U32(indices));
    mesh.compute_normals();

    let collider = Collider::trimesh_from_mesh(&mesh)
        .expect("bumpy heightmap mesh should convert to a trimesh collider");

    commands.spawn((
        Name::new("BumpyHeightmap"),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.52, 0.4),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::IDENTITY,
        RigidBody::Static,
        collider,
        Restitution::ZERO.with_combine_rule(CoefficientCombine::Min),
        CollisionLayers::new(
            [GamePhysicsLayer::Map],
            [GamePhysicsLayer::Car, GamePhysicsLayer::Wheel],
        ),
    ));
}

fn set_car_initial_speed(mut q_new_car: Query<(&Transform, &mut LinearVelocity), Added<Car>>) {
    for (transform, mut lin_vel) in q_new_car.iter_mut() {
        let fwd = transform.rotation * Vec3::NEG_Z;
        lin_vel.0 = fwd * (100.0 / 3.6);
        info!(
            "Set initial car spawn speed to 100 km/h ({:.2} m/s)",
            lin_vel.0.length()
        );
    }
}

fn update_sim_control(
    time: Res<Time>,
    mut sim_state: ResMut<SimState>,
    mut commands: Commands,
    mut q_car: Query<&mut CarDriveState, With<Car>>,
) {
    let dt = time.delta_secs();
    sim_state.time_elapsed += dt;

    // 1. Wait 1s to spawn a car 100m outside the corner of the bumpy grid (150, 150)
    if !sim_state.spawned && sim_state.time_elapsed >= 1.0 {
        sim_state.spawned = true;
        let car_type = get_random_car_type();

        // 100m outside the corner (150, 150) along diagonal
        let corner_dist = 100.0 / 2.0f32.sqrt(); // ~70.71m in X and Z
        let spawn_pos = Vec3::new(150.0 + corner_dist, 0.0, 150.0 + corner_dist);

        // Base orientation facing toward the bumpy grid corner
        let base_dir = Vec3::new(-1.0, 0.0, -1.0).normalize();
        let base_rot = Quat::from_rotation_arc(Vec3::NEG_Z, base_dir);

        // Rotate random 0..10 degrees to the right around Y axis
        let rand_deg_right = rand::random::<f32>() * 10.0f32.to_radians();
        let car_rot = base_rot * Quat::from_rotation_y(-rand_deg_right);

        info!(
            "Spawn timer met: Triggering SpawnCarRequestEvent at ({:.2}, 0, {:.2}) with right-rotation {:.1} deg",
            spawn_pos.x,
            spawn_pos.z,
            rand_deg_right.to_degrees()
        );

        commands.trigger(SpawnCarRequestEvent {
            position: spawn_pos,
            car_type: car_type.to_string(),
            rotation: Some(car_rot),
        });
    }

    // 2. Set acceleration for 5s (from t = 1.0s to t = 6.0s), then hand the controls
    //    back to the player exactly once — keeping writing avg_accelerate every frame
    //    was stomping WASD input after the rigged phase.
    if sim_state.spawned {
        if sim_state.time_elapsed < 6.0 {
            if let Some(mut drive_state) = q_car.iter_mut().next() {
                drive_state.avg_accelerate = 1.0;
            }
            sim_state.is_sim = true;
        } else if sim_state.is_sim {
            if let Some(mut drive_state) = q_car.iter_mut().next() {
                drive_state.avg_accelerate = 0.0;
            }
            sim_state.is_sim = false;
        }
    }
}

fn log_car_state(
    time: Res<Time>,
    mut log_timer: ResMut<SimLogTimer>,
    q_car: Query<
        (
            &Transform,
            &LinearVelocity,
            &CarDriveState,
            &CarWheelsContactData,
        ),
        With<Car>,
    >,
) {
    let dt = time.delta_secs();
    log_timer.total_time += dt;

    if log_timer.total_time > 30.0 {
        return;
    }

    if log_timer.total_time - log_timer.last_log_time >= 0.25 {
        log_timer.last_log_time = log_timer.total_time;
        if let Some((transform, velocity, drive_state, contact_data)) = q_car.iter().next() {
            let pos = transform.translation;
            let speed = velocity.0.length();
            let (yaw, pitch, roll) = transform.rotation.to_euler(EulerRot::YXZ);
            let acc = drive_state.avg_accelerate;
            let brake = drive_state.avg_brake;
            let steer = drive_state.avg_steer;

            let max_len = drive_state.traction_loss_threshold;
            let mut susp_lengths = [0.0f32; 4];
            for wheel_idx in 0..4 {
                let w_contact = &contact_data.wheels[wheel_idx];
                let mut sum_dist = 0.0f32;
                let mut engaged_rays = 0;
                for &d in &w_contact.ray_distances {
                    if d <= max_len {
                        sum_dist += d;
                        engaged_rays += 1;
                    }
                }
                let avg_length = if engaged_rays > 0 {
                    sum_dist / engaged_rays as f32
                } else {
                    max_len
                };
                susp_lengths[wheel_idx] = avg_length;
            }

            info!(
                "TIME: {:.2}s | POS: ({:.2}, {:.2}, {:.2}) | SPEED: {:.2} m/s | ROT: (Y:{:.1} P:{:.1} R:{:.1}) | CTL: (A:{:.1} B:{:.1} S:{:.1}) | Y0: {:.2} | SUSP: [FL: {:.2}m, FR: {:.2}m, RL: {:.2}m, RR: {:.2}m]",
                log_timer.total_time,
                pos.x,
                pos.y,
                pos.z,
                speed,
                yaw.to_degrees(),
                pitch.to_degrees(),
                roll.to_degrees(),
                acc,
                brake,
                steer,
                drive_state.ray_start_y_offset,
                susp_lengths[0],
                susp_lengths[1],
                susp_lengths[2],
                susp_lengths[3]
            );
        }
    }
}

/// Scatter non-drivable prop cars over the demo area and along the sim path.
fn spawn_random_cars(mut commands: Commands, asset_server: Res<AssetServer>) {
    let volume = CAR_SIZE.x * CAR_SIZE.y * CAR_SIZE.z;
    let density = CAR_MASS / volume;

    // Spawn prop cars near origin and directly on the car_sim path (~x: 200, z: 195)
    let positions = [
        Vec3::new(0.0, 3.0, 0.0),
        Vec3::new(10.0, 3.0, -10.0),
        Vec3::new(-10.0, 3.0, 10.0),
        Vec3::new(205.0, 3.0, 200.0), // On sim trajectory!
        Vec3::new(185.0, 3.0, 175.0), // On sim trajectory!
    ];

    for (i, pos) in positions.iter().enumerate() {
        let rot = Quat::from_rotation_y((i as f32) * 1.2);
        let car_asset = get_car_asset(get_random_car_type(), &asset_server);

        commands.spawn((
            Name::new(format!("PropCar_{}", i)),
            Car {
                _car_type: "prop".to_string(),
            },
            Transform::from_translation(*pos).with_rotation(rot),
            RigidBody::Dynamic,
            Collider::cuboid(CAR_SIZE.x, CAR_SIZE.y, CAR_SIZE.z),
            MassPropertiesBundle::from_shape(
                &Cuboid::new(CAR_SIZE.x, CAR_SIZE.y, CAR_SIZE.z),
                density,
            ),
            bevy::world_serialization::WorldAssetRoot(car_asset),
            CollisionLayers::new(
                [GamePhysicsLayer::Car],
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
            CollisionEventsEnabled,
            Visibility::default(),
            InheritedVisibility::default(),
        ));
    }
}

/// Dynamic cubes along the sim path to collide with.
fn spawn_physics_cubes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(1.5, 1.5, 1.5));
    let material = materials.add(Color::srgb_u8(124, 144, 255));

    let positions = [
        Vec3::new(0.0, 4.0, 5.0),
        Vec3::new(-5.0, 4.0, 0.0),
        Vec3::new(212.0, 4.0, 208.0), // On sim trajectory!
        Vec3::new(195.0, 4.0, 188.0), // On sim trajectory!
    ];

    for (i, pos) in positions.iter().enumerate() {
        commands.spawn((
            Name::new(format!("PhysicsCube_{}", i)),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(*pos),
            RigidBody::Dynamic,
            Collider::cuboid(1.5, 1.5, 1.5),
            CollisionLayers::new(
                GamePhysicsLayer::Car,
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
            CollisionEventsEnabled,
        ));
    }
}
