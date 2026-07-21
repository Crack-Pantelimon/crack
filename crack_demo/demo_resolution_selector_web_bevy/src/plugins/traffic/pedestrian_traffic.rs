use avian3d::prelude::*;
use bevy::prelude::*;

use super::road_graph::{RerouteMode, TrafficRoadGraph, pick_continuation, quantize};
use super::spawn::get_ground_y;
use super::{
    LOOKAHEAD_XZ, PED_ROAD_OFFSET, PED_SPAWN_SPACING, PED_STUCK_REROUTE_S, SPAWN_MIN_CAMERA_DIST,
    SpawnTrafficPedestrianEvent, TrafficConfig, TrafficPedestrian, VIEW_RAYCAST_HZ,
    WAYPOINT_REACHED_XZ,
};
use crate::plugins::{
    map_plugin::MapTree,
    pedestrian_ai::{AiPedestrian, AiState, Faction, spawn_ai::SpawnAiPedestrianEvent},
    pedestrians::pedestrian_controller_plugin::{LocomotionInput, MainCamera},
};

/// pending traffic peds.
#[derive(Resource, Default)]
pub struct PendingTrafficPeds {
/// pending field.
    pub pending: Vec<PendingTrafficPedEntry>,
}

/// pending traffic ped entry.
pub struct PendingTrafficPedEntry {
/// spawn pos field.
    pub spawn_pos: Vec3,
/// path field.
    pub path: Vec<Vec3>,
/// current seg field.
    pub current_seg: usize,
/// offset sign field.
    pub offset_sign: f32,
}

/// traffic pedestrian spawner.
pub fn traffic_pedestrian_spawner(
    time: Res<Time>,
    mut last_spawn: Local<f32>,
    config: Res<TrafficConfig>,
    graph: Res<TrafficRoadGraph>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_traffic_peds: Query<(), With<TrafficPedestrian>>,
    q_all_peds: Query<&Transform, With<AiPedestrian>>,
    mut commands: Commands,
) {
    if !config.ped_enabled || !graph.built {
        return;
    }

    let count = q_traffic_peds.iter().count();
    if count >= config.max_peds {
        return;
    }

    let fast_fill = count < (config.max_peds as f32 * super::FAST_FILL_FRACTION) as usize;
    let now = time.elapsed_secs();
    if !fast_fill && now - *last_spawn < super::SPAWN_INTERVAL_S {
        return;
    }

    let Some((camera, cam_gt)) = q_camera.iter().next() else {
        return;
    };

    let existing = q_all_peds
        .iter()
        .map(|tf| tf.translation)
        .collect::<Vec<_>>();

    if let Some(candidate_point) = super::common::pick_spawn_candidate(
        &graph,
        camera,
        cam_gt,
        config.spawn_radius,
        SPAWN_MIN_CAMERA_DIST,
        PED_SPAWN_SPACING,
        &existing,
        fast_fill,
    ) {
        commands.trigger(SpawnTrafficPedestrianEvent {
            position: candidate_point,
        });
        if !fast_fill {
            *last_spawn = now;
        }
    }
}

/// spawn traffic pedestrian observer.
pub fn spawn_traffic_pedestrian_observer(
    trigger: On<SpawnTrafficPedestrianEvent>,
    mut commands: Commands,
    graph: Res<TrafficRoadGraph>,
    map_tree: Option<Res<MapTree>>,
    spatial_query: avian3d::prelude::SpatialQuery,
    mut pending_traffic: ResMut<PendingTrafficPeds>,
) {
    if !graph.built || graph.segments.is_empty() {
        warn!("SpawnTrafficPedestrianEvent: road graph is not built or empty.");
        return;
    }

    let req_pos = trigger.event().position;

    let Some((closest_seg_idx, path_points)) = super::common::build_path_from(&graph, req_pos)
    else {
        return;
    };

    // 3. Pick offset_sign randomly (+1 / -1)
    let offset_sign = if rand::random::<bool>() { 1.0 } else { -1.0 };

    // 4. Ground the spawn position (with offset applied)
    let start_pos = path_points[0];
    let next_pos = path_points[1];
    let dir = (next_pos - start_pos).normalize_or_zero();
    let perp = Vec3::new(dir.z, 0.0, -dir.x);
    let offset_spawn = start_pos + perp * offset_sign * PED_ROAD_OFFSET;

    let ground_y = get_ground_y(
        offset_spawn,
        map_tree.as_ref().map(|r| &**r),
        &spatial_query,
    );
    let spawn_pos = Vec3::new(offset_spawn.x, ground_y, offset_spawn.z);

    let faction = match rand::random::<u32>() % 5 {
        0 => Faction::Neutral,
        1 => Faction::Red,
        2 => Faction::Green,
        3 => Faction::Blue,
        _ => Faction::Yellow,
    };

    // 5. Trigger SpawnAiPedestrianEvent with random faction (no weapon, random model)
    commands.trigger(SpawnAiPedestrianEvent {
        position: spawn_pos,
        faction,
        url: None,
        weapon: None,
        car_seat: None,
    });

    // 6. Push to PendingTrafficPeds queue
    pending_traffic.pending.push(PendingTrafficPedEntry {
        spawn_pos,
        path: path_points,
        current_seg: closest_seg_idx,
        offset_sign,
    });
}

/// adopt traffic pedestrians.
pub fn adopt_traffic_pedestrians(
    mut commands: Commands,
    mut pending: ResMut<PendingTrafficPeds>,
    q_new_ai: Query<(Entity, &Transform), (Added<AiPedestrian>, Without<TrafficPedestrian>)>,
) {
    if pending.pending.is_empty() {
        return;
    }

    for (entity, transform) in q_new_ai.iter() {
        let ped_pos = transform.translation;
        let mut best_idx = None;
        let mut best_dist = 5.0; // 5m radius to match the spawned ped

        for (idx, entry) in pending.pending.iter().enumerate() {
            let dist = ped_pos.distance(entry.spawn_pos);
            if dist < best_dist {
                best_dist = dist;
                best_idx = Some(idx);
            }
        }

        if let Some(idx) = best_idx {
            let entry = pending.pending.remove(idx);
            commands.entity(entity).insert(TrafficPedestrian {
                state: super::common::TrafficAgentState::new(entry.path, entry.current_seg),
                offset_sign: entry.offset_sign,
                last_pos: entry.spawn_pos,
            });
        }
    }
}

/// drive traffic pedestrians.
pub fn drive_traffic_pedestrians(
    time: Res<Time>,
    graph: Res<TrafficRoadGraph>,
    mut q_peds: Query<(
        Entity,
        &GlobalTransform,
        &AiState,
        &mut TrafficPedestrian,
        &mut LocomotionInput,
    )>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 || !graph.built {
        return;
    }

    for (_entity, gt, state, mut tp, mut input) in q_peds.iter_mut() {
        // Only override when AI is idle
        if *state != AiState::Idle {
            continue;
        }

        let ped_pos = gt.translation();

        // 1. Stuck detection & recovery (1.0s distance-based check)
        let mut is_stuck = false;
        tp.state.still_timer += dt;
        if tp.state.still_timer >= PED_STUCK_REROUTE_S {
            let dist_moved = ped_pos.distance(tp.last_pos);
            if dist_moved < 0.3 && !tp.state.path.is_empty() {
                is_stuck = true;
            }
            tp.last_pos = ped_pos;
            tp.state.still_timer = 0.0;
        }

        if is_stuck {
            // Reroute randomly from current nearest node
            let seg = &graph.segments[tp.state.current_seg];
            let dist_a = seg.points[0].distance(ped_pos);
            let dist_b = seg.points.last().unwrap().distance(ped_pos);
            let nearest_node = if dist_a < dist_b {
                quantize(seg.points[0])
            } else {
                quantize(*seg.points.last().unwrap())
            };

            if let Some((next_seg, next_points)) = pick_continuation(
                &graph,
                nearest_node,
                tp.state.current_seg,
                RerouteMode::Random,
            ) {
                tp.state.path = next_points;
                tp.state.next_idx = 1;
                tp.state.current_seg = next_seg;
            } else {
                // Snap to nearest segment overall fallback
                if let Some((closest_seg, path_points)) =
                    super::common::build_path_from(&graph, ped_pos)
                {
                    tp.state.path = path_points;
                    tp.state.next_idx = 1;
                    tp.state.current_seg = closest_seg;
                }
            }
            continue;
        }

        // 2. Advance waypoint index if close in XZ plane to the offset target
        while tp.state.next_idx < tp.state.path.len() {
            let target = tp.state.path[tp.state.next_idx];
            let dir = if tp.state.next_idx > 0 {
                (tp.state.path[tp.state.next_idx] - tp.state.path[tp.state.next_idx - 1])
                    .normalize_or_zero()
            } else if tp.state.path.len() >= 2 {
                (tp.state.path[1] - tp.state.path[0]).normalize_or_zero()
            } else {
                Vec3::ZERO
            };
            let mut offset_target = target;
            if dir != Vec3::ZERO {
                let perp = Vec3::new(dir.z, 0.0, -dir.x);
                offset_target += perp * tp.offset_sign * PED_ROAD_OFFSET;
            }

            let dist_xz =
                Vec2::new(ped_pos.x - offset_target.x, ped_pos.z - offset_target.z).length();
            if dist_xz < WAYPOINT_REACHED_XZ {
                tp.state.next_idx += 1;
            } else {
                break;
            }
        }

        // 3. Reroute at path end
        if tp.state.next_idx >= tp.state.path.len() {
            let last_node = quantize(*tp.state.path.last().unwrap());
            let current_points = &tp.state.path;
            let last_dir = if current_points.len() >= 2 {
                let len = current_points.len();
                (current_points[len - 1] - current_points[len - 2]).normalize_or_zero()
            } else {
                Vec3::ZERO
            };

            if let Some((next_seg, next_points)) = pick_continuation(
                &graph,
                last_node,
                tp.state.current_seg,
                RerouteMode::ClosestAngle(last_dir),
            ) {
                let mut new_path = vec![*tp.state.path.last().unwrap()];
                new_path.extend(next_points[1..].iter().cloned());
                tp.state.path = new_path;
                tp.state.next_idx = 1;
                tp.state.current_seg = next_seg;
            } else {
                // Reverse current segment to turn around
                let seg = &graph.segments[tp.state.current_seg];
                let start_quant = quantize(seg.points[0]);
                let reversed_points: Vec<Vec3> = if start_quant == last_node {
                    seg.points.clone()
                } else {
                    seg.points.iter().cloned().rev().collect()
                };

                let mut new_path = vec![*tp.state.path.last().unwrap()];
                new_path.extend(reversed_points[1..].iter().cloned());
                tp.state.path = new_path;
                tp.state.next_idx = 1;
            }
        }

        // 4. Lookahead target
        let mut target_idx = tp.state.next_idx;
        while target_idx < tp.state.path.len() {
            let target = tp.state.path[target_idx];
            let dist_xz = Vec2::new(ped_pos.x - target.x, ped_pos.z - target.z).length();
            if dist_xz >= LOOKAHEAD_XZ {
                break;
            }
            target_idx += 1;
        }
        let target_idx = target_idx.min(tp.state.path.len() - 1);
        if tp.state.path.is_empty() {
            continue;
        }
        let mut target = tp.state.path[target_idx];

        // 5. Offset laterally
        let dir = if target_idx > 0 {
            (tp.state.path[target_idx] - tp.state.path[target_idx - 1]).normalize_or_zero()
        } else if tp.state.path.len() >= 2 {
            (tp.state.path[1] - tp.state.path[0]).normalize_or_zero()
        } else {
            Vec3::ZERO
        };

        if dir != Vec3::ZERO {
            let perp = Vec3::new(dir.z, 0.0, -dir.x);
            target += perp * tp.offset_sign * PED_ROAD_OFFSET;
        }

        // Set move direction in LocomotionInput
        let to_target = target - ped_pos;
        let d = to_target.normalize_or_zero();
        input.move_dir = Vec2::new(d.x, -d.z);
    }
}

/// despawn traffic pedestrians.
pub fn despawn_traffic_pedestrians(
    time: Res<Time>,
    config: Res<TrafficConfig>,
    mut q_peds: Query<(Entity, &Transform, &mut TrafficPedestrian)>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_parent: Query<&ChildOf>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
    mut raycast_timer: Local<f32>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    *raycast_timer += dt;
    let run_raycasts = if *raycast_timer >= 1.0 / VIEW_RAYCAST_HZ {
        *raycast_timer = 0.0;
        true
    } else {
        false
    };

    let Some((camera, cam_gt)) = q_camera.iter().next() else {
        return;
    };
    let camera_pos = cam_gt.translation();

    for (entity, transform, mut tp) in q_peds.iter_mut() {
        let ped_pos = transform.translation;
        let dist_to_camera = ped_pos.distance(camera_pos);

        // 1. Direct should_despawn check (fast path using cached visibility)
        if super::common::should_despawn(dist_to_camera, config.spawn_radius, &tp.state) {
            commands.entity(entity).despawn();
            continue;
        }

        // 2. Out of view timer check
        if run_raycasts {
            let ped_top = ped_pos + Vec3::Y * 1.6;
            let visible = super::common::update_visibility(
                camera,
                cam_gt,
                &spatial_query,
                entity,
                ped_top,
                &q_parent,
            );

            tp.state.last_visible = visible;
            if visible {
                tp.state.out_of_view_timer = 0.0;
            }
        }

        if !tp.state.last_visible {
            tp.state.out_of_view_timer += dt;
        } else {
            tp.state.out_of_view_timer = 0.0;
        }

        // Recheck despawn condition after visibility update
        if super::common::should_despawn(dist_to_camera, config.spawn_radius, &tp.state) {
            commands.entity(entity).despawn();
        }
    }
}
