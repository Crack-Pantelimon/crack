//! Animation driver for the controlled pedestrian.
//!
//! Unlike the shared `play_animations_system` (which hard-switches a single clip), this drives the
//! model's [`AnimationPlayer`] directly so a **base locomotion clip** and a **combat overlay clip**
//! can play at the same time:
//! - base: idle / walk / jog / sprint / crouch / jump, chosen from controller state;
//! - overlay: LMB jab (one-shot), RMB-hold aim (loop), LMB while RMB held = shoot (one-shot).
//!
//! The controlled model carries [`ManualAnimation`] so the shared system leaves its player alone.

use bevy::{ecs::query::Has, prelude::*};
use bevy_egui::EguiContexts;

use super::*;
use crate::plugins::pedestrians::PedestrianAnimations;
use spawn::ControlledCharacter;

/// Base weight while a combat overlay is active, so the overlay reads on top of locomotion.
const BASE_WEIGHT_WITH_COMBAT: f32 = 0.6;

/// Logs the animation catalog once it is ready, so the exact clip names are visible.
pub fn print_animation_catalog(anims: Res<PedestrianAnimations>, mut done: Local<bool>) {
    if *done || !anims.ready {
        return;
    }
    info!(
        "=== Pedestrian animation catalog ({}) ===",
        anims.catalog.len()
    );
    for (name, info) in &anims.catalog {
        info!(
            "  {:<24} duration={:.2}s frames={}",
            name, info.duration, info.frames
        );
    }
    *done = true;
}

/// Returns the graph node for the first available clip name, falling back to the default clip.
fn node_for(anims: &PedestrianAnimations, candidates: &[&str]) -> Option<AnimationNodeIndex> {
    for c in candidates {
        if let Some(n) = anims.nodes.get(*c) {
            return Some(*n);
        }
    }
    anims
        .default_animation()
        .and_then(|d| anims.nodes.get(&d).copied())
}

#[allow(clippy::too_many_arguments)]
pub fn drive_character_animation(
    time: Res<Time>,
    anims: Res<PedestrianAnimations>,
    controlled: Res<ControlledCharacter>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
    mut controllers: Query<
        (
            &LinearVelocity,
            Has<Grounded>,
            &MovementModifiers,
            &mut AnimState,
            &mut CombatState,
        ),
        With<CharacterController>,
    >,
    mut players: Query<(Entity, &mut AnimationPlayer)>,
    parents: Query<&ChildOf>,
) {
    if !anims.ready {
        return;
    }
    let Some(ped) = controlled.ped else {
        return;
    };
    let Some(controller) = controlled.controller else {
        return;
    };
    let Ok((velocity, grounded, modifiers, mut anim, mut combat)) = controllers.get_mut(controller)
    else {
        return;
    };

    // Do not fire combat when interacting with egui.
    let over_ui = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
        .unwrap_or(false);
    let lmb = !over_ui && mouse.just_pressed(MouseButton::Left);
    let rmb = !over_ui && mouse.pressed(MouseButton::Right);

    // Find the AnimationPlayer that descends from the controlled pedestrian.
    let mut found = None;
    for (player_ent, _) in players.iter() {
        let mut cur = player_ent;
        loop {
            if cur == ped {
                found = Some(player_ent);
                break;
            }
            match parents.get(cur) {
                Ok(child_of) => cur = child_of.0,
                Err(_) => break,
            }
        }
        if found.is_some() {
            break;
        }
    }
    let Some(player_ent) = found else {
        return;
    };
    let Ok((_, mut player)) = players.get_mut(player_ent) else {
        return;
    };

    // Take over the player once (clear the default clip the shared setup may have started).
    if !anim.took_over {
        player.stop_all();
        anim.took_over = true;
        anim.base_node = None;
        combat.node = None;
        combat.kind = CombatKind::None;
    }

    // --- Base locomotion state machine ---------------------------------------------------------
    let dt = time.delta_secs();
    if anim.timer > 0.0 {
        anim.timer -= dt;
    }
    let just_airborne = !grounded && matches!(anim.phase, JumpPhase::Grounded | JumpPhase::Land);
    let just_landed = grounded && matches!(anim.phase, JumpPhase::Start | JumpPhase::Loop);
    if just_airborne {
        anim.phase = JumpPhase::Start;
        anim.timer = JUMP_START_TIME;
    } else if just_landed {
        anim.phase = JumpPhase::Land;
        anim.timer = JUMP_LAND_TIME;
    } else {
        match anim.phase {
            JumpPhase::Start if anim.timer <= 0.0 => anim.phase = JumpPhase::Loop,
            JumpPhase::Land if anim.timer <= 0.0 => anim.phase = JumpPhase::Grounded,
            _ => {}
        }
    }

    let speed = Vec2::new(velocity.x as f32, velocity.z as f32).length();
    let moving = speed > MOVE_ANIM_THRESHOLD;
    let base_candidates: &[&str] = match anim.phase {
        JumpPhase::Start => &["Jump_Start"],
        JumpPhase::Loop => &["Jump_Loop"],
        JumpPhase::Land => &["Jump_Land"],
        JumpPhase::Grounded => {
            if modifiers.crouch {
                if moving {
                    &["Crouch_Fwd_Loop"]
                } else {
                    &["Crouch_Idle_Loop", "Idle_Loop"]
                }
            } else if moving {
                if speed < WALK_MAX_SPEED {
                    &["Walk_Loop"]
                } else if speed < JOG_MAX_SPEED {
                    &["Jog_Fwd_Loop"]
                } else {
                    &["Sprint_Loop", "Sprint_Fwd_Loop"]
                }
            } else {
                &["Idle_Loop", "A_TPose"]
            }
        }
    };

    if let Some(base_node) = node_for(&anims, base_candidates) {
        if anim.base_node != Some(base_node) {
            if let Some(old) = anim.base_node {
                player.stop(old);
            }
            player.play(base_node).repeat();
            anim.base_node = Some(base_node);
        }
    }

    // --- Combat overlay state machine ----------------------------------------------------------
    let current_finished = match combat.kind {
        CombatKind::Jab | CombatKind::Shoot => combat
            .node
            .map_or(true, |n| player.animation(n).map_or(true, |a| a.is_finished())),
        _ => true,
    };

    let want = if rmb {
        if lmb {
            CombatKind::Shoot
        } else if combat.kind == CombatKind::Shoot && !current_finished {
            CombatKind::Shoot
        } else {
            CombatKind::Aim
        }
    } else if lmb {
        CombatKind::Jab
    } else if combat.kind == CombatKind::Jab && !current_finished {
        CombatKind::Jab
    } else {
        CombatKind::None
    };

    let want_node = match want {
        CombatKind::None => None,
        CombatKind::Jab => node_for(&anims, &["Punch_Jab", "Punch_Cross"]),
        CombatKind::Aim => node_for(&anims, &["Pistol_Idle_Loop", "Pistol_Aim_Neutral"]),
        CombatKind::Shoot => node_for(&anims, &["Pistol_Shoot"]),
    };

    let changed = want != combat.kind || want_node != combat.node;
    let restart = lmb && matches!(want, CombatKind::Jab | CombatKind::Shoot);
    if changed || restart {
        if let Some(old) = combat.node {
            if Some(old) != want_node {
                player.stop(old);
            }
        }
        if let Some(n) = want_node {
            let active = player.play(n);
            active.set_weight(1.0);
            match want {
                CombatKind::Aim => {
                    active.repeat();
                }
                CombatKind::Jab | CombatKind::Shoot => {
                    // One-shot: (re)start from the beginning, no repeat.
                    active.seek_to(0.0);
                }
                CombatKind::None => {}
            }
        }
        combat.kind = want;
        combat.node = want_node;
    }

    // Duck the base clip while a combat overlay is active so the overlay reads on top.
    if let Some(base_node) = anim.base_node {
        if let Some(active) = player.animation_mut(base_node) {
            let w = if combat.kind == CombatKind::None {
                1.0
            } else {
                BASE_WEIGHT_WITH_COMBAT
            };
            active.set_weight(w);
        }
    }
}
