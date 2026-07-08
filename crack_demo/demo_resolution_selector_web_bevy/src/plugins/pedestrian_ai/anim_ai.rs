//! AI animation driver: picks a base locomotion clip and triggers
//! [`PedestrianAnimationControlEvent`] only when the clip changes.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::plugins::pedestrians::locomotion_clip;
use crate::plugins::pedestrians::pedestrian_controller_plugin::{
    CharacterScale, MOVE_ANIM_THRESHOLD, MovementModifiers,
};
use crate::plugins::pedestrians::PedestrianAnimationControlEvent;
use crate::plugins::weapons::EquippedWeapon;

use super::{AiAnim, AiModel, AiPedestrian, AiState};

/// Picks a base locomotion clip for each AI ped and triggers the animation event when it changes.
pub fn ai_animation(
    mut commands: Commands,
    mut query: Query<
        (
            &LinearVelocity,
            &MovementModifiers,
            &AiState,
            &CharacterScale,
            &AiModel,
            &mut AiAnim,
            &super::faction::Health,
            Option<&EquippedWeapon>,
            Option<&crate::plugins::pedestrians::pedestrian_controller_plugin::EjectedDriver>,
        ),
        With<AiPedestrian>,
    >,
) {
    for (
        velocity,
        modifiers,
        _state,
        char_scale,
        ai_model,
        mut anim,
        health,
        equipped,
        ejected_driver,
    ) in query.iter_mut()
    {
        if health.current <= 0.0 {
            continue;
        }
        let speed = Vec2::new(velocity.x as f32, velocity.z as f32).length();
        let is_melee = equipped.is_some_and(|e| e.0.is_melee());

        let clips: &[&str] = if let Some(ejected) = ejected_driver {
            match ejected.stage {
                crate::plugins::pedestrians::pedestrian_controller_plugin::EjectedStage::OnGround => {
                    &["Fixing_Kneeling"]
                }
                crate::plugins::pedestrians::pedestrian_controller_plugin::EjectedStage::StandingUp => {
                    &["Sitting_Exit"]
                }
            }
        } else if is_melee && !modifiers.crouch && speed <= MOVE_ANIM_THRESHOLD {
            &["Sword_Idle"]
        } else {
            locomotion_clip(speed, modifiers.crouch, modifiers.sprint)
        };
        let clip = clips[0];

        // Only trigger when the clip changes.
        let clip_str = clip.to_string();
        if anim.last.as_ref() != Some(&clip_str) {
            anim.last = Some(clip_str.clone());
            let anim_speed = 1.0 / char_scale.0;
            commands.trigger(PedestrianAnimationControlEvent {
                ped: ai_model.0,
                animation: clip_str,
                speed: anim_speed,
            });
        }
    }
}
