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
use crate::plugins::pedestrians::skeleton::PedestrianSkeleton;
use crate::plugins::pedestrians::spawn_pedestrian::{ModelRoot, PedestrianGltf};

/// The special animation name that puts a pedestrian into ragdoll physics mode.
pub const RAGDOLL_ANIMATION: &str = "ragdoll";

/// Sphere collider radius per bone, as a fraction of the model's height.
pub const RAGDOLL_COLLIDER_RADIUS_FRAC: f32 = 0.05;

/// Per-bone constraint envelope derived from the model's animations.
#[derive(Component, Clone, Debug)]
pub struct RagdollBoneConstraint {
    pub root: Entity,
    pub parent_bone: Option<Entity>,
    pub rest_translation: Vec3,
    pub min_dist: f32,
    pub max_dist: f32,
    pub min_angle: Vec3,
    pub max_angle: Vec3,
}

/// Marker on the model root once its bone constraints have been computed.
#[derive(Component)]
pub struct RagdollComputed;

/// Marker on the model root once its physics bodies + joints have been spawned (disabled).
#[derive(Component)]
pub struct RagdollReady;

/// Marker on each per-bone ragdoll body.
#[derive(Component)]
pub struct RagdollBone;

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
        (Entity, &PedestrianGltf),
        (With<PedestrianSkeleton>, Without<RagdollComputed>),
    >,
    children_query: Query<&Children>,
    bone_query: Query<(&AnimationTargetId, &Transform)>,
    parent_query: Query<&ChildOf>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    clip_assets: Res<Assets<AnimationClip>>,
) {
    for (root, ped_gltf) in new_models.iter() {
        let Some(gltf) = gltf_assets.get(&ped_gltf.handle) else {
            continue;
        };

        // Gather every animated bone under this model.
        let mut descendants = Vec::new();
        collect_descendants(root, &children_query, &mut descendants);
        let bone_set: std::collections::HashSet<Entity> = descendants
            .iter()
            .copied()
            .filter(|e| bone_query.get(*e).is_ok())
            .collect();

        if bone_set.is_empty() {
            // Bones not spawned yet; retry next frame (query has no `RagdollComputed` filter).
            continue;
        }

        // Resolve the animation clips for this model.
        let clips: Vec<&AnimationClip> = gltf
            .animations
            .iter()
            .filter_map(|h| clip_assets.get(h))
            .collect();

        for &bone in &bone_set {
            let Ok((target_id, rest)) = bone_query.get(bone) else {
                continue;
            };
            let rest_translation = rest.translation;
            let rest_euler = Vec3::from(rest.rotation.to_euler(EulerRot::XYZ));

            let mut min_dist = rest_translation.length();
            let mut max_dist = min_dist;
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
                    let translation = clip
                        .sample_clamped(animated_field!(Transform::translation), *target_id, t)
                        .unwrap_or(rest_translation);
                    let rotation = clip
                        .sample_clamped(animated_field!(Transform::rotation), *target_id, t)
                        .unwrap_or(rest.rotation);

                    let dist = translation.length();
                    min_dist = min_dist.min(dist);
                    max_dist = max_dist.max(dist);

                    let euler = Vec3::from(rotation.to_euler(EulerRot::XYZ));
                    min_angle = min_angle.min(euler);
                    max_angle = max_angle.max(euler);
                }
            }

            // Nearest ancestor that is also a bone in this model.
            let mut parent_bone = None;
            let mut cur = bone;
            while let Ok(child_of) = parent_query.get(cur) {
                let p = child_of.get();
                if bone_set.contains(&p) {
                    parent_bone = Some(p);
                    break;
                }
                cur = p;
            }

            commands.entity(bone).insert(RagdollBoneConstraint {
                root,
                parent_bone,
                rest_translation,
                min_dist,
                max_dist,
                min_angle,
                max_angle,
            });
        }

        commands.entity(root).insert(RagdollComputed);
    }
}

pub fn spawn_ragdoll_bodies_system(
    mut commands: Commands,
    new_roots: Query<(Entity, &ModelRoot), Added<RagdollComputed>>,
    bones: Query<(Entity, &RagdollBoneConstraint)>,
) {
    for (root, model_root) in new_roots.iter() {
        let radius = (model_root.size.y * RAGDOLL_COLLIDER_RADIUS_FRAC);

        // Ragdoll bones collide only with the ground (Map), not each other or vehicles.
        let bone_layers = CollisionLayers::new([GamePhysicsLayer::Wheel], [GamePhysicsLayer::Map]);

        let mut joint_entities = Vec::new();

        for (bone, c) in bones.iter() {
            if c.root != root {
                continue;
            }

            commands.entity(bone).insert((
                RagdollBone,
                RigidBody::Dynamic,
                RigidBodyDisabled,
                Collider::sphere(radius),
                ColliderDisabled,
                bone_layers,
                // Denser than the default so the small spheres carry sane mass, plus damping and
                // hard speed caps to keep the constraint solver from launching the ragdoll.
                ColliderDensity(1000.0),
                LinearDamping(0.8),
                AngularDamping(2.0),
                MaxLinearSpeed(6.0),
                MaxAngularSpeed(15.0),
            ));

            if let Some(parent) = c.parent_bone {
                // Swing (about axes perpendicular to the bone) and twist (about Y) limits from
                // the animated range of motion, with a small margin so the pose isn't locked.
                let swing = ((c.max_angle.x - c.min_angle.x).max(c.max_angle.z - c.min_angle.z)
                    * 0.5
                    + 0.15)
                    .clamp(0.05, std::f32::consts::PI);
                let twist_min = c.min_angle.y - 0.15;
                let twist_max = c.max_angle.y + 0.15;

                let joint = commands
                    .spawn((
                        SphericalJoint::new(parent, bone)
                            .with_local_anchor1(c.rest_translation)
                            .with_local_anchor2(Vec3::ZERO)
                            .with_swing_limits(-swing, swing)
                            .with_twist_limits(twist_min, twist_max)
                            // Slightly soft point + springy limits so corrections don't explode.
                            .with_point_compliance(1.0e-5)
                            .with_swing_compliance(1.0e-4)
                            .with_twist_compliance(1.0e-4),
                        JointDisabled,
                    ))
                    .id();
                joint_entities.push(joint);
            }
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
    mesh_colliders: Query<(), (With<Collider>, Without<RagdollBone>)>,
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
            // Enable bone bodies now; disable the static mesh colliders and stop animation.
            // Joints are enabled a frame later (see `RagdollJointsPending`).
            let _ = &joints;
            for &e in &descendants {
                if bone_bodies.get(e).is_ok() {
                    commands
                        .entity(e)
                        .remove::<RigidBodyDisabled>()
                        .remove::<ColliderDisabled>();
                } else if mesh_colliders.get(e).is_ok() {
                    commands.entity(e).insert(ColliderDisabled);
                }
                if let Ok(mut player) = players.get_mut(e) {
                    player.stop_all();
                }
            }
            commands
                .entity(root)
                .insert((RagdollActive, RagdollJointsPending));
        } else {
            // Re-disable ragdoll physics; re-enable mesh colliders and let animation resume.
            for &e in &descendants {
                if bone_bodies.get(e).is_ok() {
                    commands
                        .entity(e)
                        .insert(RigidBodyDisabled)
                        .insert(ColliderDisabled);
                } else if mesh_colliders.get(e).is_ok() {
                    commands.entity(e).remove::<ColliderDisabled>();
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
