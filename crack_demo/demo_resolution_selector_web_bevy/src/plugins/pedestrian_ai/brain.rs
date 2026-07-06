//! AI brain: state machine that decides each ped's behavior.

use bevy::prelude::*;

use crate::plugins::weapons::{EquippedWeapon, GunState, ReloadGunEvent};

use super::{AiCombatTimers, AiPedestrian, AiPerception, AiState, faction::Health};

/// HP threshold below which the AI flees.
const FLEE_HP: f32 = 30.0;
/// Gun enemies closer than this trigger flee (panic).
const PANIC_RANGE: f32 = 6.0;
/// Reload duration while repositioning.
const RELOAD_TIME: f32 = 2.0;

/// Tick timers and decide the desired behavior state for each AI ped.
pub fn ai_brain(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<
        (
            Entity,
            &Health,
            Option<&EquippedWeapon>,
            Option<&GunState>,
            &AiPerception,
            &mut AiState,
            &mut AiCombatTimers,
            Option<&crate::plugins::pedestrians::pedestrian_controller_plugin::EjectedDriver>,
        ),
        With<AiPedestrian>,
    >,
) {
    let dt = time.delta_secs();

    for (entity, health, equipped, gun_state, perception, mut state, mut timers, ejected_driver) in &mut query {
        if health.current <= 0.0 {
            continue;
        }
        if ejected_driver.is_some() {
            continue;
        }
        // Tick timers.
        timers.attack_cooldown = (timers.attack_cooldown - dt).max(0.0);
        timers.reload_timer = (timers.reload_timer - dt).max(0.0);
        timers.repath_timer = (timers.repath_timer - dt).max(0.0);

        let is_gun = equipped.is_some_and(|e| e.0.is_gun());
        let has_ammo = gun_state.is_some_and(|g| g.rounds > 0);

        // Priority 1: Flee.
        let low_hp = health.current <= FLEE_HP;
        let panic = is_gun
            && perception.visible
            && perception.target_dist < PANIC_RANGE;
        if low_hp || panic {
            *state = AiState::Flee;
        }
        // Priority 2: Reposition (gun reload).
        else if is_gun && perception.visible && !has_ammo {
            if *state != AiState::Reposition {
                timers.reload_timer = RELOAD_TIME;
            }
            *state = AiState::Reposition;

            // When the reload timer expires, refill the clip and go back to Hunt.
            if timers.reload_timer <= 0.0 {
                commands.trigger(ReloadGunEvent { shooter: entity });
                *state = AiState::Hunt;
            }
        }
        // Priority 3: Hunt.
        else if perception.visible {
            *state = AiState::Hunt;
        }
        // Priority 4: Idle.
        else {
            *state = AiState::Idle;
        }
    }
}
