use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, egui};
use demo_resolution_selector_web_bevy::basic_app::make_basic_app;
use demo_resolution_selector_web_bevy::utils::setup_debug_scene::SetupDebugScenePlugin;

fn main() {
    make_basic_app("Fane")
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(EguiPlugin::default())
        .init_resource::<GratarGame>()
        .init_resource::<CameraDirector>()
        .add_systems(Startup, setup_gratar)
        .add_systems(
            Update,
            (
                gratar_hydraulics_system,
                smoke_particles_system,
                rhythm_game_system,
                rhythm_ui_system,
                cinematic_camera_system,
            ),
        )
        .run();
}

/// A component representing the gratar bouncing on hydraulics
#[derive(Component)]
struct HydraulicGratar {
    // Current offset from rest position (Y=1.2, X/Z=0.0)
    translation: Vec3,
    rotation_euler: Vec3, // pitch (X), yaw (Y), roll (Z)

    // Velocities
    velocity: Vec3,
    angular_velocity: Vec3,
}

impl Default for HydraulicGratar {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation_euler: Vec3::ZERO,
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
        }
    }
}

/// Smoke particles rising from the hot coals
#[derive(Component)]
struct SmokeParticle {
    velocity: Vec3,
    lifetime: f32,
    max_lifetime: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum NoteDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Debug)]
struct RhythmNote {
    direction: NoteDirection,
    time: f32, // Target time in seconds
    hit: bool,
}

#[derive(Resource)]
struct GratarGame {
    is_started: bool,
    is_finished: bool,
    needs_music_start: bool,
    song_time: f32,
    notes: Vec<RhythmNote>,
    score: u32,
    combo: u32,
    max_combo: u32,
    multiplier: u32,
    last_rating: String,
    last_rating_timer: f32,
    bpm: f32,
}

impl Default for GratarGame {
    fn default() -> Self {
        let mut notes = Vec::new();
        let bpm = 160.0; // Nicolae Guta - Locul 1 is 160 BPM
        let beat_duration = 60.0 / bpm; // 0.375s per beat
        let step_duration = beat_duration / 2.0; // 0.1875s per step (half beats)

        // Generate rhythm notes from step 64 (~12 seconds, when beat starts) to step 1100 (~206 seconds)
        for step in 64..1100 {
            let pattern_step = step % 8;
            let mut spawn = false;
            let mut dir = NoteDirection::Up;

            if step < 128 {
                // Intro build-up: slow downbeats
                if pattern_step == 0 {
                    spawn = true;
                    dir = if (step / 8) % 2 == 0 { NoteDirection::Left } else { NoteDirection::Right };
                }
            } else if step >= 512 && step < 768 {
                // Chorus (Intense fast manele rolls): spawn on steps 0, 2, 3, 4, 6, 7
                if pattern_step == 0 {
                    spawn = true;
                    dir = NoteDirection::Down;
                } else if pattern_step == 2 {
                    spawn = true;
                    dir = NoteDirection::Left;
                } else if pattern_step == 3 {
                    spawn = true;
                    dir = NoteDirection::Right;
                } else if pattern_step == 4 {
                    spawn = true;
                    dir = NoteDirection::Up;
                } else if pattern_step == 6 {
                    spawn = true;
                    dir = NoteDirection::Left;
                } else if pattern_step == 7 {
                    spawn = true;
                    dir = NoteDirection::Right;
                }
            } else {
                // Standard bouncy Manele syncopated rhythm: spawn on steps 0, 3, 5, 6
                if pattern_step == 0 {
                    spawn = true;
                    dir = NoteDirection::Down;
                } else if pattern_step == 3 {
                    spawn = true;
                    dir = NoteDirection::Left;
                } else if pattern_step == 5 {
                    spawn = true;
                    dir = NoteDirection::Right;
                } else if pattern_step == 6 {
                    spawn = true;
                    dir = NoteDirection::Up;
                }
            }

            if spawn {
                notes.push(RhythmNote {
                    direction: dir,
                    time: step as f32 * step_duration,
                    hit: false,
                });
            }
        }

        Self {
            is_started: false,
            is_finished: false,
            needs_music_start: false,
            song_time: 0.0,
            notes,
            score: 0,
            combo: 0,
            max_combo: 0,
            multiplier: 1,
            last_rating: "".to_string(),
            last_rating_timer: 0.0,
            bpm,
        }
    }
}

#[derive(Resource)]
struct CameraDirector {
    active_shot: usize,
    shot_timer: f32,
    transition_timer: f32,
    transition_duration: f32,
    start_pos: Vec3,
    start_rot: Quat,
    target_pos: Vec3,
    target_rot: Quat,
    orbit_angle: f32,
    auto_cycle: bool,
}

impl Default for CameraDirector {
    fn default() -> Self {
        Self {
            active_shot: 3, // start with cinematic orbit overview
            shot_timer: 0.0,
            transition_timer: 1.0,
            transition_duration: 1.0,
            start_pos: Vec3::new(-10.0, 2.0, -15.0),
            start_rot: Quat::IDENTITY,
            target_pos: Vec3::new(-10.0, 2.0, -15.0),
            target_rot: Quat::IDENTITY,
            orbit_angle: 0.0,
            auto_cycle: true,
        }
    }
}

/// Spawns the composite 3D gratar model
fn setup_gratar(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // 1. Parent Gratar Root
    let root = commands
        .spawn((
            HydraulicGratar::default(),
            Transform::from_xyz(0.0, 1.2, 0.0),
            Visibility::default(),
            InheritedVisibility::default(),
        ))
        .id();

    // 2. Gratar Bowl (Hollow metal tub body built of 5 plates)
    let wall_color = Color::srgb(0.08, 0.08, 0.09);
    let metal_mat = materials.add(StandardMaterial {
        base_color: wall_color,
        metallic: 0.8,
        perceptual_roughness: 0.65,
        ..default()
    });

    // Plate dimensions & positions
    // Bottom plate
    let bottom = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.6, 0.04, 0.9))),
            MeshMaterial3d(metal_mat.clone()),
            Transform::from_xyz(0.0, -0.22, 0.0),
        ))
        .id();
    commands.entity(root).add_child(bottom);

    // Left wall
    let left_wall = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(0.04, 0.44, 0.9))),
            MeshMaterial3d(metal_mat.clone()),
            Transform::from_xyz(-0.78, 0.0, 0.0),
        ))
        .id();
    commands.entity(root).add_child(left_wall);

    // Right wall
    let right_wall = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(0.04, 0.44, 0.9))),
            MeshMaterial3d(metal_mat.clone()),
            Transform::from_xyz(0.78, 0.0, 0.0),
        ))
        .id();
    commands.entity(root).add_child(right_wall);

    // Front wall
    let front_wall = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.52, 0.44, 0.04))),
            MeshMaterial3d(metal_mat.clone()),
            Transform::from_xyz(0.0, 0.0, 0.43),
        ))
        .id();
    commands.entity(root).add_child(front_wall);

    // Back wall
    let back_wall = commands
        .spawn((
            Mesh3d(meshes.add(Cuboid::new(1.52, 0.44, 0.04))),
            MeshMaterial3d(metal_mat.clone()),
            Transform::from_xyz(0.0, 0.0, -0.43),
        ))
        .id();
    commands.entity(root).add_child(back_wall);

    // Side Ventilation Holes (represented by dark small cylinder caps on outer walls)
    let vent_mesh = meshes.add(Cylinder::new(0.015, 0.005));
    let vent_mat = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        perceptual_roughness: 0.9,
        ..default()
    });
    for x in [-0.5, -0.25, 0.0, 0.25, 0.5] {
        // Front wall vents
        let vent_front = commands
            .spawn((
                Mesh3d(vent_mesh.clone()),
                MeshMaterial3d(vent_mat.clone()),
                Transform::from_xyz(x, -0.1, 0.452)
                    .with_rotation(Quat::from_rotation_x(90.0f32.to_radians())),
            ))
            .id();
        commands.entity(root).add_child(vent_front);

        // Back wall vents
        let vent_back = commands
            .spawn((
                Mesh3d(vent_mesh.clone()),
                MeshMaterial3d(vent_mat.clone()),
                Transform::from_xyz(x, -0.1, -0.452)
                    .with_rotation(Quat::from_rotation_x(90.0f32.to_radians())),
            ))
            .id();
        commands.entity(root).add_child(vent_back);
    }

    // Handles: steel tubes on left/right side walls
    let handle_chrome_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.7, 0.72, 0.75),
        metallic: 0.95,
        perceptual_roughness: 0.15,
        ..default()
    });

    for side in [-1.0f32, 1.0f32] {
        let x_base = side * 0.8;
        // Two standoff connectors
        for z in [-0.2, 0.2] {
            let standoff = commands
                .spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.08, 0.02, 0.02))),
                    MeshMaterial3d(handle_chrome_mat.clone()),
                    Transform::from_xyz(x_base + side * 0.04, 0.0, z),
                ))
                .id();
            commands.entity(root).add_child(standoff);
        }
        // Connecting bar
        let bar = commands
            .spawn((
                Mesh3d(meshes.add(Cuboid::new(0.02, 0.02, 0.42))),
                MeshMaterial3d(handle_chrome_mat.clone()),
                Transform::from_xyz(x_base + side * 0.08, 0.0, 0.0),
            ))
            .id();
        commands.entity(root).add_child(bar);
    }

    // 3. Four Legs (Chrome look)
    let leg_mesh = meshes.add(Cuboid::new(0.08, 0.9, 0.08));
    let leg_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.65, 0.67, 0.7),
        metallic: 0.9,
        perceptual_roughness: 0.2,
        ..default()
    });

    let leg_positions = [
        Vec3::new(-0.7, -0.45, -0.35),
        Vec3::new(0.7, -0.45, -0.35),
        Vec3::new(-0.7, -0.45, 0.35),
        Vec3::new(0.7, -0.45, 0.35),
    ];

    for pos in leg_positions {
        let child_leg = commands
            .spawn((
                Mesh3d(leg_mesh.clone()),
                MeshMaterial3d(leg_mat.clone()),
                Transform::from_translation(pos),
            ))
            .id();
        commands.entity(root).add_child(child_leg);
    }

    // 4. Bed of 50 individual Glowing Charcoal Embers
    let ash_coal_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.06, 0.06, 0.07),
        perceptual_roughness: 0.95,
        ..default()
    });
    let glow_coal_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.06, 0.04),
        emissive: LinearRgba::new(8.0, 1.8, 0.1, 1.0), // intense glowing orange/yellow embers
        perceptual_roughness: 0.9,
        ..default()
    });

    // Mesh handles to reuse
    let cuboid_coal_mesh = meshes.add(Cuboid::new(0.12, 0.09, 0.12));
    let sphere_coal_mesh = meshes.add(Sphere::new(0.06));

    // Seed/determinstic randomized pile of coals
    let mut coal_rand_x = 0.123f32;
    let mut coal_rand_z = 0.567f32;
    let mut coal_rand_y = 0.891f32;

    let next_rand = |seed: &mut f32| -> f32 {
        *seed = (*seed * 43.12351 + 0.9234).fract();
        *seed
    };

    for i in 0..50 {
        let rx = next_rand(&mut coal_rand_x) * 2.0 - 1.0; // [-1.0, 1.0]
        let rz = next_rand(&mut coal_rand_z) * 2.0 - 1.0;
        let ry = next_rand(&mut coal_rand_y);

        let coal_x = rx * 0.68;
        let coal_z = rz * 0.35;
        let coal_y = -0.16 + ry * 0.14;

        let use_glow = i % 3 != 0; // ~66% glow, 33% ash
        let mat = if use_glow { glow_coal_mat.clone() } else { ash_coal_mat.clone() };
        let mesh = if i % 2 == 0 { cuboid_coal_mesh.clone() } else { sphere_coal_mesh.clone() };

        let yaw = next_rand(&mut coal_rand_x) * std::f32::consts::TAU;
        let pitch = next_rand(&mut coal_rand_z) * std::f32::consts::TAU;

        let child_coal = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(mat),
                Transform::from_xyz(coal_x, coal_y, coal_z)
                    .with_rotation(Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0)),
            ))
            .id();
        commands.entity(root).add_child(child_coal);
    }

    // 5. Realistic Wireframe Steel Grill Grate (Frame + 24 parallel wires)
    let grate_steel_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.38, 0.39, 0.42),
        metallic: 0.9,
        perceptual_roughness: 0.4,
        ..default()
    });

    let wire_mesh = meshes.add(Cuboid::new(0.008, 0.012, 0.8));
    let frame_z_mesh = meshes.add(Cuboid::new(0.02, 0.02, 0.82));
    let frame_x_mesh = meshes.add(Cuboid::new(1.49, 0.02, 0.02));

    // Outer frame borders
    // Left border
    let border_l = commands
        .spawn((
            Mesh3d(frame_z_mesh.clone()),
            MeshMaterial3d(grate_steel_mat.clone()),
            Transform::from_xyz(-0.74, 0.22, 0.0),
        ))
        .id();
    commands.entity(root).add_child(border_l);

    // Right border
    let border_r = commands
        .spawn((
            Mesh3d(frame_z_mesh.clone()),
            MeshMaterial3d(grate_steel_mat.clone()),
            Transform::from_xyz(0.74, 0.22, 0.0),
        ))
        .id();
    commands.entity(root).add_child(border_r);

    // Front border
    let border_f = commands
        .spawn((
            Mesh3d(frame_x_mesh.clone()),
            MeshMaterial3d(grate_steel_mat.clone()),
            Transform::from_xyz(0.0, 0.22, 0.4),
        ))
        .id();
    commands.entity(root).add_child(border_f);

    // Back border
    let border_b = commands
        .spawn((
            Mesh3d(frame_x_mesh.clone()),
            MeshMaterial3d(grate_steel_mat.clone()),
            Transform::from_xyz(0.0, 0.22, -0.4),
        ))
        .id();
    commands.entity(root).add_child(border_b);

    // Parallel wires (24 wires spaced across X = -0.71 to 0.71)
    let wire_count = 24;
    for w in 0..wire_count {
        let t = w as f32 / (wire_count - 1) as f32;
        let x_pos = -0.71 + t * 1.42;

        let wire = commands
            .spawn((
                Mesh3d(wire_mesh.clone()),
                MeshMaterial3d(grate_steel_mat.clone()),
                Transform::from_xyz(x_pos, 0.22, 0.0),
            ))
            .id();
        commands.entity(root).add_child(wire);
    }

    // 6. Sausages / Romanian Mici
    // Modeled as a Capsule3d
    let mic_radius = 0.045;
    let mic_half_length = 0.11;
    let mic_mesh = meshes.add(Capsule3d::new(mic_radius, mic_half_length));
    
    // Glistening, juicy cooked meat look
    let mic_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.14, 0.09), // rich caramelized dark brown
        perceptual_roughness: 0.55,               // semi-glossy roasted texture
        reflectance: 0.25,                        // realistic organic highlight
        ..default()
    });

    // Charcoal black grill marks material
    let grill_mark_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.01, 0.01, 0.01),
        perceptual_roughness: 0.95,
        ..default()
    });
    // Thin cuboid that slightly wraps over the top surface
    let grill_mark_mesh = meshes.add(Cuboid::new(0.092, 0.006, 0.012));

    // Spawning 7 mici in a natural arrangement
    let mic_configs = [
        (Vec3::new(-0.45, 0.28, -0.15), 18.0f32.to_radians()),
        (Vec3::new(-0.2, 0.28, 0.18), -8.0f32.to_radians()),
        (Vec3::new(0.05, 0.28, -0.2), 35.0f32.to_radians()),
        (Vec3::new(0.1, 0.28, 0.1), -25.0f32.to_radians()),
        (Vec3::new(0.35, 0.28, -0.15), 12.0f32.to_radians()),
        (Vec3::new(0.48, 0.28, 0.2), -40.0f32.to_radians()),
        (Vec3::new(-0.5, 0.28, 0.15), -15.0f32.to_radians()),
    ];

    for (pos, angle) in mic_configs {
        // Spawning a mic capsule, oriented horizontally (rotate on X to lay down, then on Y for pattern angle)
        // By default, Capsule3d is oriented vertically along local Y.
        // We rotate it by 90 degrees around X so its length is along local Z.
        let local_rot = Quat::from_rotation_x(90.0f32.to_radians());
        let final_rot = Quat::from_rotation_y(angle) * local_rot;

        let child_mic = commands
            .spawn((
                Mesh3d(mic_mesh.clone()),
                MeshMaterial3d(mic_mat.clone()),
                Transform::from_translation(pos).with_rotation(final_rot),
            ))
            .id();
        commands.entity(root).add_child(child_mic);

        // Add 4 parallel burnt grill marks on top of each mic
        // Since the mic is rotated so its length is Z, local Z ranges from -0.11 to 0.11
        // Spaced along Z, we place thin black strips at local Y = mic_radius (0.045)
        let z_offsets = [-0.07, -0.02, 0.03, 0.08];
        for z_off in z_offsets {
            let mark = commands
                .spawn((
                    Mesh3d(grill_mark_mesh.clone()),
                    MeshMaterial3d(grill_mark_mat.clone()),
                    Transform::from_xyz(0.0, mic_radius - 0.001, z_off),
                ))
                .id();
            // Parent the mark directly to the mic so it rotates and moves with it
            commands.entity(child_mic).add_child(mark);
        }
    }
}

/// Simulates spring-damp hydraulics physics for the gratar
fn gratar_hydraulics_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut HydraulicGratar)>,
) {
    let dt = time.delta_secs().min(0.05);
    let Ok((mut transform, mut gratar)) = query.single_mut() else {
        return;
    };

    // Spring constants (Juicy bounces)
    let spring_k = 160.0;
    let damping_k = 14.0;

    let spring_k_rot = 190.0;
    let damping_k_rot = 12.0;

    // 1. Spring forces on translation (bounce back to Y=1.2 offset)
    let translation_force = -spring_k * gratar.translation - damping_k * gratar.velocity;
    let velocity = gratar.velocity + translation_force * dt;
    gratar.velocity = velocity;
    gratar.translation += velocity * dt;

    // 2. Spring forces on rotation (tilt back to upright)
    let rotation_force =
        -spring_k_rot * gratar.rotation_euler - damping_k_rot * gratar.angular_velocity;
    let angular_velocity = gratar.angular_velocity + rotation_force * dt;
    gratar.angular_velocity = angular_velocity;
    gratar.rotation_euler += angular_velocity * dt;

    // 3. Apply coordinates to transform
    transform.translation = Vec3::new(0.0, 1.2, 0.0) + gratar.translation;
    transform.rotation = Quat::from_euler(
        EulerRot::YXZ,
        gratar.rotation_euler.y,
        gratar.rotation_euler.x,
        gratar.rotation_euler.z,
    );
}

/// Spawns and animates smoke particles rising from the hot grill coals
fn smoke_particles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    gratar_query: Query<&Transform, With<HydraulicGratar>>,
    mut particles_query: Query<
        (
            Entity,
            &mut Transform,
            &mut SmokeParticle,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Without<HydraulicGratar>,
    >,
    mut spawn_timer: Local<f32>,
) {
    let dt = time.delta_secs();

    // 1. Update existing smoke particles
    for (entity, mut transform, mut particle, material_handle) in particles_query.iter_mut() {
        particle.lifetime += dt;
        if particle.lifetime >= particle.max_lifetime {
            commands.entity(entity).despawn();
            continue;
        }

        // Float up and drift
        transform.translation += particle.velocity * dt;

        // Grow size
        let progress = particle.lifetime / particle.max_lifetime;
        let scale = 0.1 + progress * 0.45;
        transform.scale = Vec3::splat(scale);

        // Fade out alpha
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            material.base_color = Color::srgba(0.82, 0.82, 0.85, 0.5 * (1.0 - progress));
        }
    }

    // 2. Spawn new particles
    *spawn_timer += dt;
    if *spawn_timer >= 0.18 {
        *spawn_timer = 0.0;

        if let Ok(gratar_transform) = gratar_query.single() {
            // Spawn inside the grate boundaries
            let local_x = (rand::random::<f32>() - 0.5) * 1.3;
            let local_z = (rand::random::<f32>() - 0.5) * 0.7;

            // Compute global coordinate
            let spawn_pos = gratar_transform.transform_point(Vec3::new(local_x, 0.3, local_z));

            let smoke_mesh = meshes.add(Sphere::new(1.0));
            let smoke_mat = materials.add(StandardMaterial {
                base_color: Color::srgba(0.8, 0.8, 0.8, 0.5),
                perceptual_roughness: 1.0,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });

            let velocity = Vec3::new(
                (rand::random::<f32>() - 0.5) * 0.25,
                0.7 + rand::random::<f32>() * 0.4,
                (rand::random::<f32>() - 0.5) * 0.25,
            );

            commands.spawn((
                Mesh3d(smoke_mesh),
                MeshMaterial3d(smoke_mat),
                Transform::from_translation(spawn_pos).with_scale(Vec3::splat(0.1)),
                SmokeParticle {
                    velocity,
                    lifetime: 0.0,
                    max_lifetime: 1.1 + rand::random::<f32>() * 0.6,
                },
            ));
        }
    }
}

/// Evaluates player note timing hits and updates game statistics
fn rhythm_game_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut game: ResMut<GratarGame>,
    mut gratar_query: Query<&mut HydraulicGratar>,
    audio_query: Query<Entity, With<AudioPlayer>>,
) {
    if !game.is_started {
        if keyboard.just_pressed(KeyCode::Space) {
            game.is_started = true;
            game.needs_music_start = true;
            game.is_finished = false;
            game.song_time = 0.0;
            game.score = 0;
            game.combo = 0;
            game.max_combo = 0;
            game.multiplier = 1;
            game.last_rating = "START!".to_string();
            game.last_rating_timer = 0.8;
            for note in game.notes.iter_mut() {
                note.hit = false;
            }
        }
        return;
    }

    // Handle music starting (either from spacebar or UI button click)
    if game.needs_music_start {
        game.needs_music_start = false;
        
        // Stop any existing song playing
        for entity in audio_query.iter() {
            commands.entity(entity).despawn();
        }

        // Spawn the music player
        let song_url = format!(
            "{}sound_data/ManeleMp3.Net%20-%20NICOLAE%20GUTA%20-%20LOCUL%201%20NUMAI%201%20%5BORIGINALA%5D.mp3",
            demo_resolution_selector_web_bevy::config::DATA_BASE_URL
        );
        commands.spawn((
            AudioPlayer::new(asset_server.load(song_url)),
            PlaybackSettings {
                mode: bevy::audio::PlaybackMode::Despawn,
                volume: bevy::audio::Volume::Linear(1.0),
                spatial: false, // global stereo
                ..default()
            },
        ));
    }

    // 1. Advance track time
    game.song_time += time.delta_secs();

    // 2. Check song end
    if game.song_time > 212.0 {
        game.is_started = false;
        game.is_finished = true;
        
        // Despawn the music player
        for entity in audio_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    // 3. Mark missed notes
    let song_time = game.song_time;
    let mut any_missed = false;
    for note in game.notes.iter_mut() {
        if !note.hit && song_time > note.time + 0.45 {
            note.hit = true;
            any_missed = true;
        }
    }
    if any_missed {
        game.combo = 0;
        game.multiplier = 1;
        game.last_rating = "MISS!".to_string();
        game.last_rating_timer = 0.6;
    }

    // 4. Capture input direction
    let mut pressed_dir = None;
    if keyboard.just_pressed(KeyCode::ArrowLeft) || keyboard.just_pressed(KeyCode::KeyA) {
        pressed_dir = Some(NoteDirection::Left);
    } else if keyboard.just_pressed(KeyCode::ArrowRight) || keyboard.just_pressed(KeyCode::KeyD) {
        pressed_dir = Some(NoteDirection::Right);
    } else if keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW) {
        pressed_dir = Some(NoteDirection::Up);
    } else if keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS) {
        pressed_dir = Some(NoteDirection::Down);
    }

    let Some(dir) = pressed_dir else {
        return;
    };

    // Apply hydraulic impulse instantly
    if let Ok(mut gratar) = gratar_query.single_mut() {
        match dir {
            NoteDirection::Left => {
                gratar.angular_velocity.z += 9.0;
                gratar.velocity.y += 2.0;
            }
            NoteDirection::Right => {
                gratar.angular_velocity.z -= 9.0;
                gratar.velocity.y += 2.0;
            }
            NoteDirection::Up => {
                gratar.angular_velocity.x += 9.0;
                gratar.velocity.y += 4.2;
            }
            NoteDirection::Down => {
                gratar.angular_velocity.x -= 9.0;
                gratar.velocity.y -= 2.6;
            }
        }
    }

    // Find closest active note matching direction
    let mut closest_idx = None;
    let mut min_diff = 999.0;

    for (idx, note) in game.notes.iter().enumerate() {
        if !note.hit && note.direction == dir {
            let diff = (note.time - game.song_time).abs();
            if diff < min_diff && diff <= 0.45 {
                min_diff = diff;
                closest_idx = Some(idx);
            }
        }
    }

    if let Some(idx) = closest_idx {
        game.notes[idx].hit = true;

        // Score depending on timing precision
        if min_diff < 0.12 {
            game.score += 100 * game.multiplier;
            game.combo += 1;
            game.last_rating = "PERFECT!".to_string();
        } else if min_diff < 0.28 {
            game.score += 50 * game.multiplier;
            game.combo += 1;
            game.last_rating = "GOOD!".to_string();
        } else {
            game.score += 10 * game.multiplier;
            game.combo = 0;
            game.last_rating = "BAD!".to_string();
        }
        game.max_combo = game.max_combo.max(game.combo);
        game.last_rating_timer = 0.6;

        // Multiplier progression
        game.multiplier = if game.combo >= 30 {
            4
        } else if game.combo >= 20 {
            3
        } else if game.combo >= 10 {
            2
        } else {
            1
        };
    } else {
        // Off-beat trigger penalty
        game.combo = 0;
        game.multiplier = 1;
        game.last_rating = "BAD timing!".to_string();
        game.last_rating_timer = 0.4;
    }
}

/// Renders the rhythm game HUD and status panels using egui
fn rhythm_ui_system(
    mut contexts: EguiContexts,
    mut game: ResMut<GratarGame>,
    director: Res<CameraDirector>,
    time: Res<Time>,
) {
    if time.elapsed_secs() < 0.2 {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let screen_w = ctx.input(|i| i.screen_rect().width());
    let screen_h = ctx.input(|i| i.screen_rect().height());

    // 1. Show floating rating feedback in center
    if game.last_rating_timer > 0.0 {
        game.last_rating_timer -= time.delta_secs();

        let color = match game.last_rating.as_str() {
            "PERFECT!" => egui::Color32::from_rgb(0, 255, 127),
            "GOOD!" => egui::Color32::from_rgb(0, 180, 216),
            "BAD!" | "BAD timing!" => egui::Color32::from_rgb(255, 110, 0),
            "MISS!" => egui::Color32::from_rgb(255, 0, 0),
            _ => egui::Color32::WHITE,
        };

        let size = 26.0 + 24.0 * (game.last_rating_timer / 0.6).min(1.0);

        egui::Area::new(egui::Id::new("rating_overlay"))
            .fixed_pos(egui::pos2(screen_w / 2.0, screen_h / 2.0 - 120.0))
            .show(ctx, |ui| {
                ui.painter().text(
                    egui::pos2(0.0, 0.0),
                    egui::Align2::CENTER_CENTER,
                    &game.last_rating,
                    egui::FontId::proportional(size),
                    color,
                );
            });
    }

    // 2. Play screen branching
    if !game.is_started && !game.is_finished {
        // Start Overlay
        egui::Window::new("Cezar's Gratar Challenge")
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("🍖 THE LOW-RIDER BBQ CHALLENGE 🍖")
                            .strong()
                            .size(16.0),
                    );
                    ui.separator();
                    ui.label("Time your bounces to the rhythm of sizzle!");
                    ui.allocate_space(egui::vec2(0.0, 8.0));

                    ui.label("Controls:");
                    ui.label(
                        egui::RichText::new("A / Left Arrow: Bounce Left\nD / Right Arrow: Bounce Right\nW / Up Arrow: Bounce Up\nS / Down Arrow: Bounce Down")
                            .monospace()
                            .color(egui::Color32::from_rgb(200, 200, 200))
                    );

                    ui.allocate_space(egui::vec2(0.0, 10.0));

                    if ui
                        .button(egui::RichText::new("START CHALLENGE (SPACEBAR)").strong().size(14.0))
                        .clicked()
                    {
                        game.is_started = true;
                        game.needs_music_start = true;
                        game.song_time = 0.0;
                        game.score = 0;
                        game.combo = 0;
                        game.max_combo = 0;
                        game.multiplier = 1;
                        game.last_rating = "START!".to_string();
                        game.last_rating_timer = 0.8;
                        for note in game.notes.iter_mut() {
                            note.hit = false;
                        }
                    }
                });
            });
    } else if game.is_finished {
        // Finish Overlay
        egui::Window::new("Challenge Finished!")
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("🎉 CHALLENGE COMPLETE 🎉")
                            .strong()
                            .size(18.0)
                            .color(egui::Color32::from_rgb(0, 255, 127)),
                    );
                    ui.separator();

                    ui.label(format!("FINAL SCORE: {}", game.score));
                    ui.label(format!("MAX COMBO: x{}", game.max_combo));

                    let grade = if game.score >= 140000 {
                        "S - Legendary Gratar Master 👑"
                    } else if game.score >= 90000 {
                        "A - Good Mici Flipper 🍳"
                    } else if game.score >= 50000 {
                        "B - Casual Cook 🥩"
                    } else if game.score >= 20000 {
                        "C - Charcoal Burner 🪵"
                    } else {
                        "F - Burnt to a Crisp ☠️"
                    };

                    ui.label(
                        egui::RichText::new(format!("GRADE: {}", grade))
                            .strong()
                            .size(14.0)
                            .color(egui::Color32::from_rgb(78, 205, 196)),
                    );

                    ui.allocate_space(egui::vec2(0.0, 12.0));

                    if ui
                        .button(egui::RichText::new("PLAY AGAIN (SPACEBAR)").strong().size(14.0))
                        .clicked()
                    {
                        game.is_started = true;
                        game.is_finished = false;
                        game.needs_music_start = true;
                        game.song_time = 0.0;
                        game.score = 0;
                        game.combo = 0;
                        game.max_combo = 0;
                        game.multiplier = 1;
                        game.last_rating = "START!".to_string();
                        game.last_rating_timer = 0.8;
                        for note in game.notes.iter_mut() {
                            note.hit = false;
                        }
                    }
                });
            });
    } else {
        // Active Rhythm HUD Panel
        let hud_w = 780.0f32.min(screen_w - 40.0);
        let hud_h = 100.0;

        let hud_x = (screen_w - hud_w) / 2.0;
        let hud_y = screen_h - hud_h - 25.0;

        egui::Area::new(egui::Id::new("hud_area"))
            .fixed_pos(egui::pos2(hud_x, hud_y))
            .show(ctx, |ui| {
                let painter = ui.painter();
                let rect = egui::Rect::from_min_size(
                    egui::pos2(hud_x, hud_y),
                    egui::vec2(hud_w, hud_h),
                );

                // Background glassmorphism panel
                painter.rect_filled(
                    rect,
                    12.0,
                    egui::Color32::from_rgba_unmultiplied(15, 15, 15, 210),
                );
                // Glow border
                painter.rect_stroke(
                    rect,
                    12.0,
                    egui::Stroke::new(1.8, egui::Color32::from_rgb(0, 180, 216)),
                    egui::StrokeKind::Inside,
                );

                // 1. Stats Box
                let stats_rect = egui::Rect::from_min_max(
                    egui::pos2(hud_x + 15.0, hud_y + 12.0),
                    egui::pos2(hud_x + 180.0, hud_y + hud_h - 12.0),
                );
                painter.rect_filled(
                    stats_rect,
                    6.0,
                    egui::Color32::from_black_alpha(120),
                );

                painter.text(
                    egui::pos2(hud_x + 25.0, hud_y + 26.0),
                    egui::Align2::LEFT_CENTER,
                    format!("SCORE: {:06}", game.score),
                    egui::FontId::monospace(13.0),
                    egui::Color32::WHITE,
                );

                let combo_color = if game.combo >= 20 {
                    egui::Color32::from_rgb(255, 209, 102)
                } else if game.combo >= 10 {
                    egui::Color32::from_rgb(0, 255, 244)
                } else {
                    egui::Color32::WHITE
                };

                painter.text(
                    egui::pos2(hud_x + 25.0, hud_y + 50.0),
                    egui::Align2::LEFT_CENTER,
                    format!("COMBO: x{}", game.combo),
                    egui::FontId::monospace(13.0),
                    combo_color,
                );

                painter.text(
                    egui::pos2(hud_x + 25.0, hud_y + 74.0),
                    egui::Align2::LEFT_CENTER,
                    format!("MULT: x{}", game.multiplier),
                    egui::FontId::monospace(13.0),
                    egui::Color32::from_rgb(78, 205, 196),
                );

                // 2. Scrolling Track
                let track_y = hud_y + hud_h / 2.0;
                let track_start = hud_x + 200.0;
                let track_end = hud_x + hud_w - 200.0;

                // Main track line
                painter.line_segment(
                    [egui::pos2(track_start, track_y), egui::pos2(track_end, track_y)],
                    egui::Stroke::new(2.0, egui::Color32::from_gray(80)),
                );

                // Target timing keycap shape
                let target_x = hud_x + 260.0;
                let beat_pulse = 2.5 * (game.song_time * 2.0 * std::f32::consts::PI).sin().abs();
                let target_w = 40.0;
                let target_h = 40.0;
                let target_rect = egui::Rect::from_center_size(
                    egui::pos2(target_x, track_y),
                    egui::vec2(target_w + beat_pulse, target_h + beat_pulse),
                );
                
                painter.rect_filled(
                    target_rect,
                    8.0,
                    egui::Color32::from_rgba_unmultiplied(0, 180, 216, 40),
                );
                painter.rect_stroke(
                    target_rect,
                    8.0,
                    egui::Stroke::new(2.5, egui::Color32::from_rgb(0, 255, 244)),
                    egui::StrokeKind::Inside,
                );

                // 3. Render scrolling notes as keycaps
                let scroll_speed = 280.0; // pixels per second

                for note in &game.notes {
                    if note.hit {
                        continue;
                    }

                    let x_pos = target_x + (note.time - game.song_time) * scroll_speed;
                    if x_pos >= target_x - 30.0 && x_pos <= track_end + 15.0 {
                        let color = match note.direction {
                            NoteDirection::Left => egui::Color32::from_rgb(239, 71, 111),
                            NoteDirection::Right => egui::Color32::from_rgb(78, 205, 196),
                            NoteDirection::Up => egui::Color32::from_rgb(255, 209, 102),
                            NoteDirection::Down => egui::Color32::from_rgb(6, 214, 160),
                        };

                        let rect_w = 36.0;
                        let rect_h = 36.0;
                        let note_rect = egui::Rect::from_center_size(
                            egui::pos2(x_pos, track_y),
                            egui::vec2(rect_w, rect_h),
                        );
                        // Draw keycap base shadow (3D effect)
                        let shadow_rect = note_rect.translate(egui::vec2(0.0, 3.0));
                        painter.rect_filled(
                            shadow_rect,
                            6.0,
                            egui::Color32::from_rgb(30, 30, 30),
                        );
                        // Draw keycap face
                        painter.rect_filled(
                            note_rect,
                            6.0,
                            color,
                        );
                        painter.rect_stroke(
                            note_rect,
                            6.0,
                            egui::Stroke::new(1.8, egui::Color32::WHITE),
                            egui::StrokeKind::Inside,
                        );

                        let label = match note.direction {
                            NoteDirection::Left => "A/←",
                            NoteDirection::Right => "D/→",
                            NoteDirection::Up => "W/↑",
                            NoteDirection::Down => "S/↓",
                        };

                        painter.text(
                            egui::pos2(x_pos, track_y),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::monospace(12.0),
                            egui::Color32::BLACK,
                        );
                    }
                }

                // 4. Camera controls helper panel (Right side)
                let cam_box_rect = egui::Rect::from_min_max(
                    egui::pos2(hud_x + hud_w - 180.0, hud_y + 12.0),
                    egui::pos2(hud_x + hud_w - 15.0, hud_y + hud_h - 12.0),
                );
                painter.rect_filled(
                    cam_box_rect,
                    6.0,
                    egui::Color32::from_black_alpha(120),
                );

                painter.text(
                    egui::pos2(hud_x + hud_w - 170.0, hud_y + 26.0),
                    egui::Align2::LEFT_CENTER,
                    "CAMERA SHOTS:",
                    egui::FontId::monospace(11.0),
                    egui::Color32::from_rgb(180, 180, 180),
                );
                painter.text(
                    egui::pos2(hud_x + hud_w - 170.0, hud_y + 48.0),
                    egui::Align2::LEFT_CENTER,
                    "[1-5] Preset Shots",
                    egui::FontId::monospace(11.0),
                    egui::Color32::WHITE,
                );
                let cycle_status = if director.auto_cycle { "ON" } else { "OFF" };
                painter.text(
                    egui::pos2(hud_x + hud_w - 170.0, hud_y + 70.0),
                    egui::Align2::LEFT_CENTER,
                    format!("[C] Auto-cycle: {cycle_status}"),
                    egui::FontId::monospace(11.0),
                    egui::Color32::from_rgb(0, 180, 216),
                );
            });
    }
}

/// Dynamic cinematic camera transitions and slow orbit tracking
fn cinematic_camera_system(
    time: Res<Time>,
    game: Res<GratarGame>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut director: ResMut<CameraDirector>,
    gratar_query: Query<&Transform, With<HydraulicGratar>>,
    mut camera_query: Query<&mut Transform, (With<Camera>, Without<HydraulicGratar>)>,
) {
    let dt = time.delta_secs();
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let gratar_pos = if let Ok(gt) = gratar_query.single() {
        gt.translation
    } else {
        Vec3::new(0.0, 1.2, 0.0)
    };

    // 1. Slow orbit angle update
    director.orbit_angle += 0.22 * dt;

    // Manual camera select via Keys 1-5, toggle auto-cycle with KeyC
    if keyboard.just_pressed(KeyCode::KeyC) {
        director.auto_cycle = !director.auto_cycle;
        director.shot_timer = 0.0;
    }

    let mut selected_shot = None;
    if keyboard.just_pressed(KeyCode::Digit1) {
        selected_shot = Some(0);
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        selected_shot = Some(1);
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        selected_shot = Some(2);
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        selected_shot = Some(3);
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        selected_shot = Some(4);
    }

    if let Some(shot) = selected_shot {
        director.active_shot = shot;
        director.auto_cycle = false; // Turn off auto-cycling on manual selection
        director.shot_timer = 0.0;
        director.transition_timer = 0.0;
        director.start_pos = camera_transform.translation;
        director.start_rot = camera_transform.rotation;
    }

    // 2. State machine for shot selections
    if game.is_started {
        if director.auto_cycle {
            director.shot_timer += dt;
            if director.shot_timer >= 6.5 {
                director.shot_timer = 0.0;

                // Cycle to a different camera shot
                let mut next_shot = director.active_shot;
                while next_shot == director.active_shot {
                    next_shot = (rand::random::<f32>() * 5.0) as usize;
                }
                director.active_shot = next_shot;
                director.transition_timer = 0.0;
                director.start_pos = camera_transform.translation;
                director.start_rot = camera_transform.rotation;
            }
        }
    } else {
        // Showcase orbit view when idle
        if director.active_shot != 3 {
            director.active_shot = 3;
            director.transition_timer = 0.0;
            director.start_pos = camera_transform.translation;
            director.start_rot = camera_transform.rotation;
        }
    }

    // 3. Compute active shot targets
    let (target_pos, target_rot) =
        get_camera_shot_targets(director.active_shot, director.orbit_angle, gratar_pos);
    director.target_pos = target_pos;
    director.target_rot = target_rot;

    // 4. Update camera positions smoothly
    if director.transition_timer < director.transition_duration {
        director.transition_timer += dt;
        let t = (director.transition_timer / director.transition_duration).clamp(0.0, 1.0);
        let t_smooth = t * t * (3.0 - 2.0 * t);

        camera_transform.translation = director.start_pos.lerp(director.target_pos, t_smooth);
        camera_transform.rotation = director.start_rot.slerp(director.target_rot, t_smooth);
    } else {
        camera_transform.translation = director.target_pos;
        camera_transform.rotation = director.target_rot;
    }
}

/// Presets of camera position & rotation relative to the gratar center
fn get_camera_shot_targets(shot_idx: usize, orbit_angle: f32, gratar_pos: Vec3) -> (Vec3, Quat) {
    let look_target = gratar_pos + Vec3::new(0.0, 0.2, 0.0);

    match shot_idx {
        0 => {
            // Low-Angle Front Closeup
            let pos = gratar_pos + Vec3::new(0.0, -0.4, 2.2);
            let rot = Transform::from_translation(pos)
                .looking_at(look_target, Vec3::Y)
                .rotation;
            (pos, rot)
        }
        1 => {
            // Close side view tracking hydraulic legs
            let pos = gratar_pos + Vec3::new(-2.3, 0.0, -0.4);
            let rot = Transform::from_translation(pos)
                .looking_at(gratar_pos + Vec3::new(0.0, -0.35, 0.0), Vec3::Y)
                .rotation;
            (pos, rot)
        }
        2 => {
            // Spinning top-down overhead
            let pos = gratar_pos
                + Vec3::new(orbit_angle.cos() * 2.0, 2.7, orbit_angle.sin() * 2.0);
            let rot = Transform::from_translation(pos)
                .looking_at(look_target, Vec3::Y)
                .rotation;
            (pos, rot)
        }
        3 => {
            // Wide orbit tracking
            let pos = gratar_pos
                + Vec3::new(orbit_angle.sin() * 3.6, 0.8, orbit_angle.cos() * 3.6);
            let rot = Transform::from_translation(pos)
                .looking_at(look_target, Vec3::Y)
                .rotation;
            (pos, rot)
        }
        _ => {
            // Dutch tilt corner closeup
            let pos = gratar_pos + Vec3::new(1.8, 1.4, -1.8);
            let look_rot = Transform::from_translation(pos)
                .looking_at(look_target, Vec3::Y)
                .rotation;
            let rot = look_rot * Quat::from_rotation_z(14.0f32.to_radians());
            (pos, rot)
        }
    }
}
