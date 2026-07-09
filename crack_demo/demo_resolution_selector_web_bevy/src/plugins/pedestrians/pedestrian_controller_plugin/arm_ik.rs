//! Post-animation procedural arm IK for gun aiming.
//!
//! Runs after Bevy's animation evaluation and before transform propagation so shoulder/elbow
//! rotations override the animated pose while the lower body keeps playing locomotion clips.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;

use super::{CharacterController, CombatKind, CombatState, ControlledCharacter};
use crate::plugins::cars_driving::driving_plugin::spawn_car::ActivePlayerVehicle;
use crate::plugins::pedestrians::pedestrian_controller_plugin::interaction_ui::DriverMesh;
use crate::plugins::pedestrians::skeleton::{ArmSide, PedestrianSkeleton};
use crate::plugins::states::GameControlState;
use crate::plugins::weapons::{EquippedWeapon, GunState};
use bevy_egui::EguiContexts;

/// Beyond this yaw offset from character forward, the spine is pre-rotated so the arm can reach.

/// Screen-center crosshair aim point (same convention as guns).
fn crosshair_target(cam: &GlobalTransform, spatial: &SpatialQuery) -> Vec3 {
    let origin = cam.translation();
    let dir = cam.forward();
    let filter = SpatialQueryFilter::default();
    if let Some(hit) = spatial.cast_ray(origin, dir, 500.0, true, &filter) {
        origin + *dir * hit.distance
    } else {
        origin + *dir * 100.0
    }
}

/// Horizontal yaw (radians, signed) the torso should add so the target sits in front.
/// Continuous: 0 when already facing the target, growing monotonically, clamped to a max.
fn torso_yaw_toward(char_forward: Vec3, to_target: Vec3, max_yaw: f32) -> f32 {
    let fwd = Vec3::new(char_forward.x, 0.0, char_forward.z).normalize_or_zero();
    let to = Vec3::new(to_target.x, 0.0, to_target.z).normalize_or_zero();
    if fwd.length_squared() < 1e-6 || to.length_squared() < 1e-6 {
        return 0.0;
    }
    let ang = Vec2::new(fwd.x, fwd.z).angle_to(Vec2::new(to.x, to.z)); // (-π, π]
    // Only compensate the part beyond the shoulder's comfortable ~70° cone.
    const COMFORT: f32 = 70.0 * std::f32::consts::PI / 180.0;
    let excess = (ang.abs() - COMFORT).max(0.0);
    excess.copysign(ang).clamp(-max_yaw, max_yaw)
}

/// Walk up to the root and compose world-space transform from local `Transform`s.
/// Needed because `GlobalTransform` is not updated until `TransformPropagate`.
fn world_transform(
    entity: Entity,
    transforms: &Query<&Transform>,
    parents: &Query<&ChildOf>,
) -> Option<Transform> {
    let mut chain = vec![entity];
    let mut cur = entity;
    while let Ok(child_of) = parents.get(cur) {
        cur = child_of.parent();
        chain.push(cur);
    }
    chain.reverse();
    let mut world = Transform::IDENTITY;
    for ent in chain {
        world = world.mul_transform(*transforms.get(ent).ok()?);
    }
    Some(world)
}

/// Walk up to the root and compose world-space transform from local `Transform`s,
/// consulting the overrides map.
fn world_transform_with_overrides(
    entity: Entity,
    transforms: &Query<&Transform>,
    parents: &Query<&ChildOf>,
    overrides: &std::collections::HashMap<Entity, Quat>,
) -> Option<Transform> {
    let mut chain = vec![entity];
    let mut cur = entity;
    while let Ok(child_of) = parents.get(cur) {
        cur = child_of.parent();
        chain.push(cur);
    }
    chain.reverse();
    let mut world = Transform::IDENTITY;
    for ent in chain {
        let mut local = *transforms.get(ent).ok()?;
        if let Some(&new_rot) = overrides.get(&ent) {
            local.rotation = new_rot;
        }
        world = world.mul_transform(local);
    }
    Some(world)
}

/// Two-bone IK: returns desired elbow and wrist world positions.
fn two_bone_ik_positions(
    shoulder: Vec3,
    elbow: Vec3,
    wrist: Vec3,
    target: Vec3,
    pole: Vec3,
) -> (Vec3, Vec3) {
    let upper_len = shoulder.distance(elbow).max(0.01);
    let lower_len = elbow.distance(wrist).max(0.01);
    let to_target = target - shoulder;
    let target_dist = to_target
        .length()
        .clamp(0.01, upper_len + lower_len - 0.001);
    let dir = to_target / target_dist;

    let cos_shoulder = ((upper_len * upper_len + target_dist * target_dist
        - lower_len * lower_len)
        / (2.0 * upper_len * target_dist))
        .clamp(-1.0, 1.0);
    let sin_shoulder = (1.0 - cos_shoulder * cos_shoulder).max(0.0).sqrt();

    let elbow_on_axis = shoulder + dir * (cos_shoulder * upper_len);

    let mut bend_dir = (elbow - elbow_on_axis).normalize_or_zero();
    if bend_dir.length_squared() < 1e-6 {
        let pole_rel = pole - shoulder;
        bend_dir = dir.cross(pole_rel.cross(dir)).normalize_or_zero();
        if bend_dir.length_squared() < 1e-6 {
            bend_dir = dir.cross(Vec3::Y).normalize_or_zero();
        }
    }

    let new_elbow = elbow_on_axis + bend_dir * (sin_shoulder * upper_len);
    let new_wrist = shoulder + dir * target_dist;
    (new_elbow, new_wrist)
}

/// `from_rotation_arc` that stays continuous through the antiparallel case by falling back to a
/// stable reference axis instead of glam's arbitrary pick.
fn safe_rotation_arc(from: Vec3, to: Vec3, fallback_axis: Vec3) -> Quat {
    let a = from.normalize_or_zero();
    let b = to.normalize_or_zero();
    if a.length_squared() < 1e-6 || b.length_squared() < 1e-6 {
        return Quat::IDENTITY;
    }
    let d = a.dot(b).clamp(-1.0, 1.0);
    if d < -0.9995 {
        let mut axis = a.cross(fallback_axis).normalize_or_zero();
        if axis.length_squared() < 1e-6 {
            axis = a.cross(Vec3::Y).normalize_or_zero();
        }
        if axis.length_squared() < 1e-6 {
            axis = a.cross(Vec3::X).normalize_or_zero();
        }
        return Quat::from_axis_angle(axis, std::f32::consts::PI);
    }
    Quat::from_rotation_arc(a, b)
}

/// Rotate a joint so its child moves toward `desired_child_pos`.
fn aim_joint_local_rotation(
    joint_world: &Transform,
    parent_world: &Transform,
    child_pos: Vec3,
    desired_child_pos: Vec3,
    fallback_axis: Vec3,
) -> Quat {
    let joint_pos = joint_world.translation;
    let current_dir = (child_pos - joint_pos).normalize_or_zero();
    let desired_dir = (desired_child_pos - joint_pos).normalize_or_zero();
    if current_dir.length_squared() < 1e-6 || desired_dir.length_squared() < 1e-6 {
        return joint_world.rotation;
    }
    let delta_world = safe_rotation_arc(current_dir, desired_dir, fallback_axis);
    let new_world_rot = delta_world * joint_world.rotation;
    parent_world.rotation.inverse() * new_world_rot
}

fn apply_arm_chain_ik(
    skeleton: &PedestrianSkeleton,
    arm: ArmSide,
    target: Vec3,
    pole: Vec3,
    char_forward: Vec3,
    char_up: Vec3,
    char_pos: Vec3,
    transforms: &Query<&Transform>,
    parents: &Query<&ChildOf>,
    gizmos: &mut Gizmos,
) -> Vec<(Entity, Quat)> {
    let Some((shoulder_ent, elbow_ent, wrist_ent)) = skeleton.arm_chain(arm) else {
        return Vec::new();
    };

    let mut rotations = Vec::new();
    let mut overrides = std::collections::HashMap::new();

    // 1. Spine compensation
    if let Some(spine_ent) = skeleton.spine {
        let to_target = target - char_pos;
        let yaw = torso_yaw_toward(char_forward, to_target, 60f32.to_radians());
        let spine_world = world_transform(spine_ent, transforms, parents);
        let parent_world = parents
            .get(spine_ent)
            .ok()
            .and_then(|c| world_transform(c.parent(), transforms, parents));
        if let (Some(spine_world), Some(parent_world)) = (spine_world, parent_world) {
            let new_world_rot = Quat::from_axis_angle(char_up, yaw) * spine_world.rotation;
            let local_rot = parent_world.rotation.inverse() * new_world_rot;
            rotations.push((spine_ent, local_rot));
            overrides.insert(spine_ent, local_rot);
        }
    }

    // 2. Solve two-bone IK positions
    let Some(shoulder_world) =
        world_transform_with_overrides(shoulder_ent, transforms, parents, &overrides)
    else {
        return rotations;
    };
    let Some(elbow_world) =
        world_transform_with_overrides(elbow_ent, transforms, parents, &overrides)
    else {
        return rotations;
    };
    let Some(wrist_world) =
        world_transform_with_overrides(wrist_ent, transforms, parents, &overrides)
    else {
        return rotations;
    };

    let shoulder_pos = shoulder_world.translation;
    let elbow_pos = elbow_world.translation;
    let wrist_pos = wrist_world.translation;

    let (new_elbow, new_wrist) =
        two_bone_ik_positions(shoulder_pos, elbow_pos, wrist_pos, target, pole);

    // 3. Solve shoulder rotation
    let parent_world_shoulder = parents
        .get(shoulder_ent)
        .ok()
        .and_then(|c| world_transform_with_overrides(c.parent(), transforms, parents, &overrides))
        .unwrap_or(Transform::IDENTITY);

    let shoulder_local_rot = aim_joint_local_rotation(
        &shoulder_world,
        &parent_world_shoulder,
        elbow_pos,
        new_elbow,
        char_up,
    );
    rotations.push((shoulder_ent, shoulder_local_rot));
    overrides.insert(shoulder_ent, shoulder_local_rot);

    // 4. Solve elbow rotation
    let Some(elbow_world_updated) =
        world_transform_with_overrides(elbow_ent, transforms, parents, &overrides)
    else {
        return rotations;
    };
    let parent_world_elbow = parents
        .get(elbow_ent)
        .ok()
        .and_then(|c| world_transform_with_overrides(c.parent(), transforms, parents, &overrides))
        .unwrap_or(Transform::IDENTITY);

    let wrist_pos_updated =
        world_transform_with_overrides(wrist_ent, transforms, parents, &overrides)
            .map(|t| t.translation)
            .unwrap_or(wrist_pos);

    let elbow_local_rot = aim_joint_local_rotation(
        &elbow_world_updated,
        &parent_world_elbow,
        wrist_pos_updated,
        new_wrist,
        char_up,
    );
    rotations.push((elbow_ent, elbow_local_rot));

    #[cfg(debug_assertions)]
    {
        // Forearm: new_elbow -> new_wrist
        gizmos.line(new_elbow, new_wrist, Color::srgb(0.0, 1.0, 0.0));
        // Aim line: new_wrist -> target
        gizmos.line(new_wrist, target, Color::srgb(1.0, 0.0, 0.0));
    }

    rotations
}

fn write_arm_ik_rotations(
    transform_sets: &mut ParamSet<(Query<&Transform>, Query<&mut Transform>)>,
    parents: &Query<&ChildOf>,
    skeleton: &PedestrianSkeleton,
    arm: ArmSide,
    aim_point: Vec3,
    pole: Vec3,
    char_forward: Vec3,
    char_up: Vec3,
    char_pos: Vec3,
    gizmos: &mut Gizmos,
) {
    let rotations = {
        let transforms = transform_sets.p0();
        apply_arm_chain_ik(
            skeleton,
            arm,
            aim_point,
            pole,
            char_forward,
            char_up,
            char_pos,
            &transforms,
            parents,
            gizmos,
        )
    };
    let mut transforms_mut = transform_sets.p1();
    for (ent, rot) in rotations {
        if let Ok(mut tf) = transforms_mut.get_mut(ent) {
            tf.rotation = rot;
        }
    }
}

/// Run after animation, before transform propagation.
pub fn apply_arm_ik(
    state: Res<State<GameControlState>>,
    controlled: Res<ControlledCharacter>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    skeletons: Query<&PedestrianSkeleton>,
    combat_states: Query<&CombatState>,
    equipped: Query<&EquippedWeapon>,
    gun_states: Query<&GunState>,
    controllers: Query<&GlobalTransform, With<CharacterController>>,
    driver_meshes: Query<(Entity, &DriverMesh, &GlobalTransform)>,
    cars: Query<(Entity, &GlobalTransform), With<ActivePlayerVehicle>>,
    spatial: SpatialQuery,
    parents: Query<&ChildOf>,
    mut transform_sets: ParamSet<(Query<&Transform>, Query<&mut Transform>)>,
    mut gizmos: Gizmos,
) {
    let Some(cam) = camera.iter().next() else {
        return;
    };
    let aim_point = crosshair_target(cam, &spatial);

    let over_ui = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
        .unwrap_or(false);
    let aiming = !over_ui && mouse.pressed(MouseButton::Right);

    match state.get() {
        GameControlState::ControllingPedestrian => {
            let Some(ped) = controlled.ped else {
                return;
            };
            let Some(controller) = controlled.controller else {
                return;
            };
            let Ok(weapon) = equipped.get(controller) else {
                return;
            };
            if !weapon.0.is_gun() {
                return;
            }
            if let Ok(gun) = gun_states.get(controller) {
                if gun.reload_timer > 0.0 {
                    return;
                }
            }
            let in_combat = combat_states
                .get(controller)
                .map(|c| c.kind != CombatKind::None)
                .unwrap_or(false);
            if !aiming && !in_combat {
                return;
            }
            let Ok(char_gt) = controllers.get(controller) else {
                return;
            };
            let Ok(skeleton) = skeletons.get(ped) else {
                return;
            };

            let char_forward = char_gt.rotation() * Vec3::NEG_Z;
            let char_up = char_gt.rotation() * Vec3::Y;
            let char_pos = char_gt.translation();
            let pole = char_pos - char_forward * 0.5 + Vec3::NEG_Y * 0.3;

            write_arm_ik_rotations(
                &mut transform_sets,
                &parents,
                skeleton,
                ArmSide::Right,
                aim_point,
                pole,
                char_forward,
                char_up,
                char_pos,
                &mut gizmos,
            );
        }
        GameControlState::DrivingCar => {
            if !aiming {
                return;
            }
            let Ok((car_entity, car_gt)) = cars.single() else {
                return;
            };
            let Some((driver_model, _, driver_gt)) =
                driver_meshes.iter().find(|(_, d, _)| d.car == car_entity)
            else {
                return;
            };
            let Ok(weapon) = equipped.get(driver_model) else {
                return;
            };
            if !weapon.0.is_gun() {
                return;
            }
            if let Ok(gun) = gun_states.get(driver_model) {
                if gun.reload_timer > 0.0 {
                    return;
                }
            }
            let Ok(skeleton) = skeletons.get(driver_model) else {
                return;
            };

            let car_right = car_gt.rotation() * Vec3::X;
            let to_target = aim_point - driver_gt.translation();
            let arm = if car_right.dot(to_target) >= 0.0 {
                ArmSide::Right
            } else {
                ArmSide::Left
            };

            let char_forward = driver_gt.rotation() * Vec3::NEG_Z;
            let char_up = driver_gt.rotation() * Vec3::Y;
            let char_pos = driver_gt.translation();
            let pole = char_pos - char_forward * 0.5 + Vec3::NEG_Y * 0.3;

            write_arm_ik_rotations(
                &mut transform_sets,
                &parents,
                skeleton,
                arm,
                aim_point,
                pole,
                char_forward,
                char_up,
                char_pos,
                &mut gizmos,
            );
        }
        _ => {}
    }
}
