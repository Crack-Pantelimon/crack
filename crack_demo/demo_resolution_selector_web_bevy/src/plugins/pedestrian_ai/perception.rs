//! Line-of-sight perception for AI pedestrians.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::plugins::cars_driving::driving_plugin::spawn_car::{Car, DisabledCar};
use crate::plugins::pedestrians::pedestrian_controller_plugin::{
    CharacterController, DriverMesh, CAPSULE_HALF_HEIGHT,
};

use super::{AiPedestrian, AiPerception, AiThink, faction::{Enemies, Faction, Health, WarMatrix}};

/// Maximum distance at which an AI ped can perceive enemies.
const SIGHT_RANGE: f32 = 50.0;
/// Vertical offset from capsule center to the "head" (LOS origin/target).
const HEAD_OFFSET: f32 = CAPSULE_HALF_HEIGHT;
/// Approx. LOS aim height for a car body.
const CAR_AIM_OFFSET: f32 = 0.8;

/// A candidate the AI might target: another pedestrian or an enemy-driven car.
struct Candidate {
    entity: Entity,
    pos: Vec3,
    is_car: bool,
}

/// Refreshes [`AiPerception`] for each live AI pedestrian. Per-entity throttled via [`AiThink`] so
/// the O(N^2) raycasts only run a few times a second per ped.
#[allow(clippy::too_many_arguments)]
pub fn ai_perception(
    spatial_query: SpatialQuery,
    war: Res<WarMatrix>,
    mut ai_query: Query<
        (Entity, &GlobalTransform, &Faction, &Enemies, &AiThink, &mut AiPerception),
        With<AiPedestrian>,
    >,
    targets_query: Query<
        (Entity, &GlobalTransform, &Faction, &Health),
        With<CharacterController>,
    >,
    q_cars: Query<(Entity, &GlobalTransform, &Children), (With<Car>, Without<DisabledCar>)>,
    q_driver_faction: Query<&Faction, With<DriverMesh>>,
    parents: Query<&ChildOf>,
) {
    // Collect pedestrian candidates once (avoid borrow issues with mutable ai_query).
    let ped_candidates: Vec<(Entity, Vec3, Faction, bool)> = targets_query
        .iter()
        .map(|(e, gt, f, h)| (e, gt.translation(), *f, h.current > 0.0))
        .collect();

    // Collect car candidates: cars carrying a driver, tagged with that driver's faction.
    let car_candidates: Vec<(Entity, Vec3, Faction)> = q_cars
        .iter()
        .filter_map(|(car, gt, children)| {
            children
                .iter()
                .find_map(|child| q_driver_faction.get(child).ok().copied())
                .map(|faction| (car, gt.translation(), faction))
        })
        .collect();

    for (my_entity, my_gt, my_faction, my_enemies, think, mut perception) in &mut ai_query {
        // Throttle: keep last frame's perception until this ped is due to think again.
        if !think.ready {
            continue;
        }

        let my_pos = my_gt.translation();
        let my_head = my_pos + Vec3::Y * HEAD_OFFSET;

        // Build a hostile candidate list: at-war factions plus anyone on the personal grudge list.
        let is_hostile = |e: Entity, f: Faction| war.at_war(*my_faction, f) || my_enemies.0.contains(&e);

        let mut candidates: Vec<Candidate> = Vec::new();
        for (e, pos, f, alive) in &ped_candidates {
            if *e == my_entity || !*alive || !is_hostile(*e, *f) {
                continue;
            }
            candidates.push(Candidate { entity: *e, pos: *pos, is_car: false });
        }
        for (e, pos, f) in &car_candidates {
            if !is_hostile(*e, *f) {
                continue;
            }
            candidates.push(Candidate { entity: *e, pos: *pos, is_car: true });
        }

        // Sort nearest-first and cull beyond sight range.
        let mut sorted: Vec<(usize, f32)> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, my_pos.distance(c.pos)))
            .filter(|(_, dist)| *dist <= SIGHT_RANGE)
            .collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut best: Option<(Entity, Vec3, f32, bool)> = None;

        for (idx, dist) in sorted {
            let c = &candidates[idx];
            let aim_off = if c.is_car { CAR_AIM_OFFSET } else { HEAD_OFFSET };
            let their_head = c.pos + Vec3::Y * aim_off;
            let ray_dir = (their_head - my_head).normalize_or_zero();
            let ray_len = dist + HEAD_OFFSET; // a bit of slack

            let Ok(ray_dir3) = Dir3::new(ray_dir) else {
                continue;
            };

            let filter = SpatialQueryFilter::from_excluded_entities([my_entity]);

            if let Some(hit) = spatial_query.cast_ray(my_head, ray_dir3, ray_len, true, &filter) {
                // Visible only if the first thing the ray hits belongs to the candidate's subtree
                // (walking colliders up their parent chain to the candidate root). A wall in front
                // breaks the chain and reads as occluded.
                let mut cur = hit.entity;
                let mut matched = cur == c.entity;
                if !matched {
                    loop {
                        match parents.get(cur) {
                            Ok(child_of) => {
                                cur = child_of.parent();
                                if cur == c.entity {
                                    matched = true;
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }

                if matched {
                    best = Some((c.entity, their_head, dist, c.is_car));
                    perception.last_los = Some((my_head, their_head, true));
                    break; // nearest visible found
                }
                perception.last_los = Some((my_head, my_head + ray_dir * hit.distance, false));
            } else {
                best = Some((c.entity, their_head, dist, c.is_car));
                perception.last_los = Some((my_head, their_head, true));
                break;
            }
        }

        if let Some((target, target_pos, target_dist, is_car)) = best {
            perception.target = Some(target);
            perception.target_pos = target_pos;
            perception.target_dist = target_dist;
            perception.target_is_car = is_car;
            perception.visible = true;
        } else {
            perception.target = None;
            perception.visible = false;
            perception.target_is_car = false;
        }
    }
}
