//! Line-of-sight perception for AI pedestrians.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::plugins::pedestrians::{
    ModelRoot,
    pedestrian_controller_plugin::{CharacterController, DriverMesh, CAPSULE_HALF_HEIGHT},
    skeleton::PedestrianSkeleton,
};
use crate::plugins::weapons::weapon_shooting::is_person_entity;

use super::{AiPedestrian, AiPerception, faction::{Faction, Health, WarMatrix}};

/// Maximum distance at which an AI ped can perceive enemies.
const SIGHT_RANGE: f32 = 50.0;
/// Vertical offset from capsule center to the "head" (LOS origin/target).
const HEAD_OFFSET: f32 = CAPSULE_HALF_HEIGHT;

/// Refreshes [`AiPerception`] for each live AI pedestrian.
pub fn ai_perception(
    spatial_query: SpatialQuery,
    war: Res<WarMatrix>,
    mut ai_query: Query<
        (Entity, &GlobalTransform, &Faction, &mut AiPerception),
        With<AiPedestrian>,
    >,
    targets_query: Query<
        (Entity, &GlobalTransform, &Faction, &Health),
        With<CharacterController>,
    >,
    parents: Query<&ChildOf>,
    q_controller: Query<(), With<CharacterController>>,
    q_model: Query<(), With<ModelRoot>>,
    q_skel: Query<(), With<PedestrianSkeleton>>,
    q_driver: Query<(), With<DriverMesh>>,
) {
    // Collect candidate targets once (avoid borrow issues with mutable ai_query).
    let candidates: Vec<(Entity, Vec3, Faction, bool)> = targets_query
        .iter()
        .map(|(e, gt, f, h)| (e, gt.translation(), *f, h.current > 0.0))
        .collect();

    for (my_entity, my_gt, my_faction, mut perception) in &mut ai_query {
        let my_pos = my_gt.translation();
        let my_head = my_pos + Vec3::Y * HEAD_OFFSET;

        let mut best: Option<(Entity, Vec3, f32)> = None;

        // Sort candidates by distance (nearest first).
        let mut sorted: Vec<_> = candidates
            .iter()
            .filter(|(e, _, f, alive)| {
                *e != my_entity && *alive && war.at_war(*my_faction, *f)
            })
            .map(|(e, pos, _, _)| {
                let dist = my_pos.distance(*pos);
                (*e, *pos, dist)
            })
            .filter(|(_, _, dist)| *dist <= SIGHT_RANGE)
            .collect();
        sorted.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        for (candidate_entity, candidate_pos, dist) in sorted {
            let their_head = candidate_pos + Vec3::Y * HEAD_OFFSET;
            let ray_dir = (their_head - my_head).normalize_or_zero();
            let ray_len = dist + HEAD_OFFSET; // a bit of slack

            let Ok(ray_dir3) = Dir3::new(ray_dir) else {
                continue;
            };

            let filter = SpatialQueryFilter::from_excluded_entities([my_entity]);

            if let Some(hit) = spatial_query.cast_ray(my_head, ray_dir3, ray_len, true, &filter) {
                // Check if the first hit entity belongs to the candidate's subtree.
                let hit_is_candidate = is_person_entity(
                    hit.entity,
                    &parents,
                    &q_controller,
                    &q_model,
                    &q_skel,
                    &q_driver,
                );

                if hit_is_candidate {
                    // Visible — resolve entity chain up to find the controller.
                    // For simplicity, we check if traversing parents reaches the candidate.
                    let mut cur = hit.entity;
                    let mut matched = cur == candidate_entity;
                    if !matched {
                        loop {
                            match parents.get(cur) {
                                Ok(child_of) => {
                                    cur = child_of.parent();
                                    if cur == candidate_entity {
                                        matched = true;
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }

                    if matched {
                        best = Some((candidate_entity, their_head, dist));
                        perception.last_los = Some((my_head, their_head, true));
                        break; // nearest visible found
                    }
                }
                // Hit something else (a wall) — this candidate is not visible.
                perception.last_los = Some((my_head, my_head + ray_dir * hit.distance, false));
            } else {
                // Ray missed everything — target is visible (no obstructions).
                best = Some((candidate_entity, their_head, dist));
                perception.last_los = Some((my_head, their_head, true));
                break;
            }
        }

        if let Some((target, target_pos, target_dist)) = best {
            perception.target = Some(target);
            perception.target_pos = target_pos;
            perception.target_dist = target_dist;
            perception.visible = true;
        } else {
            perception.target = None;
            perception.visible = false;
        }
    }
}
