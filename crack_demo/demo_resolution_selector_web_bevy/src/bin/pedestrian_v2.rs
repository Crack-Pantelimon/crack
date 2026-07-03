use avian3d::prelude::{
    Collider, CollisionLayers, Restitution, RigidBody, PhysicsPlugins,
    SpatialQuery, SpatialQueryFilter,
};
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use bevy::{
    asset::{Asset, AssetLoader, LoadContext, RenderAssetUsages, io::Reader},
    ecs::relationship::Relationship,
    prelude::*,
    reflect::TypePath,
    render::{
        RenderPlugin,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        settings::{Backends, WgpuSettings},
    },
    window::WindowResolution,
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use demo_resolution_selector_web_bevy::plugins::{
    cars_driving::driving_plugin::GamePhysicsLayer,
    game_freecam::camera_controls::{ActiveCameraAnimation, CameraControlsPlugin},
    map_plugin::{BBox, MapTree},
    states::GameControlState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BoneLabel {
    Head,
    Neck,
    Spine,
    Midgroin,
    LeftShoulder,
    RightShoulder,
    LeftArm,
    RightArm,
    LeftHand,
    RightHand,
    LeftLeg,
    RightLeg,
    LeftFoot,
    RightFoot,
}

impl BoneLabel {
    fn color(&self) -> Color {
        match self {
            BoneLabel::Head | BoneLabel::Neck => Color::srgb(1.0, 0.4, 0.7), // Pink
            BoneLabel::Spine => Color::srgb(0.0, 0.0, 0.5), // Dark Blue
            BoneLabel::Midgroin => Color::srgb(1.0, 1.0, 0.0), // Yellow
            BoneLabel::LeftShoulder => Color::srgb(0.5, 0.8, 1.0), // Light Blue
            BoneLabel::RightShoulder => Color::srgb(1.0, 0.7, 0.85), // Light Pink
            BoneLabel::LeftArm => Color::srgb(0.6, 0.2, 0.8), // Purple
            BoneLabel::RightArm => Color::srgb(1.0, 0.6, 0.0), // Orange
            BoneLabel::LeftHand => Color::srgb(0.6, 0.6, 0.0), // Dark Yellow
            BoneLabel::RightHand => Color::srgb(1.0, 1.0, 0.5), // Light Yellow
            BoneLabel::LeftLeg => Color::srgb(1.0, 0.2, 0.2), // Red
            BoneLabel::RightLeg => Color::srgb(0.2, 1.0, 0.2), // Green
            BoneLabel::LeftFoot => Color::srgb(0.8, 0.0, 0.8), // Dark Purple/Magenta
            BoneLabel::RightFoot => Color::srgb(0.0, 0.8, 0.8), // Light Purple/Teal
        }
    }
}

#[derive(Component)]
struct ModelRoot {
    index: usize,
    name: String,
    size: Vec3,
}

#[derive(Component)]
struct NeedAlignment;

#[derive(Component)]
struct PedestrianSkeleton {
    joint_labels: std::collections::HashMap<Entity, BoneLabel>,
}

#[derive(Component)]
struct PedestrianGltf {
    handle: Handle<bevy::gltf::Gltf>,
}

#[derive(Resource)]
struct AnimationSettings {
    available_animations: Vec<String>,
    selected_animation: Option<String>,
    speed: f32,
    graph_handle: Handle<AnimationGraph>,
    animation_nodes: std::collections::HashMap<String, AnimationNodeIndex>,
}

impl Default for AnimationSettings {
    fn default() -> Self {
        Self {
            available_animations: Vec::new(),
            selected_animation: None,
            speed: 1.0,
            graph_handle: Handle::default(),
            animation_nodes: std::collections::HashMap::new(),
        }
    }
}

#[derive(Resource, Default)]
struct FrameCounter(u64);

#[derive(Resource)]
struct SkeletonVisuals {
    show_skeleton: bool,
}

impl Default for SkeletonVisuals {
    fn default() -> Self {
        Self { show_skeleton: false }
    }
}

#[derive(Event, Clone)]
struct SpawnPedestrianRequest {
    position: Vec3,
    model_name: String,
    handle: Handle<WorldAsset>,
    gltf_handle: Handle<bevy::gltf::Gltf>,
    model_index: usize,
}

#[derive(Resource)]
struct ManifestLoader {
    handle: Handle<TextAsset>,
    glb_handles: Option<Vec<(String, Handle<WorldAsset>, Handle<bevy::gltf::Gltf>)>>,
    spawned: bool,
}

#[derive(Resource, Default)]
struct SelectedModel {
    entity: Option<Entity>,
}

#[derive(Resource, Default)]
struct HoveredModel {
    entity: Option<Entity>,
}



#[derive(Asset, TypePath, Debug, Clone)]
pub struct TextAsset {
    pub text: String,
}

#[derive(Default, TypePath)]
pub struct TextAssetLoader;

impl AssetLoader for TextAssetLoader {
    type Asset = TextAsset;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(TextAsset { text })
    }

    fn extensions(&self) -> &[&str] {
        &["txt"]
    }
}

struct JointData {
    entity: Entity,
    _name: String,
    pos: Vec3,
    parent: Option<Entity>,
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
                        title: "Pedestrian V2 Viewer".into(),
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
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .init_state::<GameControlState>()
        .insert_resource(MapTree {
            parsed: true,
            bbox: BBox {
                min: Vec3::new(-1000.0, -100.0, -1000.0),
                max: Vec3::new(1000.0, 100.0, 1000.0),
            },
            ..default()
        })
        .add_plugins(CameraControlsPlugin)
        .init_asset::<TextAsset>()
        .init_asset_loader::<TextAssetLoader>()
        .init_resource::<SelectedModel>()
        .init_resource::<HoveredModel>()
        .init_resource::<AnimationSettings>()
        .init_resource::<FrameCounter>()
        .init_resource::<SkeletonVisuals>()
        .add_observer(spawn_pedestrian_observer)
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (
                load_manifest_system,
                init_pedestrians_system,
                setup_animation_players_system,
                play_animations_system,
                draw_skeletons_system,
                picker_system,
                draw_hovered_bbox_system,
            ),
        )
        .add_systems(EguiPrimaryContextPass, draw_gui_system)
        .run();
}


fn spawn_pedestrian_observer(
    trigger: On<SpawnPedestrianRequest>,
    mut commands: Commands,
) {
    let req = trigger.event();
    commands.spawn((
        Transform::from_translation(req.position),
        Visibility::default(),
        InheritedVisibility::default(),
        ModelRoot {
            index: req.model_index,
            name: req.model_name.clone(),
            size: Vec3::ZERO,
        },
        PedestrianGltf {
            handle: req.gltf_handle.clone(),
        },
        NeedAlignment,
    ))
    .with_children(|parent| {
        parent.spawn((
            WorldAssetRoot(req.handle.clone()),
            Transform::IDENTITY,
            Visibility::default(),
            InheritedVisibility::default(),
        ));
    });
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
    asset_server: Res<AssetServer>,
    mut gizmo_store: ResMut<GizmoConfigStore>,
) {
    let (config, _) = gizmo_store.config_mut::<DefaultGizmoConfigGroup>();
    config.depth_bias = -1.0;

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

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-10.0, 2.0, -15.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        AmbientLight {
            color: Color::srgb(0.8, 0.85, 1.0),
            brightness: 1000.0,
            ..default()
        },
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 10000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(200.0, 400.0, 200.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let base_url = demo_resolution_selector_web_bevy::config::DATA_BASE_URL.trim_end_matches('/');
    let manifest_url = format!("{}/3d_data/pedestrian_3d_gen/manifest.txt", base_url);
    let handle = asset_server.load::<TextAsset>(manifest_url);

    commands.insert_resource(ManifestLoader {
        handle,
        glb_handles: None,
        spawned: false,
    });
}

fn load_manifest_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut loader: ResMut<ManifestLoader>,
    text_assets: Res<Assets<TextAsset>>,
    world_assets: Res<Assets<WorldAsset>>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    if loader.glb_handles.is_none() {
        if let Some(text_asset) = text_assets.get(&loader.handle) {
            let base_url =
                demo_resolution_selector_web_bevy::config::DATA_BASE_URL.trim_end_matches('/');
            let mut handles = Vec::new();
            for line in text_asset.text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let glb_url = format!("{}/3d_data/pedestrian_3d_gen/{}", base_url, line);
                let scene_url = GltfAssetLabel::Scene(0).from_asset(glb_url.clone());
                let handle = asset_server.load::<WorldAsset>(scene_url);
                let gltf_handle = asset_server.load::<bevy::gltf::Gltf>(glb_url);
                handles.push((line.to_string(), handle, gltf_handle));
            }
            info!(
                "Parsed manifest. Loading {} GLB world assets in parallel...",
                handles.len()
            );
            loader.glb_handles = Some(handles);
        }
    } else if !loader.spawned {
        let handles = loader.glb_handles.as_ref().unwrap();
        let mut all_loaded = true;
        for (_, handle, gltf_handle) in handles {
            if world_assets.get(handle).is_none() || gltf_assets.get(gltf_handle).is_none() {
                all_loaded = false;
                break;
            }
        }
        if all_loaded {
            info!("All GLB scenes loaded! Triggering spawn events...");

            // Collect all animations and populate AnimationSettings resource
            let mut animation_names = std::collections::BTreeSet::new();
            let mut clips = Vec::new();
            let mut clip_to_name = std::collections::HashMap::new();

            for (_, _, gltf_handle) in handles {
                if let Some(gltf) = gltf_assets.get(gltf_handle) {
                    for (name, clip_handle) in &gltf.named_animations {
                        let name_str = name.to_string();
                        if !animation_names.contains(&name_str) {
                            animation_names.insert(name_str.clone());
                            clips.push(clip_handle.clone());
                            clip_to_name.insert(clip_handle.id(), name_str);
                        }
                    }
                }
            }

            let available_animations: Vec<String> = animation_names.into_iter().collect();
            let selected_animation = if available_animations.contains(&"A_TPose".to_string()) {
                Some("A_TPose".to_string())
            } else if !available_animations.is_empty() {
                Some(available_animations[0].clone())
            } else {
                None
            };

            let (graph, node_indices) = AnimationGraph::from_clips(clips.clone());
            let graph_handle = graphs.add(graph);

            let mut animation_nodes = std::collections::HashMap::new();
            for (idx, clip_handle) in clips.iter().enumerate() {
                if let Some(name) = clip_to_name.get(&clip_handle.id()) {
                    animation_nodes.insert(name.clone(), node_indices[idx]);
                }
            }

            commands.insert_resource(AnimationSettings {
                available_animations,
                selected_animation,
                speed: 1.0,
                graph_handle,
                animation_nodes,
            });

            let count = handles.len();
            let cols = (count as f32).sqrt().ceil() as usize;

            for (idx, (line, handle, gltf_handle)) in handles.iter().enumerate() {
                let col = idx % cols;
                let row = idx / cols;

                const GRID_SIZE: f32 = 1.6;
                let x = (col as f32 - (cols - 1) as f32 / 2.0) * GRID_SIZE;
                let z = (row as f32 - (((count as f32 / cols as f32).ceil() - 1.0) / 2.0)) * GRID_SIZE;
                let y = 0.0;

                let model_name = line.split('/').last().unwrap_or(line).replace(".glb", "");

                commands.trigger(SpawnPedestrianRequest {
                    position: Vec3::new(x, y, z),
                    model_name: model_name.clone(),
                    handle: handle.clone(),
                    gltf_handle: gltf_handle.clone(),
                    model_index: idx,
                });
            }

            loader.spawned = true;
        }
    }
}

fn get_mesh_descendants(
    entity: Entity,
    children_query: &Query<&Children>,
    mesh_query: &Query<&Mesh3d>,
    results: &mut Vec<(Entity, Handle<Mesh>)>,
) {
    if let Ok(mesh3d) = mesh_query.get(entity) {
        results.push((entity, mesh3d.0.clone()));
    }
    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            get_mesh_descendants(child, children_query, mesh_query, results);
        }
    }
}

fn init_pedestrians_system(
    mut commands: Commands,
    query: Query<(Entity, &NeedAlignment, &Children)>,
    children_query: Query<&Children>,
    mesh_query: Query<&Mesh3d>,
    global_transform_query: Query<&GlobalTransform>,
    mut model_root_query: Query<&mut ModelRoot>,
    parent_query: Query<&ChildOf>,
    name_query: Query<&Name>,
    meshes: Res<Assets<Mesh>>,
) {
    for (root_entity, _need_align, children) in query.iter() {
        let mut mesh_entities = Vec::new();

        for child in children.iter() {
            get_mesh_descendants(child, &children_query, &mesh_query, &mut mesh_entities);
        }

        if mesh_entities.is_empty() {
            continue;
        }

        let mut all_meshes_loaded = true;
        for (_, mesh_handle) in &mesh_entities {
            if meshes.get(mesh_handle).is_none() {
                all_meshes_loaded = false;
                break;
            }
        }

        if !all_meshes_loaded {
            continue;
        }

        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        let mut found_vertices = false;

        let Ok(root_gt) = global_transform_query.get(root_entity) else {
            continue;
        };
        let root_inv = root_gt.to_matrix().inverse();

        for (ent, mesh_handle) in &mesh_entities {
            let Ok(mesh_gt) = global_transform_query.get(*ent) else {
                continue;
            };

            if let Some(mesh) = meshes.get(mesh_handle) {
                if let Some(collider) = Collider::trimesh_from_mesh(mesh) {
                    commands.entity(*ent).insert(collider);
                }
                if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                {
                    for pos in positions {
                        let vertex_local = Vec3::from(*pos);
                        let world_pos = mesh_gt.transform_point(vertex_local);
                        let vertex_rel_root = root_inv.transform_point3(world_pos);
                        min = min.min(vertex_rel_root);
                        max = max.max(vertex_rel_root);
                        found_vertices = true;
                    }
                }
            }
        }

        if !found_vertices {
            continue;
        }

        let size = max - min;

        if let Ok(mut root) = model_root_query.get_mut(root_entity) {
            root.size = size;
        }

        let mut skeleton_root = root_entity;
        let mut queue = vec![root_entity];
        while let Some(ent) = queue.pop() {
            if let Ok(name) = name_query.get(ent) {
                if name.as_str() == "Armature" {
                    skeleton_root = ent;
                    break;
                }
            }
            if let Ok(children) = children_query.get(ent) {
                for child in children.iter() {
                    queue.push(child);
                }
            }
        }

        // Perform skeleton classification
        let mut nodes_raw = Vec::new();
        traverse_hierarchy_raw(
            skeleton_root,
            &children_query,
            &name_query,
            &global_transform_query,
            &mut nodes_raw,
        );

        let mut joints = Vec::new();
        for (ent, name, world_pos) in &nodes_raw {
            let rel_pos = root_inv.transform_point3(*world_pos);
            let parent_ent = parent_query.get(*ent).ok().map(|p| p.get());
            joints.push(JointData {
                entity: *ent,
                _name: name.clone(),
                pos: rel_pos,
                parent: parent_ent,
            });
        }
        
        let (classification, _, _, _, _, _, _) = classify_skeleton(root_entity, &joints);
        
        commands.entity(root_entity).insert(PedestrianSkeleton {
            joint_labels: classification,
        });

        commands.entity(root_entity).remove::<NeedAlignment>();
    }
}

fn traverse_hierarchy_raw(
    entity: Entity,
    children_query: &Query<&Children>,
    name_query: &Query<&Name>,
    transform_query: &Query<&GlobalTransform>,
    nodes: &mut Vec<(Entity, String, Vec3)>,
) {
    let name_str = if let Ok(name) = name_query.get(entity) {
        name.to_string()
    } else {
        format!("Entity_{}", entity.index())
    };

    let pos = if let Ok(gt) = transform_query.get(entity) {
        gt.translation()
    } else {
        Vec3::ZERO
    };

    let is_valid_joint = name_str.starts_with("bone_")
        || name_str == "Armature";

    if is_valid_joint {
        nodes.push((entity, name_str, pos));
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            traverse_hierarchy_raw(child, children_query, name_query, transform_query, nodes);
        }
    }
}

fn classify_skeleton(
    root_entity: Entity,
    joints: &[JointData],
) -> (
    std::collections::HashMap<Entity, BoneLabel>,
    Option<Entity>, // left shoulder
    Option<Entity>, // left elbow
    Option<Entity>, // left wrist
    Option<Entity>, // right shoulder
    Option<Entity>, // right elbow
    Option<Entity>, // right wrist
) {
    let mut labels = std::collections::HashMap::new();
    if joints.is_empty() {
        return (labels, None, None, None, None, None, None);
    }

    let coccis_entity = joints[0].entity;
    labels.insert(coccis_entity, BoneLabel::Midgroin);

    let mut head_idx = 0;
    let mut max_y = joints[0].pos.y;
    for (idx, joint) in joints.iter().enumerate() {
        if joint.pos.y > max_y {
            max_y = joint.pos.y;
            head_idx = idx;
        }
    }
    let head_entity = joints[head_idx].entity;
    labels.insert(head_entity, BoneLabel::Head);

    let mut spine_path = Vec::new();
    let mut current = head_entity;
    while current != coccis_entity && current != root_entity {
        spine_path.push(current);
        if let Some(parent) = find_parent_of(current, joints) {
            current = parent;
        } else {
            break;
        }
    }
    spine_path.push(coccis_entity);

    let mut neck_entity = None;
    if let Some(parent) = joints[head_idx].parent {
        if parent != root_entity && parent != coccis_entity {
            labels.insert(parent, BoneLabel::Neck);
            neck_entity = Some(parent);
        }
    }
    for &node in &spine_path {
        if node != head_entity && Some(node) != neck_entity && node != coccis_entity {
            labels.insert(node, BoneLabel::Spine);
        }
    }

    let mut joints_min_x = f32::MAX;
    let mut joints_max_x = -f32::MAX;
    for joint in joints {
        joints_min_x = joints_min_x.min(joint.pos.x);
        joints_max_x = joints_max_x.max(joint.pos.x);
    }
    let joints_center_x = (joints_min_x + joints_max_x) / 2.0;

    let is_left = |pos: Vec3| pos.x > joints_center_x;
    let is_right = |pos: Vec3| pos.x < joints_center_x;

    let mut left_heel_entity = None;
    let mut left_min_y = f32::MAX;
    let mut right_heel_entity = None;
    let mut right_min_y = f32::MAX;

    for joint in joints {
        if is_left(joint.pos) && joint.pos.y < left_min_y {
            left_min_y = joint.pos.y;
            left_heel_entity = Some(joint.entity);
        }
        if is_right(joint.pos) && joint.pos.y < right_min_y {
            right_min_y = joint.pos.y;
            right_heel_entity = Some(joint.entity);
        }
    }

    let mut left_hand_tip_entity = None;
    let mut left_max_dist = -f32::MAX;
    let mut right_hand_tip_entity = None;
    let mut right_max_dist = -f32::MAX;

    for joint in joints {
        let dist = (joint.pos.x - joints_center_x).abs();
        if is_left(joint.pos) && dist > left_max_dist {
            left_max_dist = dist;
            left_hand_tip_entity = Some(joint.entity);
        }
        if is_right(joint.pos) && dist > right_max_dist {
            right_max_dist = dist;
            right_hand_tip_entity = Some(joint.entity);
        }
    }

    let left_arm_info = classify_limb_path(left_hand_tip_entity, &spine_path, root_entity, joints, &mut labels, BoneLabel::LeftArm, BoneLabel::LeftShoulder, BoneLabel::LeftHand);
    let right_arm_info = classify_limb_path(right_hand_tip_entity, &spine_path, root_entity, joints, &mut labels, BoneLabel::RightArm, BoneLabel::RightShoulder, BoneLabel::RightHand);

    let _left_leg_info = classify_limb_path(left_heel_entity, &spine_path, root_entity, joints, &mut labels, BoneLabel::LeftLeg, BoneLabel::Midgroin, BoneLabel::LeftFoot);
    let _right_leg_info = classify_limb_path(right_heel_entity, &spine_path, root_entity, joints, &mut labels, BoneLabel::RightLeg, BoneLabel::Midgroin, BoneLabel::RightFoot);

    let (left_shoulder, left_elbow, left_wrist) = match left_arm_info {
        Some((s, e, w)) => (Some(s), Some(e), Some(w)),
        None => (None, None, None),
    };

    let (right_shoulder, right_elbow, right_wrist) = match right_arm_info {
        Some((s, e, w)) => (Some(s), Some(e), Some(w)),
        None => (None, None, None),
    };

    (labels, left_shoulder, left_elbow, left_wrist, right_shoulder, right_elbow, right_wrist)
}

fn find_parent_of(entity: Entity, joints: &[JointData]) -> Option<Entity> {
    for joint in joints {
        if joint.entity == entity {
            return joint.parent;
        }
    }
    None
}

fn find_pos_of(entity: Entity, joints: &[JointData]) -> Option<Vec3> {
    for joint in joints {
        if joint.entity == entity {
            return Some(joint.pos);
        }
    }
    None
}

fn classify_limb_path(
    tip_entity: Option<Entity>,
    spine_path: &[Entity],
    root_entity: Entity,
    joints: &[JointData],
    labels: &mut std::collections::HashMap<Entity, BoneLabel>,
    limb_main_label: BoneLabel,
    limb_shoulder_label: BoneLabel,
    limb_hand_label: BoneLabel,
) -> Option<(Entity, Entity, Entity)> {
    let tip = tip_entity?;
    
    let mut path = Vec::new();
    let mut current = tip;
    while !spine_path.contains(&current) && current != root_entity {
        path.push(current);
        if let Some(parent) = find_parent_of(current, joints) {
            current = parent;
        } else {
            break;
        }
    }
    
    if path.len() < 2 {
        labels.insert(tip, limb_hand_label);
        return None;
    }
    
    let mut segments = Vec::new();
    for i in 0..path.len() {
        let node = path[i];
        let parent = if i == path.len() - 1 {
            find_parent_of(node, joints)
        } else {
            Some(path[i + 1])
        };
        if let Some(p) = parent {
            let pos_node = find_pos_of(node, joints).unwrap_or(Vec3::ZERO);
            let pos_parent = find_pos_of(p, joints).unwrap_or(Vec3::ZERO);
            let length = pos_node.distance(pos_parent);
            segments.push((i, node, p, length));
        }
    }
    
    segments.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    
    let (idx1, idx2) = if segments.len() >= 2 {
        let mut idxs = [segments[0].0, segments[1].0];
        idxs.sort();
        (idxs[0], idxs[1])
    } else {
        (0, path.len() - 1)
    };
    
    let wrist_node = path[idx1];
    let elbow_node = path[idx2];
    let shoulder_node = if idx2 == path.len() - 1 {
        find_parent_of(path[idx2], joints).unwrap_or(path[idx2])
    } else {
        path[idx2 + 1]
    };
    
    for i in 0..path.len() {
        let node = path[i];
        if i < idx1 {
            labels.insert(node, limb_hand_label);
        } else if i >= idx1 && i <= idx2 {
            labels.insert(node, limb_main_label);
        } else {
            labels.insert(node, limb_shoulder_label);
        }
    }
    
    Some((shoulder_node, elbow_node, wrist_node))
}

#[allow(dead_code)]
fn print_classification_results(
    model_name: &str,
    joints: &[JointData],
    labels: &std::collections::HashMap<Entity, BoneLabel>,
) {
    let mut joints_min_x = f32::MAX;
    let mut joints_max_x = -f32::MAX;
    for joint in joints {
        joints_min_x = joints_min_x.min(joint.pos.x);
        joints_max_x = joints_max_x.max(joint.pos.x);
    }
    let joints_center_x = (joints_min_x + joints_max_x) / 2.0;

    println!("Character Bone Identification Results for {}:", model_name);
    
    let print_joint = |label_name: &str, target_label: BoneLabel| {
        let mut found = None;
        for (ent, label) in labels {
            if *label == target_label {
                for joint in joints {
                    if joint.entity == *ent {
                        found = Some(joint.pos);
                        break;
                    }
                }
            }
        }
        if let Some(pos) = found {
            println!("  {}: Identified at {:?}", label_name, pos);
        } else {
            println!("  {}: Not Identified", label_name);
        }
    };
    
    print_joint("Head", BoneLabel::Head);
    print_joint("Neck", BoneLabel::Neck);
    print_joint("Midgroin", BoneLabel::Midgroin);
    
    let spine_count = labels.values().filter(|&l| *l == BoneLabel::Spine).count();
    println!("  Spine: Identified, count: {}", spine_count);

    if joints.is_empty() { return; }
    let coccis_entity = joints[0].entity;
    let mut head_idx = 0;
    let mut max_y = joints[0].pos.y;
    for (idx, joint) in joints.iter().enumerate() {
        if joint.pos.y > max_y {
            max_y = joint.pos.y;
            head_idx = idx;
        }
    }
    let head_entity = joints[head_idx].entity;

    let mut spine_path = Vec::new();
    let mut current = head_entity;
    let dummy_root = Entity::from_raw_u32(999999).unwrap();
    while current != coccis_entity && current != dummy_root {
        spine_path.push(current);
        if let Some(parent) = find_parent_of(current, joints) {
            current = parent;
        } else {
            break;
        }
    }
    spine_path.push(coccis_entity);

    let is_left = |pos: Vec3| pos.x > joints_center_x;
    let is_right = |pos: Vec3| pos.x < joints_center_x;

    let mut left_heel_entity = None;
    let mut left_min_y = f32::MAX;
    let mut right_heel_entity = None;
    let mut right_min_y = f32::MAX;
    for joint in joints {
        if is_left(joint.pos) && joint.pos.y < left_min_y {
            left_min_y = joint.pos.y;
            left_heel_entity = Some(joint.entity);
        }
        if is_right(joint.pos) && joint.pos.y < right_min_y {
            right_min_y = joint.pos.y;
            right_heel_entity = Some(joint.entity);
        }
    }

    let mut left_hand_tip_entity = None;
    let mut left_max_dist = -f32::MAX;
    let mut right_hand_tip_entity = None;
    let mut right_max_dist = -f32::MAX;
    for joint in joints {
        let dist = (joint.pos.x - joints_center_x).abs();
        if is_left(joint.pos) && dist > left_max_dist {
            left_max_dist = dist;
            left_hand_tip_entity = Some(joint.entity);
        }
        if is_right(joint.pos) && dist > right_max_dist {
            right_max_dist = dist;
            right_hand_tip_entity = Some(joint.entity);
        }
    }

    let mut temp_labels = std::collections::HashMap::new();
    let left_arm_info = classify_limb_path(left_hand_tip_entity, &spine_path, dummy_root, joints, &mut temp_labels, BoneLabel::LeftArm, BoneLabel::LeftShoulder, BoneLabel::LeftHand);
    let right_arm_info = classify_limb_path(right_hand_tip_entity, &spine_path, dummy_root, joints, &mut temp_labels, BoneLabel::RightArm, BoneLabel::RightShoulder, BoneLabel::RightHand);
    let left_leg_info = classify_limb_path(left_heel_entity, &spine_path, dummy_root, joints, &mut temp_labels, BoneLabel::LeftLeg, BoneLabel::Midgroin, BoneLabel::LeftFoot);
    let right_leg_info = classify_limb_path(right_heel_entity, &spine_path, dummy_root, joints, &mut temp_labels, BoneLabel::RightLeg, BoneLabel::Midgroin, BoneLabel::RightFoot);

    let print_limb = |side_prefix: &str, info: Option<(Entity, Entity, Entity)>, shoulder_lbl: &str, elbow_lbl: &str, wrist_lbl: &str| {
        if let Some((s, e, w)) = info {
            let pos_s = find_pos_of(s, joints).unwrap_or(Vec3::ZERO);
            let pos_e = find_pos_of(e, joints).unwrap_or(Vec3::ZERO);
            let pos_w = find_pos_of(w, joints).unwrap_or(Vec3::ZERO);
            println!("  {} {}: Identified at {:?}", side_prefix, shoulder_lbl, pos_s);
            println!("  {} {}: Identified at {:?}", side_prefix, elbow_lbl, pos_e);
            println!("  {} {}: Identified at {:?}", side_prefix, wrist_lbl, pos_w);
        } else {
            println!("  {} {}: Not Identified", side_prefix, shoulder_lbl);
            println!("  {} {}: Not Identified", side_prefix, elbow_lbl);
            println!("  {} {}: Not Identified", side_prefix, wrist_lbl);
        }
    };

    print_limb("Left", left_arm_info, "Shoulder", "Elbow", "Wrist");
    print_limb("Right", right_arm_info, "Shoulder", "Elbow", "Wrist");
    print_limb("Left", left_leg_info, "Hip", "Knee", "Heel");
    print_limb("Right", right_leg_info, "Hip", "Knee", "Heel");
}

fn draw_skeletons_system(
    skeleton_visuals: Res<SkeletonVisuals>,
    mut gizmos: Gizmos,
    model_roots: Query<(Entity, &ModelRoot, &GlobalTransform)>,
    skeletons: Query<&PedestrianSkeleton>,
    children_query: Query<&Children>,
    name_query: Query<&Name>,
    transform_query: Query<&GlobalTransform>,
    parent_query: Query<&ChildOf>,
) {
    if !skeleton_visuals.show_skeleton {
        return;
    }

    for (root_entity, _root, _root_gt) in model_roots.iter() {
        let mut skeleton_root = root_entity;
        let mut queue = vec![root_entity];
        while let Some(ent) = queue.pop() {
            if let Ok(name) = name_query.get(ent) {
                if name.as_str() == "Armature" {
                    skeleton_root = ent;
                    break;
                }
            }
            if let Ok(children) = children_query.get(ent) {
                for child in children.iter() {
                    queue.push(child);
                }
            }
        }

        let mut nodes = Vec::new();
        traverse_hierarchy_raw(
            skeleton_root,
            &children_query,
            &name_query,
            &transform_query,
            &mut nodes,
        );

        let entity_to_info: std::collections::HashMap<Entity, (usize, Vec3)> = nodes
            .iter()
            .enumerate()
            .map(|(idx, &(ent, _, pos))| (ent, (idx, pos)))
            .collect();

        let skeleton = skeletons.get(root_entity).ok();

        for &(ent, _, pos) in &nodes {
            let label = skeleton.and_then(|s| s.joint_labels.get(&ent));
            let color = label.map(|l| l.color()).unwrap_or(Color::srgb(0.5, 0.5, 0.5));

            if let Ok(parent) = parent_query.get(ent) {
                if let Some(&(_, parent_pos)) = entity_to_info.get(&parent.get()) {
                    gizmos.line(parent_pos, pos, color);
                }
            }
        }
    }
}

fn picker_system(
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: SpatialQuery,
    parent_query: Query<&ChildOf>,
    model_root_query: Query<(Entity, &ModelRoot, &GlobalTransform)>,
    mut hovered: ResMut<HoveredModel>,
    mut selected: ResMut<SelectedModel>,
    mut contexts: EguiContexts,
) {
    let egui_focused = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
    } else {
        false
    };
    if egui_focused {
        hovered.entity = None;
        return;
    }

    let Some(window) = windows.iter().next() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        hovered.entity = None;
        return;
    };
    let Some((camera, camera_transform)) = camera_query.iter().next() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let ray_dir = ray.direction;

    hovered.entity = None;

    if let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray_dir,
        1000.0,
        true,
        &SpatialQueryFilter::default(),
    ) {
        let mut current = hit.entity;
        let mut found_root = None;
        loop {
            if let Ok((root_ent, root, _)) = model_root_query.get(current) {
                found_root = Some((root_ent, root.index));
                break;
            }
            if let Ok(parent) = parent_query.get(current) {
                current = parent.get();
            } else {
                break;
            }
        }

        if let Some((root_ent, model_idx)) = found_root {
            hovered.entity = Some(root_ent);

            if mouse_button.just_pressed(MouseButton::Left) {
                selected.entity = Some(root_ent);
                info!("Selected model: {} (entity: {:?})", model_idx, root_ent);

                if let Ok((_, root, root_gt)) = model_root_query.get(root_ent) {
                    let model_pos = root_gt.translation();
                    let head_height = root.size.y;

                    let start_pos = camera_transform.translation();
                    let start_rot = camera_transform.rotation();

                    // Camera position in front of pedestrian (facing away towards -Z means front is at -Z)
                    let target_pos = model_pos + Vec3::new(0.0, head_height / 2.0 + 0.3, -1.8);
                    
                    // Look back at the pedestrian's upper chest / face
                    let look_target = model_pos + Vec3::new(0.0, head_height / 4.0, 0.0);
                    let target_rot = Transform::from_translation(target_pos)
                        .looking_at(look_target, Vec3::Y)
                        .rotation;

                    commands.insert_resource(ActiveCameraAnimation {
                        start_pos,
                        start_rot,
                        target_pos,
                        target_rot,
                        elapsed: 0.0,
                        duration: 0.8,
                    });
                }
            }
        }
    }
}

fn draw_hovered_bbox_system(
    mut gizmos: Gizmos,
    hovered: Res<HoveredModel>,
    model_root_query: Query<(&GlobalTransform, &ModelRoot)>,
) {
    if let Some(hovered_ent) = hovered.entity {
        if let Ok((gt, root)) = model_root_query.get(hovered_ent) {
            let center = gt.translation();
            let size = root.size;
            let cuboid = Cuboid::new(size.x, size.y, size.z);
            gizmos.primitive_3d(
                &cuboid,
                Isometry3d::from_translation(center),
                Color::srgb(1.0, 1.0, 0.0),
            );
        }
    }
}

fn draw_gui_system(
    mut contexts: EguiContexts,
    selected: Res<SelectedModel>,
    model_roots: Query<&ModelRoot>,
    mut anim_settings: ResMut<AnimationSettings>,
    mut skeleton_visuals: ResMut<SkeletonVisuals>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new("Pedestrian V2 Viewer Info")
        .default_pos(egui::pos2(12.0, 50.0))
        .show(ctx, |ui| {
            ui.label("Controls:");
            ui.label("- WASD: Move parallel to ground");
            ui.label("- Space / Ctrl: Move Up / Down");
            ui.label("- Mouse Scroll: Height zoom");
            ui.label("- Left Drag: Rotate Camera");
            ui.label("- Hover a pedestrian to show bbox");
            ui.label("- Click a pedestrian to focus/center");

            ui.separator();
            ui.checkbox(&mut skeleton_visuals.show_skeleton, "Show Skeleton Graph");

            ui.separator();
            ui.heading("Skeleton Bone Color Legend:");
            
            let mut show_legend = |label: &str, color: egui::Color32| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, color);
                    ui.label(label);
                });
            };
            
            show_legend("Head & Neck (Pink)", egui::Color32::from_rgb(255, 102, 178));
            show_legend("Spinal Column (Dark Blue)", egui::Color32::from_rgb(0, 0, 128));
            show_legend("Midgroin / Pelvis (Yellow)", egui::Color32::from_rgb(255, 255, 0));
            show_legend("Left Shoulder Connectors (Light Blue)", egui::Color32::from_rgb(127, 204, 255));
            show_legend("Right Shoulder Connectors (Light Pink)", egui::Color32::from_rgb(255, 178, 204));
            show_legend("Left Arm (Purple)", egui::Color32::from_rgb(153, 51, 204));
            show_legend("Right Arm (Orange)", egui::Color32::from_rgb(255, 153, 0));
            show_legend("Left Hand (Dark Yellow)", egui::Color32::from_rgb(153, 153, 0));
            show_legend("Right Hand (Light Yellow)", egui::Color32::from_rgb(255, 255, 127));
            show_legend("Left Leg (Red)", egui::Color32::from_rgb(255, 51, 51));
            show_legend("Right Leg (Green)", egui::Color32::from_rgb(51, 255, 51));
            show_legend("Left Foot (Dark Purple)", egui::Color32::from_rgb(204, 0, 204));
            show_legend("Right Foot (Light Purple/Teal)", egui::Color32::from_rgb(0, 204, 204));
            show_legend("Unclassified (Gray)", egui::Color32::GRAY);

            ui.separator();
            if let Some(selected_ent) = selected.entity {
                if let Ok(root) = model_roots.get(selected_ent) {
                    ui.heading("Selected Pedestrian:");
                    ui.label(format!("Index: {}", root.index));
                    ui.label(format!("Name: {}", root.name));
                    ui.label(format!("Size: {:.2} x {:.2} x {:.2}", root.size.x, root.size.y, root.size.z));
                }
            } else {
                ui.label("No pedestrian selected");
            }
        });

    egui::Window::new("Animation Selector")
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
        .default_size(egui::vec2(250.0, 200.0))
        .show(ctx, |ui| {
            // Speed slider above the list
            ui.add(egui::Slider::new(&mut anim_settings.speed, 0.3..=3.0).text("Speed"));
            
            ui.separator();
            ui.label("Select Animation:");
            
            let anim_names = anim_settings.available_animations.clone();
            let current_selected = anim_settings.selected_animation.clone();

            // A list/selector of animations
            egui::ScrollArea::vertical().show(ui, |ui| {
                for anim_name in &anim_names {
                    if ui.radio(current_selected.as_ref() == Some(anim_name), anim_name).clicked() {
                        anim_settings.selected_animation = Some(anim_name.clone());
                    }
                }
            });
        });
}

#[derive(Component)]
struct CurrentPlayingAnimation {
    name: String,
    speed: f32,
}

fn setup_animation_players_system(
    mut commands: Commands,
    anim_settings: Option<Res<AnimationSettings>>,
    players: Query<Entity, (With<AnimationPlayer>, Without<AnimationGraphHandle>)>,
) {
    let Some(settings) = anim_settings else {
        return;
    };
    for player_ent in &players {
        commands.entity(player_ent).insert(AnimationGraphHandle(settings.graph_handle.clone()));
    }
}

fn play_animations_system(
    mut commands: Commands,
    settings: Res<AnimationSettings>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    model_roots: Query<&PedestrianGltf>,
    mut players: Query<(Entity, &mut AnimationPlayer, Option<&mut CurrentPlayingAnimation>)>,
    parent_query: Query<&ChildOf>,
) {
    for (player_ent, mut player, current_playing) in players.iter_mut() {
        // Find the model root entity by walking up the hierarchy
        let mut current = player_ent;
        let mut model_root_gltf = None;
        loop {
            if let Ok(gltf_comp) = model_roots.get(current) {
                model_root_gltf = Some(gltf_comp);
                break;
            }
            if let Ok(parent) = parent_query.get(current) {
                current = parent.get();
            } else {
                break;
            }
        }

        let Some(gltf_comp) = model_root_gltf else {
            continue;
        };

        let Some(gltf) = gltf_assets.get(&gltf_comp.handle) else {
            continue;
        };

        // Determine which animation to play
        let anim_name = if let Some(selected) = &settings.selected_animation {
            if gltf.named_animations.contains_key(selected.as_str()) {
                selected.as_str()
            } else {
                "A_TPose"
            }
        } else {
            "A_TPose"
        };

        let target_speed = settings.speed;

        let should_update = match &current_playing {
            Some(curr) => curr.name != anim_name || curr.speed != target_speed,
            None => true,
        };

        if should_update {
            if let Some(&node_index) = settings.animation_nodes.get(anim_name) {
                let name_changed = match &current_playing {
                    Some(curr) => curr.name != anim_name,
                    None => true,
                };

                if name_changed {
                    player.stop_all();
                    player.play(node_index).repeat().set_speed(target_speed);
                } else {
                    if let Some(active) = player.animation_mut(node_index) {
                        active.set_speed(target_speed);
                    }
                }

                // Update tracking component
                if let Some(mut curr) = current_playing {
                    curr.name = anim_name.to_string();
                    curr.speed = target_speed;
                } else {
                    commands.entity(player_ent).insert(CurrentPlayingAnimation {
                        name: anim_name.to_string(),
                        speed: target_speed,
                    });
                }
            }
        }
    }
}
