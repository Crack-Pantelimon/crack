
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use std::collections::{BTreeSet, BinaryHeap};

use crate::plugins::map_plugin::{Data3DResource, MapTreeNode};



#[derive(Component)]
pub struct RenderedNodeModel {
    node_name: String,
}

pub fn sync_node_models(
    mut commands: Commands,
    data_res: Res<Data3DResource>,
    model_query: Query<(Entity, &RenderedNodeModel)>,
) {
    if !data_res.parsed {
        return;
    }

    // Despawn models for nodes that are no longer in rendered_nodes
    let mut spawned_names = BTreeSet::new();
    for (entity, model) in &model_query {
        if !data_res.rendered_nodes.contains(&model.node_name) {
            commands.entity(entity).despawn();
        } else {
            spawned_names.insert(model.node_name.clone());
        }
    }

    // Spawn models for nodes in rendered_nodes that aren't spawned yet
    for node_name in &data_res.rendered_nodes {
        if !spawned_names.contains(node_name) {
            if let Some(handle) = data_res.loaded_scenes.get(node_name) {
                commands.spawn((
                    WorldAssetRoot(handle.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0),
                    RenderedNodeModel {
                        node_name: node_name.clone(),
                    },
                    avian3d::prelude::RigidBody::Static,
                    avian3d::prelude::ColliderConstructorHierarchy::new(
                        avian3d::prelude::ColliderConstructor::TrimeshFromMesh,
                    ),
                ));
            }
        }
    }
}

#[inline]
fn compute_distance_to_aabb(node: &MapTreeNode, p: Vec3) -> f32 {
    let cx =
        p.x.clamp(node.bbox.min.x.min(node.bbox.max.x), node.bbox.min.x.max(node.bbox.max.x));
    let cy =
        p.y.clamp(node.bbox.min.y.min(node.bbox.max.y), node.bbox.min.y.max(node.bbox.max.y));
    let cz =
        p.z.clamp(node.bbox.min.z.min(node.bbox.max.z), node.bbox.min.z.max(node.bbox.max.z));
    let d1 = p.distance(Vec3::new(cx, cy, cz));
    let middle = Vec3::new(
        (node.bbox.min.x + node.bbox.max.x) / 2.0,
        (node.bbox.min.y + node.bbox.max.y) / 2.0,
        (node.bbox.min.z + node.bbox.max.z) / 2.0,
    );
    d1 + p.distance(middle)
}

pub fn recompute_lod_system(
    mut data_res: ResMut<Data3DResource>,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
) {
    if !data_res.parsed {
        return;
    }

    // 1. Check if any loading assets finished loading
    let mut newly_loaded = Vec::new();
    for (name, handle) in &data_res.loading_scenes {
        if asset_server.load_state(handle.id()).is_loaded() {
            newly_loaded.push(name.clone());
        }
    }
    for name in newly_loaded {
        if let Some(handle) = data_res.loading_scenes.remove(&name) {
            data_res.loaded_scenes.insert(name, handle);
        }
    }

    // 2. Tick timer and run recompute if timed out
    if let Some(ref mut timer) = data_res.lod_timer {
        timer.tick(time.delta());
        if timer.just_finished() {
            // Reset with random duration 0.15s +/- 0.05s
            let next_timeout = 0.1 + rand::random::<f32>() * 0.1;
            timer.set_duration(std::time::Duration::from_secs_f32(next_timeout));
            timer.reset();

            // Early exit check: did budget or reference points change?
            let budget_changed = data_res.lod_budget != data_res.last_lod_budget;
            let refs_changed = data_res.reference_points != data_res.last_reference_points;

            if !budget_changed && !refs_changed {
                return;
            }

            let start_time = _crack_utils::get_timestamp_now_ms();

            // Determine re-evaluated nodes and update cache
            let mut nodes_to_reevaluate = BTreeSet::new();
            let last_refs = &data_res.last_reference_points;
            let new_refs = &data_res.reference_points;

            let mut addition_idx = None;
            let mut removal_idx = None;

            if new_refs.len() == last_refs.len() + 1 && new_refs[..last_refs.len()] == *last_refs {
                addition_idx = Some(last_refs.len());
            } else if last_refs.len() > 0 && new_refs.len() == last_refs.len() - 1 {
                let mut diff_at = last_refs.len() - 1;
                for i in 0..new_refs.len() {
                    if new_refs[i] != last_refs[i] {
                        diff_at = i;
                        break;
                    }
                }
                if new_refs[diff_at..] == last_refs[diff_at + 1..] {
                    removal_idx = Some(diff_at);
                }
            }

            if let Some(idx) = addition_idx {
                let new_pt = new_refs[idx];
                let names: Vec<String> = data_res.nodes.keys().cloned().collect();
                for name in names {
                    let node = data_res.nodes.get(&name).unwrap();
                    let d = compute_distance_to_aabb(node, new_pt);
                    let old_min = *data_res
                        .node_min_distances
                        .get(&name)
                        .unwrap_or(&f32::INFINITY);

                    {
                        let dists = data_res.node_distances.entry(name.clone()).or_default();
                        dists.push(d);
                    }

                    if d < old_min || budget_changed {
                        data_res.node_min_distances.insert(name.clone(), d);
                        nodes_to_reevaluate.insert(name);
                    } else if budget_changed {
                        nodes_to_reevaluate.insert(name);
                    }
                }
            } else if let Some(idx) = removal_idx {
                let names: Vec<String> = data_res.nodes.keys().cloned().collect();
                for name in names {
                    let old_min = *data_res
                        .node_min_distances
                        .get(&name)
                        .unwrap_or(&f32::INFINITY);
                    let removed_d = {
                        let dists = data_res.node_distances.get_mut(&name).unwrap();
                        dists.remove(idx)
                    };

                    if (removed_d - old_min).abs() < 0.0001 || budget_changed {
                        let new_min = {
                            let dists = data_res.node_distances.get(&name).unwrap();
                            if dists.is_empty() {
                                let node = data_res.nodes.get(&name).unwrap();
                                compute_distance_to_aabb(node, Vec3::ZERO)
                            } else {
                                dists.iter().copied().fold(f32::INFINITY, f32::min)
                            }
                        };
                        data_res.node_min_distances.insert(name.clone(), new_min);
                        nodes_to_reevaluate.insert(name);
                    } else if budget_changed {
                        nodes_to_reevaluate.insert(name);
                    }
                }
            } else {
                let names: Vec<String> = data_res.nodes.keys().cloned().collect();
                let refs_to_use = if new_refs.is_empty() {
                    vec![Vec3::ZERO]
                } else {
                    new_refs.clone()
                };

                for name in names {
                    let node = data_res.nodes.get(&name).unwrap();
                    let mut new_dists = Vec::new();
                    for &pt in &refs_to_use {
                        new_dists.push(compute_distance_to_aabb(node, pt));
                    }
                    let new_min = new_dists.iter().copied().fold(f32::INFINITY, f32::min);
                    let old_min = *data_res
                        .node_min_distances
                        .get(&name)
                        .unwrap_or(&f32::INFINITY);

                    data_res.node_distances.insert(name.clone(), new_dists);
                    data_res.node_min_distances.insert(name.clone(), new_min);

                    if (new_min - old_min).abs() > 0.0001 || budget_changed {
                        nodes_to_reevaluate.insert(name);
                    }
                }
            }

            // Run subdivision
            let (target_rendered, target_loaded) = run_lod_subdivision(&data_res);
            data_res.target_rendered_nodes = Some(target_rendered);

            // Fetch any target assets that aren't loaded or loading
            for node_name in &target_loaded {
                if !data_res.loaded_scenes.contains_key(node_name)
                    && !data_res.loading_scenes.contains_key(node_name)
                {
                    if let Some(node) = data_res.nodes.get(node_name) {
                        if let Some(ref filename) = node.filename {
                            let glb_url =
                                format!("{}/3d_data/{}", crate::config::DATA_BASE_URL, filename);
                            let asset_path = GltfAssetLabel::Scene(0).from_asset(glb_url);
                            let handle = asset_server.load(asset_path);
                            data_res.loading_scenes.insert(node_name.clone(), handle);
                        }
                    }
                }
            }

            // Deterministic logging to console
            let elapsed_ms = _crack_utils::get_timestamp_now_ms() - start_time;
            info!(
                "LOD recompute iteration: budget = {}, ref_points = {}, rendered = {} tiles, re-evaluated nodes = {}, took = {}ms",
                data_res.lod_budget,
                data_res.reference_points.len(),
                data_res
                    .target_rendered_nodes
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(0),
                nodes_to_reevaluate.len(),
                elapsed_ms
            );

            // Update last budget and reference points cache
            data_res.last_lod_budget = data_res.lod_budget;
            data_res.last_reference_points = data_res.reference_points.clone();
        }
    }

    // 3. If target_rendered_nodes is set, check if all of its leaf nodes are loaded
    if let Some(ref target) = data_res.target_rendered_nodes {
        let all_loaded = target
            .iter()
            .all(|name| data_res.loaded_scenes.contains_key(name));
        if all_loaded {
            data_res.rendered_nodes = target.clone();
            data_res.target_rendered_nodes = None;

            // Retain only ancestors and currently rendered nodes in loaded_scenes
            let mut needed_loaded_nodes = BTreeSet::new();
            for rendered in &data_res.rendered_nodes {
                needed_loaded_nodes.insert(rendered.clone());
                let mut curr = rendered.clone();
                while let Some(parent) = data_res.parents.get(&curr) {
                    needed_loaded_nodes.insert(parent.clone());
                    curr = parent.clone();
                }
            }
            data_res
                .loaded_scenes
                .retain(|name, _| needed_loaded_nodes.contains(name));
        }
    }
}

#[derive(PartialEq, Eq)]
struct Candidate {
    metric: bevy::math::FloatOrd,
    node_name: String,
}
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .metric
            .cmp(&self.metric)
            .then_with(|| self.node_name.cmp(&other.node_name))
    }
}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn run_lod_subdivision(data_res: &Data3DResource) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut rendered = BTreeSet::new();
    let mut loaded = BTreeSet::new();
    for root in &data_res.roots {
        rendered.insert(root.clone());
        loaded.insert(root.clone());
    }

    let compute_metric = |node_name: &str| -> f32 {
        if let Some(node) = data_res.nodes.get(node_name) {
            let size = Vec3::new(
                (node.bbox.max.x - node.bbox.min.x).abs(),
                (node.bbox.max.y - node.bbox.min.y).abs(),
                (node.bbox.max.z - node.bbox.min.z).abs(),
            );
            let tile_diagonal = size.length().max(0.0001);
            let min_dist = *data_res.node_min_distances.get(node_name).unwrap_or(&0.0);
            min_dist / tile_diagonal
        } else {
            f32::INFINITY
        }
    };

    // Initialize min-heap with roots that have children
    let mut heap = BinaryHeap::new();
    for root in &data_res.roots {
        if data_res.children.contains_key(root) {
            let metric = compute_metric(root);
            heap.push(Candidate {
                metric: bevy::math::FloatOrd(metric),
                node_name: root.clone(),
            });
        }
    }

    while let Some(candidate) = heap.pop() {
        if let Some(child_map) = data_res.children.get(&candidate.node_name) {
            let children_count = child_map.len();
            if loaded.len() + children_count <= data_res.lod_budget as usize {
                // Perform split
                rendered.remove(&candidate.node_name);
                for child in child_map.values() {
                    rendered.insert(child.clone());
                    loaded.insert(child.clone());
                    if data_res.children.contains_key(child) {
                        let metric = compute_metric(child);
                        heap.push(Candidate {
                            metric: bevy::math::FloatOrd(metric),
                            node_name: child.clone(),
                        });
                    }
                }
            } else {
                break;
            }
        }
    }

    (rendered, loaded)
}
