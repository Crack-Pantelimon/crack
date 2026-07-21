//! The pedestrian spawn path: turn a [`SpawnPedestrianEvent`] into a live, aligned,
//! classified pedestrian entity.

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use crate::basic_app::MemoryDir;
use crate::plugins::pedestrians::manifest::PedestrianUrl;
use crate::plugins::pedestrians::skeleton::{
    JointData, PedestrianSkeleton, classify_skeleton, traverse_hierarchy_raw,
};

/// Public spawn request: spawn the pedestrian at `url` at `position`.
#[derive(Event, Clone)]
pub struct SpawnPedestrianEvent {
/// url field.
    pub url: PedestrianUrl,
/// position field.
    pub position: Vec3,
/// controller field.
    pub controller: Entity,
/// parent field.
    pub parent: Entity,
}

/// model root.
#[derive(Component)]
pub struct ModelRoot {
/// index field.
    pub index: usize,
/// name field.
    pub name: String,
/// size field.
    pub size: Vec3,
}

/// pedestrian gltf.
#[derive(Component)]
pub struct PedestrianGltf {
/// handle field.
    pub handle: Handle<bevy::gltf::Gltf>,
}

/// need alignment.
#[derive(Component)]
pub struct NeedAlignment;

/// model controller.
#[derive(Component)]
pub struct ModelController(pub Entity);

/// Monotonic index handed to each spawned pedestrian (spawn order).
#[derive(Resource, Default)]
pub struct PedestrianSpawnCounter(pub usize);

fn parse_url_to_rpc_args(url: &str) -> (String, String) {
    let base_url = crate::config::DATA_BASE_URL.trim_end_matches('/');
    let glb_path = if url.starts_with(base_url) {
        url[base_url.len()..].trim_start_matches('/').to_string()
    } else {
        if let Some(pos) = url.find("/3d_data/") {
            url[pos..].trim_start_matches('/').to_string()
        } else {
            url.to_string()
        }
    };
    let asset_id = url.split('/').last().unwrap_or(url).to_string();
    (glb_path, asset_id)
}

/// pending pedestrian glb fetch.
#[derive(Component)]
pub struct PendingPedestrianGlbFetch {
/// task field.
    pub task: bevy::tasks::Task<anyhow::Result<game_logic::glb::FetchGlbResponse>>,
/// controller field.
    pub controller: Entity,
/// model name field.
    pub model_name: String,
}

/// spawn pedestrian observer.
pub fn spawn_pedestrian_observer(
    trigger: On<SpawnPedestrianEvent>,
    mut commands: Commands,
    client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,
) {
    let req = trigger.event();
    let url = &req.url.0;

    let Some(client) = client else {
        tracing::error!("Cannot spawn pedestrian: CrackClient not available");
        return;
    };

    let (glb_path, asset_id) = parse_url_to_rpc_args(url);
    let api_client = client.0.clone();
    let base_url = crate::config::DATA_BASE_URL.to_string();

    let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
        api_client
            .call::<game_logic::api::FetchPedestrianModel>(game_logic::glb::FetchGlbRequest {
                base_url,
                glb_path,
                asset_id,
            })
            .await
    });

    let model_name = url.split('/').last().unwrap_or(url).replace(".glb", "");

    commands.spawn((
        ChildOf(req.parent),
        Transform::IDENTITY,
        Visibility::default(),
        InheritedVisibility::default(),
        PendingPedestrianGlbFetch {
            task,
            controller: req.controller,
            model_name,
        },
    ));
}

/// poll pedestrian glb fetches.
pub fn poll_pedestrian_glb_fetches(
    mut commands: Commands,
    mut q_fetches: Query<(Entity, &mut PendingPedestrianGlbFetch)>,
    memory_dir: ResMut<MemoryDir>,
    asset_server: Res<AssetServer>,
    mut counter: ResMut<PedestrianSpawnCounter>,
) {
    for (entity, mut fetch) in q_fetches.iter_mut() {
        if let Some(res) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut fetch.task),
        ) {
            match res {
                Ok(response) => {
                    let sanitized_id = response
                        .asset_id
                        .replace('/', "_")
                        .replace('\\', "_")
                        .replace('.', "_");
                    let memory_path = format!("ped_{}.glb", sanitized_id);

                    // Insert bytes into MemoryDir
                    memory_dir.dir.insert_asset(
                        std::path::Path::new(&memory_path),
                        response.glb_bytes.clone(),
                    );

                    // Build memory URLs
                    let scene_url =
                        GltfAssetLabel::Scene(0).from_asset(format!("memory://{}", memory_path));
                    let gltf_url = format!("memory://{}", memory_path);

                    let handle = asset_server.load::<WorldAsset>(scene_url);
                    let gltf_handle = asset_server.load::<bevy::gltf::Gltf>(gltf_url);

                    let index = counter.0;
                    counter.0 += 1;

                    // Add the components to the existing parent entity
                    commands
                        .entity(entity)
                        .insert((
                            ModelRoot {
                                index,
                                name: fetch.model_name.clone(),
                                size: Vec3::ZERO,
                            },
                            PedestrianGltf {
                                handle: gltf_handle,
                            },
                            NeedAlignment,
                            ModelController(fetch.controller),
                        ))
                        .with_children(|parent| {
                            parent.spawn((
                                WorldAssetRoot(handle),
                                Transform::IDENTITY,
                                Visibility::default(),
                                InheritedVisibility::default(),
                            ));
                        });

                    // Remove the fetch component
                    commands
                        .entity(entity)
                        .remove::<PendingPedestrianGlbFetch>();
                }
                Err(e) => {
                    tracing::error!("Pedestrian model fetch RPC error: {e:?}");
                    // Despawn parent entity if fetch failed
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

/// link pedestrian model.
pub fn link_pedestrian_model(
    mut commands: Commands,
    mut controlled: Option<
        ResMut<crate::plugins::pedestrians::pedestrian_controller_plugin::ControlledCharacter>,
    >,
    q_models: Query<(Entity, &ModelController), Added<ModelRoot>>,
    q_ai: Query<(), With<crate::plugins::pedestrian_ai::AiPedestrian>>,
) {
    for (model_ent, controller_ref) in &q_models {
        let controller = controller_ref.0;
        if q_ai.get(controller).is_ok() {
            // It's an AI pedestrian!
            commands
                .entity(controller)
                .insert(crate::plugins::pedestrian_ai::AiModel(model_ent));
        } else {
            // Player or remote player!
            let is_local_player = controlled
                .as_ref()
                .map_or(false, |ctrl| ctrl.controller == Some(controller));
            if is_local_player {
                commands
                    .entity(model_ent)
                    .insert(crate::plugins::pedestrians::ManualAnimation);
                if let Some(ref mut ctrl) = controlled {
                    ctrl.ped = Some(model_ent);
                    ctrl.awaiting = false;
                }
            }
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

/// init pedestrians system.
pub fn init_pedestrians_system(
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
                // if let Some(collider) = Collider::trimesh_from_mesh(mesh) {
                //     commands.entity(*ent).insert(collider);
                // }
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

        let (
            classification,
            left_shoulder,
            left_elbow,
            left_wrist,
            right_shoulder,
            right_elbow,
            right_wrist,
            spine,
        ) = classify_skeleton(root_entity, &joints);

        commands.entity(root_entity).insert(PedestrianSkeleton {
            joint_labels: classification,
            right_hand: right_wrist,
            left_shoulder,
            left_elbow,
            left_wrist,
            right_shoulder,
            right_elbow,
            right_wrist,
            spine,
        });

        commands.entity(root_entity).remove::<NeedAlignment>();
    }
}
