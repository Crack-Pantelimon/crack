//! Physics ragdoll for pedestrians.
//!
//! Pipeline (all reusable engine logic):
//! 1. [`compute_ragdoll_constraints_system`] — reacts to a freshly-loaded pedestrian
//!    (`Added<PedestrianSkeleton>`), walks its bones, samples every animation frame to derive each
//!    bone's min/max distance-to-parent and min/max local euler angle per axis, stores those on the
//!    bone entities, and tags the root `RagdollComputed`.
//! 2. [`spawn_ragdoll_bodies_system`] — the frame after, spawns a sphere [`Collider`] +
//!    [`RigidBody`] per bone and a [`SphericalJoint`] per bone→parent link, all **disabled** at
//!    first. Tags the root `RagdollReady`.
//! 3. [`ragdoll_toggle_system`] — enables the bodies/joints (and disables the mesh colliders +
//!    animation) while the pedestrian is in the special [`RAGDOLL_ANIMATION`]; reverses it on exit.

use avian3d::prelude::*;
use bevy::animation::{AnimationTargetId, animated_field};
use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;

use crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer;
use crate::plugins::pedestrians::animation::TargetAnimation;
use crate::plugins::pedestrians::skeleton::{BoneLabel, PedestrianSkeleton};
use crate::plugins::pedestrians::spawn_pedestrian::{ModelRoot, PedestrianGltf};

/// The special animation name that puts a pedestrian into ragdoll physics mode.
pub const RAGDOLL_ANIMATION: &str = "ragdoll";

/// Upper cap on a node sphere's radius, as a fraction of the model's height.
pub const RAGDOLL_COLLIDER_RADIUS_FRAC: f32 = 0.04;

/// Total ragdoll mass in kg, split equally across all colliders (spheres + tubes) of a model.
pub const TOTAL_MASS: f32 = 90.0;

/// Bones shorter than this fraction of model height get no tube collider.
pub const MIN_BONE_LEN_FRAC: f32 = 0.01;

/// Which classified bones get physics. Arms/shoulders (to the wrist), legs, hips, spine, neck,
/// and the head (sphere only). Hands, feet, and unclassified bones are excluded.
fn is_important(label: BoneLabel) -> bool {
    matches!(
        label,
        BoneLabel::Head
            | BoneLabel::Neck
            | BoneLabel::Spine
            | BoneLabel::Midgroin
            | BoneLabel::LeftShoulder
            | BoneLabel::RightShoulder
            | BoneLabel::LeftArm
            | BoneLabel::RightArm
            | BoneLabel::LeftLeg
            | BoneLabel::RightLeg
    )
}

/// Per-bone constraint envelope derived from the model's animations, for important bones only.
#[derive(Component, Clone, Debug)]
pub struct RagdollBoneConstraint {
    pub root: Entity,
    pub label: BoneLabel,
    /// Nearest ancestor that is also an important bone (the joint/tube parent).
    pub parent_bone: Option<Entity>,
    /// This bone's rest transform relative to the model root (for anchors + tube geometry).
    pub rest_model_transform: Transform,
    pub min_angle: Vec3,
    pub max_angle: Vec3,
}

/// Marker on the model root once its bone constraints have been computed.
#[derive(Component)]
pub struct RagdollComputed;

/// Marker on the model root once its physics bodies + joints have been spawned (disabled).
#[derive(Component)]
pub struct RagdollReady;

/// Marker on each per-bone ragdoll body (carries no collider itself; colliders are children).
#[derive(Component)]
pub struct RagdollBone;

/// Marker on each ragdoll collider child entity (a node sphere or a bone tube).
#[derive(Component)]
pub struct RagdollCollider;

/// The joint entities belonging to a model root.
#[derive(Component)]
pub struct RagdollJoints(pub Vec<Entity>);

/// Marker on the model root while ragdoll physics is active.
#[derive(Component)]
pub struct RagdollActive;

/// Transient marker: bodies were just enabled; enable the joints one frame later so avian has
/// registered the bodies into islands first (enabling both at once panics the island solver).
#[derive(Component)]
pub struct RagdollJointsPending;

fn collect_descendants(root: Entity, children_query: &Query<&Children>, out: &mut Vec<Entity>) {
    if let Ok(children) = children_query.get(root) {
        for child in children.iter() {
            out.push(child);
            collect_descendants(child, children_query, out);
        }
    }
}

pub fn compute_ragdoll_constraints_system(
    mut commands: Commands,
    // Not `Added<..>`: bones (with `AnimationTargetId`) can appear a frame or two after the
    // skeleton is classified, so we retry each frame until they exist, then mark `RagdollComputed`.
    new_models: Query<
        (Entity, &PedestrianGltf, &PedestrianSkeleton),
        Without<RagdollComputed>,
    >,
    bone_query: Query<(&AnimationTargetId, &GlobalTransform)>,
    parent_query: Query<&ChildOf>,
    root_transform: Query<&GlobalTransform>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    clip_assets: Res<Assets<AnimationClip>>,
) {
    for (root, ped_gltf, skeleton) in new_models.iter() {
        let Some(gltf) = gltf_assets.get(&ped_gltf.handle) else {
            continue;
        };
        let Ok(root_global) = root_transform.get(root) else {
            continue;
        };

        // Important bones = classified bones whose label we want physics for, that also carry an
        // `AnimationTargetId` (so we can sample their clips) and a global transform.
        let important: std::collections::HashMap<Entity, BoneLabel> = skeleton
            .joint_labels
            .iter()
            .filter(|(e, label)| is_important(**label) && bone_query.get(**e).is_ok())
            .map(|(e, label)| (*e, *label))
            .collect();

        if important.is_empty() {
            // Bones/targets not spawned yet; retry next frame.
            continue;
        }

        let clips: Vec<&AnimationClip> = gltf
            .animations
            .iter()
            .filter_map(|h| clip_assets.get(h))
            .collect();

        for (&bone, &label) in &important {
            let Ok((target_id, bone_global)) = bone_query.get(bone) else {
                continue;
            };
            let rest_model_transform = bone_global.reparented_to(root_global);
            let rest_euler = Vec3::from(rest_model_transform.rotation.to_euler(EulerRot::XYZ));

            let mut min_angle = rest_euler;
            let mut max_angle = rest_euler;
            for clip in &clips {
                let duration = clip.duration();
                let frames = ((duration * 30.0).ceil() as usize).clamp(2, 120);
                for f in 0..frames {
                    let t = if frames > 1 {
                        duration * (f as f32 / (frames - 1) as f32)
                    } else {
                        0.0
                    };
                    // Local rotation vs. parent bone (already relative in a `Transform` curve).
                    let rotation = clip
                        .sample_clamped(animated_field!(Transform::rotation), *target_id, t)
                        .unwrap_or(rest_model_transform.rotation);
                    let euler = Vec3::from(rotation.to_euler(EulerRot::XYZ));
                    min_angle = min_angle.min(euler);
                    max_angle = max_angle.max(euler);
                }
            }

            // Nearest ancestor that is also an important bone.
            let mut parent_bone = None;
            let mut cur = bone;
            while let Ok(child_of) = parent_query.get(cur) {
                let p = child_of.get();
                if important.contains_key(&p) {
                    parent_bone = Some(p);
                    break;
                }
                cur = p;
            }

            commands.entity(bone).insert(RagdollBoneConstraint {
                root,
                label,
                parent_bone,
                rest_model_transform,
                min_angle,
                max_angle,
            });
        }

        commands.entity(root).insert(RagdollComputed);
    }
}

fn sphere_volume(r: f32) -> f32 {
    4.0 / 3.0 * std::f32::consts::PI * r * r * r
}

fn capsule_volume(r: f32, seg_len: f32) -> f32 {
    // Cylinder body + two hemispherical caps (= one sphere).
    std::f32::consts::PI * r * r * seg_len + 4.0 / 3.0 * std::f32::consts::PI * r * r * r
}

pub fn spawn_ragdoll_bodies_system(
    mut commands: Commands,
    new_roots: Query<(Entity, &ModelRoot), Added<RagdollComputed>>,
    bones: Query<(Entity, &RagdollBoneConstraint)>,
) {
    for (root, model_root) in new_roots.iter() {
        let height = model_root.size.y;
        let radius_cap = height * RAGDOLL_COLLIDER_RADIUS_FRAC;
        let min_bone_len = height * MIN_BONE_LEN_FRAC;

        // This model's important bones (owned so we don't fight the borrow checker across maps).
        let node_list: Vec<(Entity, RagdollBoneConstraint)> = bones
            .iter()
            .filter(|(_, c)| c.root == root)
            .map(|(e, c)| (e, c.clone()))
            .collect();
        if node_list.is_empty() {
            continue;
        }

        let pos: std::collections::HashMap<Entity, Vec3> = node_list
            .iter()
            .map(|(e, c)| (*e, c.rest_model_transform.translation))
            .collect();

        // Node sphere radius: capped by model height, and small enough to leave room for a tube
        // to the nearest neighbour node (so two adjacent spheres don't overlap).
        let mut radius: std::collections::HashMap<Entity, f32> = std::collections::HashMap::new();
        for (e, _) in &node_list {
            let p = pos[e];
            let mut min_dist = f32::MAX;
            for (o, _) in &node_list {
                if o != e {
                    min_dist = min_dist.min(p.distance(pos[o]));
                }
            }
            let r = if min_dist.is_finite() {
                radius_cap.min(min_dist / 3.0)
            } else {
                radius_cap
            };
            radius.insert(*e, r.max(1.0e-4));
        }

        // Plan tubes: one per link between a node and its important parent, except the head
        // (sphere only) and links shorter than `MIN_BONE_LEN_FRAC` of the model height.
        struct TubePlan {
            child: Entity,
            r_tube: f32,
            a: Vec3,
            b: Vec3,
        }
        let mut tubes: Vec<TubePlan> = Vec::new();
        for (e, c) in &node_list {
            if c.label == BoneLabel::Head {
                continue;
            }
            let Some(parent) = c.parent_bone else {
                continue;
            };
            let Some(&p_pos) = pos.get(&parent) else {
                continue;
            };
            let b_pos = pos[e];
            let bone_len = b_pos.distance(p_pos);
            if bone_len < min_bone_len {
                continue;
            }
            let r_b = radius[e];
            let r_p = radius[&parent];
            if bone_len - r_b - r_p <= 0.0 {
                continue;
            }
            // Endpoints expressed in the child bone's local frame.
            let b_inv = c.rest_model_transform.compute_affine().inverse();
            let p_local = b_inv.transform_point3(p_pos);
            let dir = p_local.normalize_or_zero();
            tubes.push(TubePlan {
                child: *e,
                r_tube: r_b.min( r_p),
                a: dir * r_p.max(r_b),
                b: dir * (bone_len - r_b - r_p),
            });
        }

        // Equal mass for every component (each node sphere + each bone tube).
        let n_components = node_list.len() + tubes.len();
        let mass_per = TOTAL_MASS / n_components as f32;

        // Spheres are `Bone1`, tubes are `Bone2`: each class self-collides (+ ground), but spheres
        // never collide tubes (they overlap at shared nodes by construction).
        let sphere_layers = CollisionLayers::new(
            [GamePhysicsLayer::Bone1],
            [GamePhysicsLayer::Map, GamePhysicsLayer::Bone1],
        );
        let tube_layers = CollisionLayers::new(
            [GamePhysicsLayer::Bone2],
            [GamePhysicsLayer::Map, GamePhysicsLayer::Bone2],
        );

        // Node bodies + their sphere colliders (as child entities so a body can carry both a
        // sphere and a tube; avian sums child-collider masses into the body).
        for (e, _c) in &node_list {
            let r = radius[e];
            commands.entity(*e).insert((
                RagdollBone,
                RigidBody::Dynamic,
                RigidBodyDisabled,
                LinearDamping(0.8),
                AngularDamping(2.0),
                MaxLinearSpeed(6.0),
                MaxAngularSpeed(15.0),
            ));
            commands.spawn((
                ChildOf(*e),
                Transform::IDENTITY,
                Collider::cuboid(r*2.0, r*2.0,r*2.0),
                ColliderDensity(mass_per / sphere_volume(r)),
                sphere_layers,
                ColliderDisabled,
                RagdollCollider,
            ));
        }

        // Bone tube colliders.
        for t in &tubes {
            let seg_len = (t.b - t.a).length();
            commands.spawn((
                ChildOf(t.child),
                Transform::IDENTITY,
                Collider::capsule_endpoints(t.r_tube, t.a, t.b),
                ColliderDensity(mass_per / capsule_volume(t.r_tube, seg_len)),
                tube_layers,
                ColliderDisabled,
                RagdollCollider,
            ));
        }

        // Joints between each node and its important parent.
        let rest_tf: std::collections::HashMap<Entity, Transform> = node_list
            .iter()
            .map(|(e, c)| (*e, c.rest_model_transform))
            .collect();
        let mut joint_entities = Vec::new();
        for (e, c) in &node_list {
            let Some(parent) = c.parent_bone else {
                continue;
            };
            let Some(parent_tf) = rest_tf.get(&parent) else {
                continue;
            };
            // Anchor on the parent body = this node's origin expressed in the parent's local frame.
            let anchor1 = parent_tf.compute_affine().inverse().transform_point3(pos[e]);

            let swing = ((c.max_angle.x - c.min_angle.x).max(c.max_angle.z - c.min_angle.z) * 0.5
                + 0.15)
                .clamp(0.05, std::f32::consts::PI);
            let twist_min = c.min_angle.y - 0.15;
            let twist_max = c.max_angle.y + 0.15;

            let joint = commands
                .spawn((
                    SphericalJoint::new(parent, *e)
                        .with_local_anchor1(anchor1)
                        .with_local_anchor2(Vec3::ZERO)
                        .with_swing_limits(-swing, swing)
                        .with_twist_limits(twist_min, twist_max)
                        .with_point_compliance(1.0e-5)
                        .with_swing_compliance(1.0e-4)
                        .with_twist_compliance(1.0e-4),
                    JointDisabled,
                ))
                .id();
            joint_entities.push(joint);
        }

        commands
            .entity(root)
            .insert((RagdollJoints(joint_entities), RagdollReady));
    }
}

/// Enables/disables ragdoll physics based on each ready pedestrian's target animation.
pub fn ragdoll_toggle_system(
    mut commands: Commands,
    roots: Query<
        (
            Entity,
            Option<&TargetAnimation>,
            &RagdollJoints,
            Has<RagdollActive>,
        ),
        With<RagdollReady>,
    >,
    children_query: Query<&Children>,
    bone_bodies: Query<(), With<RagdollBone>>,
    ragdoll_colliders: Query<(), With<RagdollCollider>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for (root, target, joints, is_active) in roots.iter() {
        let want_ragdoll = target
            .map(|t| t.name == RAGDOLL_ANIMATION)
            .unwrap_or(false);

        if want_ragdoll == is_active {
            continue;
        }

        let mut descendants = Vec::new();
        collect_descendants(root, &children_query, &mut descendants);

        if want_ragdoll {
            // Enable bone bodies + their colliders now; stop the animation player.
            // Joints are enabled a frame later (see `RagdollJointsPending`).
            for &e in &descendants {
                if bone_bodies.get(e).is_ok() {
                    commands.entity(e).remove::<RigidBodyDisabled>();
                }
                if ragdoll_colliders.get(e).is_ok() {
                    commands.entity(e).remove::<ColliderDisabled>();
                }
                if let Ok(mut player) = players.get_mut(e) {
                    player.stop_all();
                }
            }
            commands
                .entity(root)
                .insert((RagdollActive, RagdollJointsPending));
        } else {
            // Re-disable ragdoll physics; animation resumes.
            for &e in &descendants {
                if bone_bodies.get(e).is_ok() {
                    commands.entity(e).insert(RigidBodyDisabled);
                }
                if ragdoll_colliders.get(e).is_ok() {
                    commands.entity(e).insert(ColliderDisabled);
                }
            }
            for &j in &joints.0 {
                commands.entity(j).insert(JointDisabled);
            }
            commands
                .entity(root)
                .remove::<RagdollActive>()
                .remove::<RagdollJointsPending>();
        }
    }
}

/// Enables ragdoll joints the frame after their bodies were enabled, once avian has registered
/// the bodies into physics islands.
pub fn ragdoll_enable_joints_system(
    mut commands: Commands,
    pending: Query<(Entity, &RagdollJoints), (With<RagdollActive>, With<RagdollJointsPending>)>,
) {
    for (root, joints) in pending.iter() {
        for &j in &joints.0 {
            commands.entity(j).remove::<JointDisabled>();
        }
        commands.entity(root).remove::<RagdollJointsPending>();
    }
}
