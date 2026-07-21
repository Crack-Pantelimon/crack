use super::road_graph::{RerouteMode, TrafficRoadGraph, pick_continuation, quantize};
use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

/// traffic agent state.
#[derive(Clone, Debug)]
pub struct TrafficAgentState {
/// path field.
    pub path: Vec<Vec3>,
/// next idx field.
    pub next_idx: usize,
/// current seg field.
    pub current_seg: usize,
/// stuck timer field.
    pub stuck_timer: f32,
/// still timer field.
    pub still_timer: f32, // Accumulator for still (no movement) time
/// out of view timer field.
    pub out_of_view_timer: f32,
/// last visible field.
    pub last_visible: bool,
}

impl TrafficAgentState {
/// new.
    pub fn new(path: Vec<Vec3>, current_seg: usize) -> Self {
        Self {
            path,
            next_idx: 1,
            current_seg,
            stuck_timer: 0.0,
            still_timer: 0.0,
            out_of_view_timer: 0.0,
            last_visible: true,
        }
    }
}

/// walk up to root.
pub fn walk_up_to_root(
    hit_entity: Entity,
    root_entity: Entity,
    q_parent: &Query<&ChildOf>,
) -> bool {
    let mut current = hit_entity;
    loop {
        if current == root_entity {
            return true;
        }
        if let Ok(child_of) = q_parent.get(current) {
            current = child_of.parent();
        } else {
            return false;
        }
    }
}

/// update visibility.
pub fn update_visibility(
    camera: &Camera,
    cam_gt: &GlobalTransform,
    spatial_query: &SpatialQuery,
    root_entity: Entity,
    probe_point: Vec3,
    q_parent: &Query<&ChildOf>,
) -> bool {
    let camera_pos = cam_gt.translation();

    // Check frustum first
    let in_frustum = if let Some(ndc) = camera.world_to_ndc(cam_gt, probe_point) {
        ndc.x >= -1.0
            && ndc.x <= 1.0
            && ndc.y >= -1.0
            && ndc.y <= 1.0
            && ndc.z >= 0.0
            && ndc.z <= 1.0
    } else {
        false
    };

    if !in_frustum {
        return false;
    }

    // In frustum, run occlusion raycast
    let cam_to_target = probe_point - camera_pos;
    let dist = cam_to_target.length();
    let dir_vec = cam_to_target.normalize_or_zero();

    if dir_vec == Vec3::ZERO {
        return false;
    }

    let Ok(hit_dir) = bevy::prelude::Dir3::new(dir_vec) else {
        return false;
    };

    if let Some(hit) = spatial_query.cast_ray(
        camera_pos,
        hit_dir,
        dist - 0.1,
        true,
        &SpatialQueryFilter::default(),
    ) {
        walk_up_to_root(hit.entity, root_entity, q_parent)
    } else {
        true
    }
}

/// should despawn.
pub fn should_despawn(dist_to_camera: f32, spawn_radius: f32, state: &TrafficAgentState) -> bool {
    if dist_to_camera > spawn_radius * super::OUT_OF_RANGE_FACTOR && !state.last_visible {
        return true;
    }

    if state.stuck_timer > super::STUCK_HARD_DESPAWN_S && !state.last_visible {
        return true;
    }

    if state.out_of_view_timer > super::OUT_OF_VIEW_DESPAWN_S {
        return true;
    }

    false
}

/// build path from.
pub fn build_path_from(
    graph: &TrafficRoadGraph,
    pos: Vec3,
) -> Option<(
    usize,     /*closest_seg_idx*/
    Vec<Vec3>, /*path_points*/
)> {
    if graph.segments.is_empty() {
        return None;
    }

    let mut closest_dist = f32::MAX;
    let mut closest_seg_idx = 0;
    let mut closest_pt_idx = 0;

    for (s_idx, seg) in graph.segments.iter().enumerate() {
        for (p_idx, &pt) in seg.points.iter().enumerate() {
            let d = pt.distance(pos);
            if d < closest_dist {
                closest_dist = d;
                closest_seg_idx = s_idx;
                closest_pt_idx = p_idx;
            }
        }
    }

    let seg = &graph.segments[closest_seg_idx];
    let mut forward_dist = 0.0;
    for w in seg.points[closest_pt_idx..].windows(2) {
        forward_dist += w[0].distance(w[1]);
    }
    let mut backward_dist = 0.0;
    for w in seg.points[..=closest_pt_idx].windows(2) {
        backward_dist += w[0].distance(w[1]);
    }
    let forward = forward_dist >= backward_dist;

    let mut path_points = if forward {
        seg.points[closest_pt_idx..].to_vec()
    } else {
        seg.points[..=closest_pt_idx]
            .iter()
            .cloned()
            .rev()
            .collect::<Vec<_>>()
    };

    if path_points.len() >= 2 {
        let end_node = quantize(*path_points.last().unwrap());
        let dir = (path_points[1] - path_points[0]).normalize_or_zero();
        if let Some((_, next_points)) = pick_continuation(
            graph,
            end_node,
            closest_seg_idx,
            RerouteMode::ClosestAngle(dir),
        ) {
            path_points.extend(next_points[1..].iter().cloned());
        }
    }

    if path_points.len() < 2 {
        return None;
    }

    Some((closest_seg_idx, path_points))
}

/// pick spawn candidate.
pub fn pick_spawn_candidate(
    graph: &TrafficRoadGraph,
    camera: &Camera,
    cam_gt: &GlobalTransform,
    radius: f32,
    min_dist: f32,
    spacing: f32,
    existing: &[Vec3],
    fast_fill: bool,
) -> Option<Vec3> {
    let num_segments = graph.segments.len();
    if num_segments == 0 {
        return None;
    }

    let camera_pos = cam_gt.translation();
    let cam_fwd = cam_gt.forward();

    for _ in 0..10 {
        let seg_idx = (rand::random::<f32>() * num_segments as f32) as usize;
        let seg = &graph.segments[seg_idx];
        if seg.points.is_empty() {
            continue;
        }
        let pt_idx = (rand::random::<f32>() * seg.points.len() as f32) as usize;
        let candidate_point = seg.points[pt_idx];

        let dist = camera_pos.distance(candidate_point);
        if dist > radius || dist < min_dist {
            continue;
        }

        if !fast_fill {
            // Reject if candidate is in front of the camera (behind/side check)
            let to_pt = (candidate_point - camera_pos).normalize_or_zero();
            if cam_fwd.dot(to_pt) >= super::SPAWN_BEHIND_MAX_DOT {
                continue;
            }

            // Check if inside frustum
            if let Some(ndc) = camera.world_to_ndc(cam_gt, candidate_point) {
                let inside_x = ndc.x >= -1.0 && ndc.x <= 1.0;
                let inside_y = ndc.y >= -1.0 && ndc.y <= 1.0;
                let inside_z = ndc.z >= 0.0 && ndc.z <= 1.0;
                if inside_x && inside_y && inside_z {
                    // Reject visible candidate
                    continue;
                }
            }
        }

        // Check distance to existing entities
        let mut too_close = false;
        for &pos in existing {
            if pos.distance(candidate_point) < spacing {
                too_close = true;
                break;
            }
        }
        if too_close {
            continue;
        }

        return Some(candidate_point);
    }

    None
}
