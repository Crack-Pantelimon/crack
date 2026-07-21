//! Debug gizmo rendering of pedestrian skeletons + the bone color mapping.

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;

use crate::plugins::pedestrians::skeleton::{
    BoneLabel, PedestrianSkeleton, traverse_hierarchy_raw,
};
use crate::plugins::pedestrians::spawn_pedestrian::ModelRoot;

/// Toggles skeleton gizmo drawing.
#[derive(Resource, Default)]
pub struct SkeletonDebug {
/// show field.
    pub show: bool,
}

/// Color used to draw a bone with the given classification label.
pub fn bone_color(label: BoneLabel) -> Color {
    match label {
        BoneLabel::Head | BoneLabel::Neck => Color::srgb(1.0, 0.4, 0.7), // Pink
        BoneLabel::Spine => Color::srgb(0.0, 0.0, 0.5),                  // Dark Blue
        BoneLabel::Midgroin => Color::srgb(1.0, 1.0, 0.0),               // Yellow
        BoneLabel::LeftShoulder => Color::srgb(0.5, 0.8, 1.0),           // Light Blue
        BoneLabel::RightShoulder => Color::srgb(1.0, 0.7, 0.85),         // Light Pink
        BoneLabel::LeftArm => Color::srgb(0.6, 0.2, 0.8),                // Purple
        BoneLabel::RightArm => Color::srgb(1.0, 0.6, 0.0),               // Orange
        BoneLabel::LeftHand => Color::srgb(0.6, 0.6, 0.0),               // Dark Yellow
        BoneLabel::RightHand => Color::srgb(1.0, 1.0, 0.5),              // Light Yellow
        BoneLabel::LeftLeg => Color::srgb(1.0, 0.2, 0.2),                // Red
        BoneLabel::RightLeg => Color::srgb(0.2, 1.0, 0.2),               // Green
        BoneLabel::LeftFoot => Color::srgb(0.8, 0.0, 0.8),               // Dark Purple/Magenta
        BoneLabel::RightFoot => Color::srgb(0.0, 0.8, 0.8),              // Light Purple/Teal
    }
}

/// draw skeletons system.
pub fn draw_skeletons_system(
    skeleton_debug: Res<SkeletonDebug>,
    mut gizmos: Gizmos,
    model_roots: Query<(Entity, &ModelRoot, &GlobalTransform)>,
    skeletons: Query<&PedestrianSkeleton>,
    children_query: Query<&Children>,
    name_query: Query<&Name>,
    transform_query: Query<&GlobalTransform>,
    parent_query: Query<&ChildOf>,
) {
    if !skeleton_debug.show {
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
            let color = label
                .map(|l| bone_color(*l))
                .unwrap_or(Color::srgb(0.5, 0.5, 0.5));

            if let Ok(parent) = parent_query.get(ent) {
                if let Some(&(_, parent_pos)) = entity_to_info.get(&parent.get()) {
                    gizmos.line(parent_pos, pos, color);
                }
            }
        }
    }
}
