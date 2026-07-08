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
use crate::plugins::pedestrian_ai::Dying;
use crate::plugins::pedestrians::PedestrianAnimations;
use crate::plugins::pedestrians::pedestrian_controller_plugin::interaction_ui::EnteringCarTimer;
use crate::plugins::weapons::weapon_attach::{WeaponModel, WeaponModelState};
use crate::plugins::weapons::{EquippedWeapon, FireGunEvent, GunState, ReloadGunEvent};
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
pub(super) fn node_for(
    anims: &PedestrianAnimations,
    candidates: &[&str],
) -> Option<AnimationNodeIndex> {
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
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut controllers: Query<
        (
            &LinearVelocity,
            Has<Grounded>,
            &MovementModifiers,
            &CharacterScale,
            Has<Climbing>,
            Has<Rolling>,
            Option<&EquippedWeapon>,
            Option<&GunState>,
            &mut AnimState,
            &mut CombatState,
            Option<&EnteringCarTimer>,
            Option<&WeaponModelState>,
            &GlobalTransform,
            Has<Dying>,
            Option<&super::EjectedDriver>,
        ),
        With<CharacterController>,
    >,
    mut players: Query<(Entity, &mut AnimationPlayer)>,
    parents: Query<&ChildOf>,
    weapon_models: Query<&GlobalTransform, With<WeaponModel>>,
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
    let Ok((
        velocity,
        grounded,
        modifiers,
        char_scale,
        climbing,
        rolling,
        equipped,
        gun_state,
        mut anim,
        mut combat,
        entering,
        weapon_model_state,
        char_gt,
        dying,
        ejected_driver,
    )) = controllers.get_mut(controller)
    else {
        return;
    };
    // Shorter characters animate faster (inverse of mesh scale).
    let anim_speed = 1.0 / char_scale.0;
    // The Roll clip (used for climbing and crouch rolls) is too long at 1x; play it faster.
    let base_speed = if climbing || rolling {
        anim_speed * ROLL_ANIM_SPEED_MULT
    } else {
        anim_speed
    };

    // Which weapon class is equipped (None component == Unarmed).
    let weapon = equipped.map(|e| &e.0);
    let is_gun = weapon.is_some_and(|w| w.is_gun());
    let is_melee = weapon.is_some_and(|w| w.is_melee());

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

    // --- Ejected driver stand-up sequence animation -------------------------------------------
    if let Some(ejected) = ejected_driver {
        if let Some(old) = combat.node {
            player.stop(old);
        }
        combat.node = None;
        combat.kind = CombatKind::None;

        let candidates: &[&str] = match ejected.stage {
            super::EjectedStage::OnGround => &["Fixing_Kneeling"],
            super::EjectedStage::StandingUp => &["Sitting_Exit"],
        };
        let target_node = node_for(&anims, candidates);
        if anim.base_node != target_node {
            if let Some(old) = anim.base_node {
                player.stop(old);
            }
            if let Some(node) = target_node {
                player.play(node).set_speed(anim_speed).repeat();
            }
            anim.base_node = target_node;
        }
        return;
    }

    // --- Dead: play the death clip once and freeze; no locomotion or combat overlay. ----------
    if dying {
        // Drop any combat overlay so only the death clip reads.
        if let Some(old) = combat.node {
            player.stop(old);
        }
        combat.node = None;
        combat.kind = CombatKind::None;

        let death_node = node_for(&anims, &["Death01"]);
        if anim.base_node != death_node {
            if let Some(old) = anim.base_node {
                player.stop(old);
            }
            if let Some(node) = death_node {
                // No `.repeat()`: play once and hold on the final (downed) frame.
                player.play(node).set_speed(anim_speed);
            }
            anim.base_node = death_node;
        }
        return;
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
    let base_candidates: &[&str] = if entering.is_some() {
        &["Sitting_Enter"]
    } else if climbing || rolling {
        // No dedicated climb clip exists in the catalog; play the "Roll" clip for climbs & rolls.
        &["Roll", "Jump_Loop"]
    } else {
        match anim.phase {
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
                } else if is_melee {
                    // A melee weapon replaces the neutral idle with the sword idle.
                    &["Sword_Idle", "Idle_Loop"]
                } else {
                    &["Idle_Loop", "A_TPose"]
                }
            }
        }
    };

    if let Some(base_node) = node_for(&anims, base_candidates) {
        if anim.base_node != Some(base_node) {
            if let Some(old) = anim.base_node {
                player.stop(old);
            }
            player.play(base_node).repeat().set_speed(base_speed);
            anim.base_node = Some(base_node);
        } else if let Some(active) = player.animation_mut(base_node) {
            // Keep the height/state-based speed applied even if the clip did not change.
            active.set_speed(base_speed);
        }
    }

    // --- Combat overlay state machine (weapon-aware) -------------------------------------------
    // A one-shot attack keeps playing until it finishes; an aim loop holds while RMB is down.
    let one_shot_finished = combat.kind == CombatKind::OneShot
        && combat.node.map_or(true, |n| {
            player.animation(n).map_or(true, |a| a.is_finished())
        });

    let can_shoot = gun_state.map_or(false, |g| g.rounds > 0);
    let reload_pressed = !over_ui && keys.just_pressed(KeyCode::KeyR);

    // A press starts (or restarts) a one-shot clip depending on the equipped weapon.
    let mut pressed_node = None;
    if lmb {
        let whoosh_pos = weapon_model_state
            .and_then(|wms| wms.entity)
            .and_then(|e| weapon_models.get(e).ok())
            .map(|gt| gt.translation())
            .unwrap_or_else(|| char_gt.translation());

        if is_gun {
            // Only fire (and animate) when there are rounds left in the clip.
            if can_shoot {
                pressed_node = node_for(&anims, &["Pistol_Shoot"]);
                commands.trigger(FireGunEvent {
                    shooter: controller,
                });
            }
        } else if is_melee {
            pressed_node = node_for(&anims, &["Sword_Attack"]);
            commands.entity(controller).insert(
                crate::plugins::weapons::weapon_shooting::PendingMeleeHit {
                    timer: 0.25,
                    is_melee: true,
                },
            );
            commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                fx: crate::plugins::audio::audio_fx::AudioFxEventType::MeleeWhoosh { volume: 1.0 },
                position: whoosh_pos,
                follow: None,
            });
        } else {
            if rand::random::<bool>() {
                pressed_node = node_for(&anims, &["Punch_Jab", "Punch_Cross"]);
            } else {
                pressed_node = node_for(&anims, &["Punch_Cross", "Punch_Jab"]);
            }
            commands.entity(controller).insert(
                crate::plugins::weapons::weapon_shooting::PendingMeleeHit {
                    timer: 0.25,
                    is_melee: false,
                },
            );
            commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                fx: crate::plugins::audio::audio_fx::AudioFxEventType::MeleeWhoosh { volume: 0.4 },
                position: whoosh_pos,
                follow: None,
            });
        }
    } else if reload_pressed && is_gun && gun_state.is_some_and(|g| g.rounds < g.clip_size) {
        pressed_node = node_for(&anims, &["Pistol_Reload"]);
        commands.trigger(ReloadGunEvent {
            shooter: controller,
        });
    }

    let (want_kind, want_node) = if pressed_node.is_some() {
        (CombatKind::OneShot, pressed_node)
    } else if combat.kind == CombatKind::OneShot && !one_shot_finished {
        // Keep the in-progress one-shot playing.
        (CombatKind::OneShot, combat.node)
    } else if is_gun && rmb {
        // Guns aim while RMB is held.
        (
            CombatKind::Aim,
            node_for(&anims, &["Pistol_Idle_Loop", "Pistol_Aim_Neutral"]),
        )
    } else {
        (CombatKind::None, None)
    };

    let changed = want_kind != combat.kind || want_node != combat.node;
    // A fresh press always (re)starts the one-shot from the beginning.
    let restart = pressed_node.is_some();
    if changed || restart {
        if let Some(old) = combat.node {
            if Some(old) != want_node {
                player.stop(old);
            }
        }
        if let Some(n) = want_node {
            let active = player.play(n);
            active.set_weight(1.0);
            match want_kind {
                CombatKind::Aim => {
                    active.repeat();
                }
                CombatKind::OneShot => {
                    // One-shot: (re)start from the beginning, no repeat.
                    active.seek_to(0.0);
                }
                CombatKind::None => {}
            }
        }
        combat.kind = want_kind;
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
