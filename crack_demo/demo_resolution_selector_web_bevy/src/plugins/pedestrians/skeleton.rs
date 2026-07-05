//! Bone classification for pedestrian skeletons (no rendering).
//!
//! Given the raw joint hierarchy of a rigged model, this labels each joint with a
//! [`BoneLabel`] (head, spine, left arm, ...) using purely geometric heuristics.

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoneLabel {
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

#[derive(Component)]
pub struct PedestrianSkeleton {
    pub joint_labels: std::collections::HashMap<Entity, BoneLabel>,
    /// The right-wrist bone entity (a good attach point for a held weapon), if found.
    pub right_hand: Option<Entity>,
}

pub struct JointData {
    pub entity: Entity,
    pub _name: String,
    pub pos: Vec3,
    pub parent: Option<Entity>,
}

pub fn traverse_hierarchy_raw(
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

    let is_valid_joint = name_str.starts_with("bone_") || name_str == "Armature";

    if is_valid_joint {
        nodes.push((entity, name_str, pos));
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            traverse_hierarchy_raw(child, children_query, name_query, transform_query, nodes);
        }
    }
}

pub fn classify_skeleton(
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

    let left_arm_info = classify_limb_path(
        left_hand_tip_entity,
        &spine_path,
        root_entity,
        joints,
        &mut labels,
        BoneLabel::LeftArm,
        BoneLabel::LeftShoulder,
        BoneLabel::LeftHand,
    );
    let right_arm_info = classify_limb_path(
        right_hand_tip_entity,
        &spine_path,
        root_entity,
        joints,
        &mut labels,
        BoneLabel::RightArm,
        BoneLabel::RightShoulder,
        BoneLabel::RightHand,
    );

    let _left_leg_info = classify_limb_path(
        left_heel_entity,
        &spine_path,
        root_entity,
        joints,
        &mut labels,
        BoneLabel::LeftLeg,
        BoneLabel::Midgroin,
        BoneLabel::LeftFoot,
    );
    let _right_leg_info = classify_limb_path(
        right_heel_entity,
        &spine_path,
        root_entity,
        joints,
        &mut labels,
        BoneLabel::RightLeg,
        BoneLabel::Midgroin,
        BoneLabel::RightFoot,
    );

    let (left_shoulder, left_elbow, left_wrist) = match left_arm_info {
        Some((s, e, w)) => (Some(s), Some(e), Some(w)),
        None => (None, None, None),
    };

    let (right_shoulder, right_elbow, right_wrist) = match right_arm_info {
        Some((s, e, w)) => (Some(s), Some(e), Some(w)),
        None => (None, None, None),
    };

    (
        labels,
        left_shoulder,
        left_elbow,
        left_wrist,
        right_shoulder,
        right_elbow,
        right_wrist,
    )
}

pub fn find_parent_of(entity: Entity, joints: &[JointData]) -> Option<Entity> {
    for joint in joints {
        if joint.entity == entity {
            return joint.parent;
        }
    }
    None
}

pub fn find_pos_of(entity: Entity, joints: &[JointData]) -> Option<Vec3> {
    for joint in joints {
        if joint.entity == entity {
            return Some(joint.pos);
        }
    }
    None
}

pub fn classify_limb_path(
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
