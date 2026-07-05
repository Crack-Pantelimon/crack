//! The pedestrian spawn path: turn a [`SpawnPedestrianEvent`] into a live, aligned,
//! classified pedestrian entity.

use avian3d::collision::collider::{ColliderConstructor, ColliderConstructorHierarchy};
use avian3d::prelude::Collider;
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use crate::plugins::pedestrians::manifest::PedestrianUrl;
use crate::plugins::pedestrians::skeleton::{
    JointData, PedestrianSkeleton, classify_skeleton, traverse_hierarchy_raw,
};

/// Public spawn request: spawn the pedestrian at `url` at `position`.
#[derive(Event, Clone)]
pub struct SpawnPedestrianEvent {
    pub url: PedestrianUrl,
    pub position: Vec3,
}

#[derive(Component)]
pub struct ModelRoot {
    pub index: usize,
    pub name: String,
    pub size: Vec3,
}

#[derive(Component)]
pub struct PedestrianGltf {
    pub handle: Handle<bevy::gltf::Gltf>,
}

#[derive(Component)]
pub struct NeedAlignment;

/// Monotonic index handed to each spawned pedestrian (spawn order).
#[derive(Resource, Default)]
pub struct PedestrianSpawnCounter(pub usize);

pub fn spawn_pedestrian_observer(
    trigger: On<SpawnPedestrianEvent>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut counter: ResMut<PedestrianSpawnCounter>,
) {
    let req = trigger.event();
    let url = &req.url.0;

    let scene_url = GltfAssetLabel::Scene(0).from_asset(url.clone());
    let handle = asset_server.load::<WorldAsset>(scene_url);
    let gltf_handle = asset_server.load::<bevy::gltf::Gltf>(url.clone());

    let index = counter.0;
    counter.0 += 1;

    let model_name = url.split('/').last().unwrap_or(url).replace(".glb", "");

    commands
        .spawn((
            Transform::from_translation(req.position),
            Visibility::default(),
            InheritedVisibility::default(),
            ModelRoot {
                index,
                name: model_name,
                size: Vec3::ZERO,
            },
            PedestrianGltf {
                handle: gltf_handle,
            },
            NeedAlignment,
        ))
        .with_children(|parent| {
            parent.spawn((
                WorldAssetRoot(handle),
                // Do not enable physics on mesh - it would need to be reinterpolated!
                //  ColliderConstructorHierarchy::new(ColliderConstructor::TrimeshFromMesh),
                Transform::IDENTITY,
                Visibility::default(),
                InheritedVisibility::default(),
            ));
        });
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

        let (classification, _, _, _, _, _, right_wrist) =
            classify_skeleton(root_entity, &joints);

        commands.entity(root_entity).insert(PedestrianSkeleton {
            joint_labels: classification,
            right_hand: right_wrist,
        });

        commands.entity(root_entity).remove::<NeedAlignment>();
    }
}
