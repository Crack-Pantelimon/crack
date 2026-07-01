use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        settings::{Backends, WgpuSettings},
        RenderPlugin,
    },
    window::WindowResolution,
    world_serialization::WorldAssetRoot,
};

use avian3d::prelude::{CollisionLayers, Restitution, RigidBody, Mass, Collider, LinearVelocity, AngularVelocity, LockedAxes};
use bevy_egui::{egui, EguiContexts};

use demo_resolution_selector_web_bevy::{
    plugins::{
        cars_driving::driving_plugin::GamePhysicsLayer,
        physics_plugin::PhysicsPlugin,
        states::GameStatesPlugin,
    },
    ui_egui::UiState,
};

#[derive(Component)]
struct Pedestrian;

#[derive(Component)]
struct PedestrianVisual;

#[derive(Component)]
struct Torso;

#[derive(Component)]
struct Head;

#[derive(Component)]
struct LeftArm;

#[derive(Component)]
struct RightArm;

#[derive(Component)]
struct LeftLeg;

#[derive(Component)]
struct RightLeg;

#[derive(Component, Default)]
struct PedestrianAnimator {
    phase: f32,
    bob: f32,
    sway: f32,
    lean: f32,
    jump_timer: Option<f32>,
    punch_timer: Option<f32>,
    is_ragdoll: bool,
}

#[derive(Resource)]
struct PedestrianSettings {
    model_type: PedestrianModelType,
    swing_amplitude: f32,
    walk_speed: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PedestrianModelType {
    GlbModel,
    ProceduralPuppet,
    RiggedGlb,
}

#[derive(Component)]
#[allow(dead_code)]
struct BoneBaseRotation {
    base_rotation: Quat,
    local_lateral_axis: Vec3,
    local_vertical_axis: Vec3,
}

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
                        title: "Pedestrian Animations".into(),
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
        .init_resource::<UiState>() // Satisfies PhysicsPlugin's sync_physics_debug_config
        .insert_resource(PedestrianSettings {
            model_type: PedestrianModelType::RiggedGlb,
            swing_amplitude: 0.6,
            walk_speed: 2.5,
        })
        .add_plugins(PhysicsPlugin)
        .add_plugins(GameStatesPlugin)
        .add_systems(Startup, setup_scene)
        .add_systems(bevy_egui::EguiPrimaryContextPass, draw_pedestrian_ui)
        .add_systems(
            Update,
            (
                move_pedestrian,
                camera_follows_pedestrian,
                animate_pedestrian,
            ),
        )
        .add_systems(
            PostUpdate,
            init_bone_base_rotations.after(bevy::transform::TransformSystems::Propagate),
        )
        .run();
}

fn create_grayscale_texture(gray1: u8, gray2: u8) -> Image {
    let mut texture_data = vec![0; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let color = if (x / 4 + y / 4) % 2 == 0 { gray1 } else { gray2 };
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
    asset_server: Res<AssetServer>,
) {
    // 1. Spawning 4 ground cubes of size 500x500x500
    let cubes_info = [
        (Vec3::new(250.0, -250.0, 250.0), (50, 70)),
        (Vec3::new(-250.0, -250.0, 250.0), (90, 110)),
        (Vec3::new(250.0, -250.0, -250.0), (130, 150)),
        (Vec3::new(-250.0, -250.0, -250.0), (170, 190)),
    ];

    for (center, (gray1, gray2)) in cubes_info {
        let tile_repeat: f32 = 1.0 + rand::random::<f32>() * 2.0;

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
            avian3d::prelude::Collider::cuboid(500.0, 500.0, 500.0),
            Restitution::ZERO.with_combine_rule(avian3d::prelude::CoefficientCombine::Min),
            CollisionLayers::new(
                [GamePhysicsLayer::Map],
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car, GamePhysicsLayer::Wheel],
            ),
        ));
    }

    // 2. Spawn camera with AmbientLight component
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 15.0, -30.0).looking_at(Vec3::ZERO, Vec3::Y),
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

    // 4. Spawn Pedestrian with Physics & Rigged GLB visual child (default)
    let base_url = demo_resolution_selector_web_bevy::config::DATA_BASE_URL.trim_end_matches('/');
    let pedestrian_url = format!(
        "{}/3d_data/3d_slop_models_clean/pedestrian/armin_rigged.glb",
        base_url
    );
    let pedestrian_handle = asset_server.load(GltfAssetLabel::Scene(0).from_asset(pedestrian_url));

    commands.spawn((
        Transform::from_xyz(0.0, 2.0, 0.0),
        Pedestrian,
        PedestrianAnimator::default(),
        RigidBody::Dynamic,
        Collider::cuboid(0.6, 1.8, 0.6),
        LockedAxes::new().lock_rotation_x().lock_rotation_z(),
        Mass(100.0),
        CollisionLayers::new(
            [GamePhysicsLayer::Car],
            [GamePhysicsLayer::Map],
        ),
        Visibility::default(),
        InheritedVisibility::default(),
    )).with_children(|parent| {
        parent.spawn((
            WorldAssetRoot(pedestrian_handle),
            Transform {
                translation: Vec3::new(0.0, -0.9, 0.0),
                rotation: Quat::from_rotation_y(std::f32::consts::PI),
                scale: Vec3::splat(2.0),
            },
            PedestrianVisual,
            Visibility::default(),
            InheritedVisibility::default(),
        ));
    });

}

fn move_pedestrian(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    settings: Res<PedestrianSettings>,
    mut query: Query<(
        &mut Transform,
        &mut LinearVelocity,
        &mut AngularVelocity,
        &mut LockedAxes,
        &mut PedestrianAnimator,
    ), With<Pedestrian>>,
) {
    let Ok((mut transform, mut lin_vel, mut ang_vel, mut locked_axes, mut animator)) = query.single_mut() else {
        return;
    };

    let dt = time.delta_secs();

    // Handle Ragdoll Toggle
    if keyboard.just_pressed(KeyCode::KeyR) {
        animator.is_ragdoll = !animator.is_ragdoll;
        if animator.is_ragdoll {
            *locked_axes = LockedAxes::new();
        } else {
            *locked_axes = LockedAxes::new().lock_rotation_x().lock_rotation_z();
            // Reset upright position
            let euler = transform.rotation.to_euler(EulerRot::YXZ);
            transform.rotation = Quat::from_rotation_y(euler.0);
            transform.translation.y += 0.5; // boost slightly to avoid clipping floor
        }
    }

    // Handle Jump
    if keyboard.just_pressed(KeyCode::Space) && !animator.is_ragdoll && animator.jump_timer.is_none() {
        animator.jump_timer = Some(0.0);
        lin_vel.y = 6.5;
    }

    // Handle Punch
    if (keyboard.just_pressed(KeyCode::KeyF) || mouse_button.just_pressed(MouseButton::Left))
        && !animator.is_ragdoll
        && animator.punch_timer.is_none()
    {
        animator.punch_timer = Some(0.0);
    }

    if animator.is_ragdoll {
        // In ragdoll mode, decay normal linear velocity so physics takes over completely
        let decelerate_rate = 2.0;
        lin_vel.x += (0.0 - lin_vel.x) * decelerate_rate * dt;
        lin_vel.z += (0.0 - lin_vel.z) * decelerate_rate * dt;
        ang_vel.0.y = 0.0;
        return;
    }

    // 1. Rotation (yaw) via A/D or ArrowLeft/ArrowRight or Q/E
    let mut rotation_input = 0.0;
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        rotation_input += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        rotation_input -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyQ) {
        rotation_input += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyE) {
        rotation_input -= 1.0;
    }

    if rotation_input != 0.0 {
        let target_ang_vel = rotation_input * 3.0;
        ang_vel.0.y = target_ang_vel;
    } else {
        ang_vel.0.y = 0.0;
    }

    // 2. Translation (forward/backward) via W/S or ArrowUp/ArrowDown
    let mut move_input = 0.0;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        move_input += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        move_input -= 1.0;
    }

    let speed_multiplier = if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
        2.2
    } else {
        1.0
    };

    if move_input != 0.0 {
        let forward = transform.forward();
        let target_lin_vel = forward * move_input * settings.walk_speed * speed_multiplier;
        
        let accel_rate = 10.0;
        lin_vel.x += (target_lin_vel.x - lin_vel.x) * accel_rate * dt;
        lin_vel.z += (target_lin_vel.z - lin_vel.z) * accel_rate * dt;
    } else {
        let decelerate_rate = 12.0;
        lin_vel.x += (0.0 - lin_vel.x) * decelerate_rate * dt;
        lin_vel.z += (0.0 - lin_vel.z) * decelerate_rate * dt;
    }
}

fn camera_follows_pedestrian(
    mut camera_query: Query<&mut Transform, (With<Camera3d>, Without<Pedestrian>)>,
    pedestrian_query: Query<&Transform, (With<Pedestrian>, Without<Camera3d>)>,
) {
    let Ok(pedestrian_transform) = pedestrian_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let back_dir = *pedestrian_transform.back();
    let target_position = pedestrian_transform.translation + back_dir * 6.0 + Vec3::Y * 3.0;

    camera_transform.translation = target_position;
    camera_transform.look_at(pedestrian_transform.translation + Vec3::Y * 1.0, Vec3::Y);
}

fn animate_pedestrian(
    time: Res<Time>,
    settings: Res<PedestrianSettings>,
    mut pedestrian_query: Query<(&LinearVelocity, &Children, &mut PedestrianAnimator), With<Pedestrian>>,
    mut transform_query: Query<(
        &mut Transform,
        Option<&Name>,
        Option<&BoneBaseRotation>,
        Option<&PedestrianVisual>,
        Option<&LeftLeg>,
        Option<&RightLeg>,
        Option<&LeftArm>,
        Option<&RightArm>,
    )>,
) {
    let dt = time.delta_secs();

    let Ok((lin_vel, children, mut animator)) = pedestrian_query.single_mut() else {
        return;
    };

    // Update action timers
    if let Some(t) = animator.jump_timer {
        let next_t = t + dt;
        if next_t > 0.8 {
            animator.jump_timer = None;
        } else {
            animator.jump_timer = Some(next_t);
        }
    }

    if let Some(t) = animator.punch_timer {
        let next_t = t + dt;
        if next_t > 0.35 {
            animator.punch_timer = None;
        } else {
            animator.punch_timer = Some(next_t);
        }
    }

    let speed = Vec3::new(lin_vel.x, 0.0, lin_vel.z).length();
    
    let target_bob;
    let target_sway;
    let target_lean;
    
    if speed > 0.1 {
        let walk_frequency = 12.0; 
        animator.phase += speed * walk_frequency * dt;
        if animator.phase > std::f32::consts::TAU * 100.0 {
            animator.phase -= std::f32::consts::TAU * 100.0;
        }
        
        target_bob = -0.9 + (animator.phase * 2.0).sin().abs() * 0.08;
        target_sway = animator.phase.sin() * 0.06;
        target_lean = (speed * 0.02).min(0.12);
    } else {
        animator.phase = 0.0;
        target_bob = -0.9;
        target_sway = 0.0;
        target_lean = 0.0;
    }
    
    let lerp_speed = 8.0;
    animator.bob += (target_bob - animator.bob) * lerp_speed * dt;
    animator.sway += (target_sway - animator.sway) * lerp_speed * dt;
    animator.lean += (target_lean - animator.lean) * lerp_speed * dt;

    // 1. Animate Visual Root (bob, sway, lean)
    for child in children.iter() {
        if let Ok((mut child_transform, _, _, Some(_), _, _, _, _)) = transform_query.get_mut(child) {
            if settings.model_type == PedestrianModelType::RiggedGlb {
                if animator.is_ragdoll {
                    child_transform.translation.y = -0.9;
                    child_transform.rotation = Quat::from_rotation_y(std::f32::consts::PI);
                } else {
                    child_transform.translation.y = animator.bob;
                    child_transform.rotation = Quat::from_rotation_y(std::f32::consts::PI) * Quat::from_euler(
                        EulerRot::YXZ,
                        0.0,
                        -animator.lean,
                        animator.sway,
                    );
                }
            } else {
                child_transform.translation.y = animator.bob;
                child_transform.rotation = Quat::from_euler(
                    EulerRot::YXZ,
                    0.0,
                    -animator.lean,
                    animator.sway,
                );
            }
        }
    }

    // 2. Animate Limbs if using ProceduralPuppet
    if settings.model_type == PedestrianModelType::ProceduralPuppet {
        let swing_angle = if speed > 0.1 {
            animator.phase.sin() * settings.swing_amplitude
        } else {
            0.0
        };

        let swing_lerp_speed = 10.0;

        for (mut transform, _, _, _, left_leg, right_leg, left_arm, right_arm) in &mut transform_query {
            if left_leg.is_some() {
                let target_rot = Quat::from_rotation_x(swing_angle);
                transform.rotation = transform.rotation.slerp(target_rot, swing_lerp_speed * dt);
            } else if right_leg.is_some() {
                let target_rot = Quat::from_rotation_x(-swing_angle);
                transform.rotation = transform.rotation.slerp(target_rot, swing_lerp_speed * dt);
            } else if left_arm.is_some() {
                let target_rot = Quat::from_rotation_x(-swing_angle * 0.8);
                transform.rotation = transform.rotation.slerp(target_rot, swing_lerp_speed * dt);
            } else if right_arm.is_some() {
                let target_rot = Quat::from_rotation_x(swing_angle * 0.8);
                transform.rotation = transform.rotation.slerp(target_rot, swing_lerp_speed * dt);
            }
        }
    }

    // 3. Animate Rigged GLB procedurally
    if settings.model_type == PedestrianModelType::RiggedGlb {
        let is_moving = speed > 0.1;

        // Elbow bend increases with speed (running/walking vs idle)
        let base_elbow_bend = 0.25 + (speed * 0.15).min(0.85);
        let left_elbow_bend = (base_elbow_bend + animator.phase.cos() * 0.15).max(0.05);
        let right_elbow_bend = (base_elbow_bend - animator.phase.cos() * 0.15).max(0.05);

        // Knee bend: knees flex when the leg swings forward to clear the ground (peaks at phase = 0, straight at landing)
        let left_knee_flex = if is_moving {
            animator.phase.cos().max(0.0) * settings.swing_amplitude * 1.1
        } else {
            0.0
        };
        let right_knee_flex = if is_moving {
            (-animator.phase.cos()).max(0.0) * settings.swing_amplitude * 1.1
        } else {
            0.0
        };

        // Idle breathing and shoulder sway
        let breathing_phase = time.elapsed_secs() * 1.8;
        let breathing_angle = breathing_phase.sin() * 0.02;
        let idle_arm_sway = breathing_phase.cos() * 0.03;

        let swing_angle = if is_moving {
            animator.phase.sin() * settings.swing_amplitude
        } else {
            0.0
        };

        let swing_lerp_speed = 10.0;

        for (mut transform, name, base_rot, _, _, _, _, _) in &mut transform_query {
            if let (Some(name), Some(base_rot)) = (name, base_rot) {
                let name_str = name.as_str();

                let target_rot = if animator.is_ragdoll {
                    // Ragdoll flailing math
                    let t_val = time.elapsed_secs();
                    match name_str {
                        "bone_20" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, 0.4 * (t_val * 8.0).sin()),
                        "bone_24" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, 0.4 * (t_val * 8.0 + 2.0).sin()),
                        "bone_21" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, -0.8 * (t_val * 10.0).cos().abs() - 0.1),
                        "bone_25" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, -0.8 * (t_val * 10.0 + 2.0).cos().abs() - 0.1),
                        "bone_7" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, 0.9 * (t_val * 7.0).sin()),
                        "bone_14" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, 0.9 * (t_val * 7.0 + 2.0).sin()),
                        "bone_8" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, 0.9 * (t_val * 9.0).cos().abs() + 0.1),
                        "bone_15" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, 0.9 * (t_val * 9.0 + 2.0).cos().abs() + 0.1),
                        "bone_2" | "bone_3" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, 0.15 * (t_val * 5.0).sin()),
                        _ => base_rot.base_rotation,
                    }
                } else {
                    // Normal movements (walk/idle, jump, punch blended)
                    // 1. Calculate walk/idle rotation angles
                    let mut hip_swing_l = swing_angle;
                    let mut hip_swing_r = -swing_angle;
                    let mut knee_flex_l = left_knee_flex;
                    let mut knee_flex_r = right_knee_flex;
                    let mut arm_swing_l = if is_moving { -swing_angle * 0.8 } else { idle_arm_sway };
                    let mut arm_swing_r = if is_moving { swing_angle * 0.8 } else { -idle_arm_sway };
                    let mut elbow_bend_l = left_elbow_bend;
                    let mut elbow_bend_r = right_elbow_bend;
                    let mut spine_lean = if is_moving { (speed * 0.025).min(0.12) } else { breathing_angle };
                    let mut spine_twist = 0.0;
                    let shoulder_twist_l = 0.0;
                    let mut shoulder_twist_r = 0.0;

                    // 2. Blend jump pose (overrides/blends lower body and raises arms)
                    if let Some(jump_t) = animator.jump_timer {
                        let tuck_w = if jump_t < 0.2 {
                            jump_t / 0.2
                        } else if jump_t < 0.5 {
                            1.0
                        } else {
                            ((0.8 - jump_t) / 0.3).max(0.0)
                        };

                        // Blend legs into jump tuck pose (symmetrically swinging both hips forward)
                        hip_swing_l = hip_swing_l * (1.0 - tuck_w) + (0.7 * tuck_w);
                        hip_swing_r = hip_swing_r * (1.0 - tuck_w) + (0.7 * tuck_w);
                        knee_flex_l = knee_flex_l * (1.0 - tuck_w) + (1.2 * tuck_w);
                        knee_flex_r = knee_flex_r * (1.0 - tuck_w) + (1.2 * tuck_w);

                        // Blend arms up in the air
                        arm_swing_l = arm_swing_l * (1.0 - tuck_w) + (-1.2 * tuck_w);
                        arm_swing_r = arm_swing_r * (1.0 - tuck_w) + (-1.2 * tuck_w);
                        elbow_bend_l = elbow_bend_l * (1.0 - tuck_w) + (0.8 * tuck_w);
                        elbow_bend_r = elbow_bend_r * (1.0 - tuck_w) + (0.8 * tuck_w);
                    }

                    // 3. Blend punch pose (affects spine, right arm extension, left arm retraction)
                    if let Some(punch_t) = animator.punch_timer {
                        let punch_w = if punch_t < 0.08 {
                            punch_t / 0.08 // fast extension (80ms)
                        } else {
                            ((0.35 - punch_t) / 0.27).max(0.0) // slower retraction (270ms)
                        };

                        // Right arm extension (very clear, straight forward punch)
                        arm_swing_r = arm_swing_r * (1.0 - punch_w) + (-1.6 * punch_w);
                        shoulder_twist_r = shoulder_twist_r * (1.0 - punch_w) + (0.5 * punch_w);
                        elbow_bend_r = elbow_bend_r * (1.0 - punch_w) + (0.0 * punch_w); // fully straighten

                        // Torso twist and lean forward (dramatic, visible movement)
                        spine_lean += 0.25 * punch_w;
                        spine_twist += -0.6 * punch_w; // twist chest left to push right arm forward
                    }

                    // 4. Construct final bone Quaternions
                    match name_str {
                        "bone_20" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, hip_swing_l),
                        "bone_24" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, hip_swing_r),
                        "bone_21" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, -knee_flex_l),
                        "bone_25" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, -knee_flex_r),
                        "bone_7" => {
                            base_rot.base_rotation
                                * Quat::from_axis_angle(base_rot.local_lateral_axis, arm_swing_l)
                                * Quat::from_axis_angle(base_rot.local_vertical_axis, shoulder_twist_l)
                        }
                        "bone_14" => {
                            base_rot.base_rotation
                                * Quat::from_axis_angle(base_rot.local_lateral_axis, arm_swing_r)
                                * Quat::from_axis_angle(base_rot.local_vertical_axis, shoulder_twist_r)
                        }
                        "bone_8" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, elbow_bend_l),
                        "bone_15" => base_rot.base_rotation * Quat::from_axis_angle(base_rot.local_lateral_axis, elbow_bend_r),
                        "bone_2" | "bone_3" => {
                            base_rot.base_rotation
                                * Quat::from_axis_angle(base_rot.local_lateral_axis, spine_lean)
                                * Quat::from_axis_angle(base_rot.local_vertical_axis, spine_twist)
                        }
                        _ => base_rot.base_rotation,
                    }
                };

                transform.rotation = transform.rotation.slerp(target_rot, swing_lerp_speed * dt);
            }
        }
    }
}

fn init_bone_base_rotations(
    mut commands: Commands,
    bone_query: Query<(Entity, &Name, &Transform, &GlobalTransform), Without<BoneBaseRotation>>,
    pedestrian_query: Query<&GlobalTransform, With<Pedestrian>>,
) {
    let Ok(pedestrian_global) = pedestrian_query.single() else {
        return;
    };
    for (entity, name, transform, bone_global) in &bone_query {
        let name_str = name.as_str();
        if name_str == "leg_joint_L_1"
            || name_str == "leg_joint_R_1"
            || name_str == "Skeleton_arm_joint_L__4_"
            || name_str == "Skeleton_arm_joint_R"
            || name_str == "bone_20"
            || name_str == "bone_24"
            || name_str == "bone_21"
            || name_str == "bone_25"
            || name_str == "bone_7"
            || name_str == "bone_14"
            || name_str == "bone_8"
            || name_str == "bone_15"
            || name_str == "bone_2"
            || name_str == "bone_3"
        {
            let pedestrian_transform = pedestrian_global.compute_transform();
            let char_right = *pedestrian_transform.right();
            let char_up = *pedestrian_transform.up();
            let bone_transform = bone_global.compute_transform();
            let local_lateral_axis = bone_transform.rotation.inverse().mul_vec3(char_right);
            let local_vertical_axis = bone_transform.rotation.inverse().mul_vec3(char_up);

            commands.entity(entity).insert(BoneBaseRotation {
                base_rotation: transform.rotation,
                local_lateral_axis,
                local_vertical_axis,
            });
        }
    }
}

fn draw_pedestrian_ui(
    mut contexts: EguiContexts,
    mut settings: ResMut<PedestrianSettings>,
    mut pedestrian_query: Query<(
        Entity,
        Option<&Children>,
        &mut PedestrianAnimator,
        &mut LinearVelocity,
        &mut LockedAxes,
        &mut Transform,
    ), With<Pedestrian>>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new("Pedestrian Animation Settings")
        .default_pos(egui::pos2(12.0, 50.0))
        .show(ctx, |ui| {
            ui.label("Choose how the pedestrian moves:");
            
            let old_type = settings.model_type;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut settings.model_type, PedestrianModelType::GlbModel, "Static GLB Model");
                ui.selectable_value(&mut settings.model_type, PedestrianModelType::ProceduralPuppet, "Jointed Puppet");
                ui.selectable_value(&mut settings.model_type, PedestrianModelType::RiggedGlb, "Rigged GLB Model");
            });

            if settings.model_type != old_type {
                for (entity, children, _, _, _, _) in &mut pedestrian_query {
                    if let Some(children) = children {
                        for child in children.iter() {
                            commands.entity(child).despawn();
                        }
                    }
                    
                    match settings.model_type {
                        PedestrianModelType::GlbModel => {
                            let base_url = demo_resolution_selector_web_bevy::config::DATA_BASE_URL.trim_end_matches('/');
                            let pedestrian_url = format!("{}/3d_data/3d_slop_models_clean/pedestrian/armin-1b.glb", base_url);
                            let pedestrian_handle = asset_server.load(GltfAssetLabel::Scene(0).from_asset(pedestrian_url));
                            
                            commands.entity(entity).with_children(|parent| {
                                parent.spawn((
                                    WorldAssetRoot(pedestrian_handle),
                                    Transform::from_xyz(0.0, -0.9, 0.0),
                                    PedestrianVisual,
                                    Visibility::default(),
                                    InheritedVisibility::default(),
                                ));
                            });
                        }
                        PedestrianModelType::RiggedGlb => {
                            let base_url = demo_resolution_selector_web_bevy::config::DATA_BASE_URL.trim_end_matches('/');
                            let pedestrian_url = format!("{}/3d_data/3d_slop_models_clean/pedestrian/armin_rigged.glb", base_url);
                            let pedestrian_handle = asset_server.load(GltfAssetLabel::Scene(0).from_asset(pedestrian_url));
                            
                            commands.entity(entity).with_children(|parent| {
                                parent.spawn((
                                    WorldAssetRoot(pedestrian_handle),
                                    Transform {
                                        translation: Vec3::new(0.0, -0.9, 0.0),
                                        rotation: Quat::from_rotation_y(std::f32::consts::PI),
                                        scale: Vec3::splat(2.0),
                                    },
                                    PedestrianVisual,
                                    Visibility::default(),
                                    InheritedVisibility::default(),
                                ));
                            });
                        }
                        PedestrianModelType::ProceduralPuppet => {
                            commands.entity(entity).with_children(|parent| {
                                parent.spawn((
                                    Transform::from_xyz(0.0, -0.9, 0.0),
                                    PedestrianVisual,
                                    Visibility::default(),
                                    InheritedVisibility::default(),
                                )).with_children(|vis_root| {
                                    let torso_id = vis_root.spawn((
                                        Mesh3d(meshes.add(Cuboid::new(0.38, 0.65, 0.22))),
                                        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.6, 0.86))),
                                        Transform::from_xyz(0.0, 0.95, 0.0),
                                        Torso,
                                    )).id();
                                    
                                    vis_root.commands().entity(torso_id).with_children(|torso| {
                                        torso.spawn((
                                            Mesh3d(meshes.add(Sphere::new(0.18))),
                                            MeshMaterial3d(materials.add(Color::srgb(0.95, 0.80, 0.69))),
                                            Transform::from_xyz(0.0, 0.48, 0.0),
                                            Head,
                                        ));
                                        
                                        torso.spawn((
                                            Transform::from_xyz(-0.25, 0.22, 0.0),
                                            LeftArm,
                                            Visibility::default(),
                                            InheritedVisibility::default(),
                                        )).with_children(|arm| {
                                            arm.spawn((
                                                Mesh3d(meshes.add(Capsule3d::new(0.06, 0.35))),
                                                MeshMaterial3d(materials.add(Color::srgb(0.2, 0.6, 0.86))),
                                                Transform::from_xyz(0.0, -0.175, 0.0),
                                            ));
                                        });
                                        
                                        torso.spawn((
                                            Transform::from_xyz(0.25, 0.22, 0.0),
                                            RightArm,
                                            Visibility::default(),
                                            InheritedVisibility::default(),
                                        )).with_children(|arm| {
                                            arm.spawn((
                                                Mesh3d(meshes.add(Capsule3d::new(0.06, 0.35))),
                                                MeshMaterial3d(materials.add(Color::srgb(0.2, 0.6, 0.86))),
                                                Transform::from_xyz(0.0, -0.175, 0.0),
                                            ));
                                        });
                                        
                                        torso.spawn((
                                            Transform::from_xyz(-0.11, -0.32, 0.0),
                                            LeftLeg,
                                            Visibility::default(),
                                            InheritedVisibility::default(),
                                        )).with_children(|leg| {
                                            leg.spawn((
                                                Mesh3d(meshes.add(Capsule3d::new(0.08, 0.45))),
                                                MeshMaterial3d(materials.add(Color::srgb(0.15, 0.15, 0.15))),
                                                Transform::from_xyz(0.0, -0.225, 0.0),
                                            ));
                                        });
                                        
                                        torso.spawn((
                                            Transform::from_xyz(0.11, -0.32, 0.0),
                                            RightLeg,
                                            Visibility::default(),
                                            InheritedVisibility::default(),
                                        )).with_children(|leg| {
                                            leg.spawn((
                                                Mesh3d(meshes.add(Capsule3d::new(0.08, 0.45))),
                                                MeshMaterial3d(materials.add(Color::srgb(0.15, 0.15, 0.15))),
                                                Transform::from_xyz(0.0, -0.225, 0.0),
                                            ));
                                        });
                                    });
                                });
                            });
                        }
                    }
                }
            }

            ui.separator();
            ui.label("Actions:");
            ui.horizontal(|ui| {
                let mut p_query = pedestrian_query;
                if let Ok((_, _, mut animator, mut lin_vel, mut locked_axes, mut transform)) = p_query.single_mut() {
                    let is_ragdoll = animator.is_ragdoll;
                    if ui.button(if is_ragdoll { "Stand Up" } else { "Ragdoll (R)" }).clicked() {
                        animator.is_ragdoll = !animator.is_ragdoll;
                        if animator.is_ragdoll {
                            *locked_axes = LockedAxes::new();
                        } else {
                            *locked_axes = LockedAxes::new().lock_rotation_x().lock_rotation_z();
                            let euler = transform.rotation.to_euler(EulerRot::YXZ);
                            transform.rotation = Quat::from_rotation_y(euler.0);
                            transform.translation.y += 0.5;
                        }
                    }

                    if ui.add_enabled(!is_ragdoll && animator.jump_timer.is_none(), egui::Button::new("Jump (Space)")).clicked() {
                        animator.jump_timer = Some(0.0);
                        lin_vel.y = 6.5;
                    }

                    if ui.add_enabled(!is_ragdoll && animator.punch_timer.is_none(), egui::Button::new("Punch (F)")).clicked() {
                        animator.punch_timer = Some(0.0);
                    }
                }
            });

            ui.separator();
            ui.add(egui::Slider::new(&mut settings.swing_amplitude, 0.1..=1.2).text("Swing Angle (Rad)"));
            ui.add(egui::Slider::new(&mut settings.walk_speed, 1.0..=6.0).text("Walk Speed"));
            
            ui.allocate_space(egui::Vec2::new(1.0, 5.0));
            ui.label("Controls:\n- WASD / Arrows to Walk/Steer\n- Left Shift to Run\n- Space to Jump\n- F / Left Click to Punch\n- R to Toggle Ragdoll");
        });
}
