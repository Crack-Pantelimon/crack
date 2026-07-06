//! Pedestrian AI plugin: faction-based autonomous combatants.
//!
//! Spawned AI pedestrians receive a [`Faction`], [`Health`], and a behavior state machine
//! ([`AiState`]) driven by line-of-sight perception. Systems run un-gated (regardless of
//! [`GameControlState`]) so AI operates both in the main game and in headless test binaries.

pub mod anim_ai;
pub mod brain;
pub mod combat;
pub mod debug_ui;
pub mod faction;
pub mod movement_ai;
pub mod perception;
pub mod spawn_ai;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

use crate::plugins::pedestrians::pedestrian_controller_plugin::locomotion::CharacterLocomotionPlugin;

pub use faction::{Faction, Health, WarMatrix};
pub use spawn_ai::SpawnAiPedestrianEvent;

// -------------------------------------------------------------------------------------
// AI Components
// -------------------------------------------------------------------------------------

/// Marks an AI-driven pedestrian (present on the capsule controller entity).
#[derive(Component)]
pub struct AiPedestrian;

/// Current behavior state. Logged on every transition.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum AiState {
    Idle,
    /// Has a visible enemy: move to engage + attack per weapon.
    Hunt,
    /// Gun only: break contact to reload behind cover.
    Reposition,
    /// Low HP, or gun-enemy inside panic range.
    Flee,
}

/// Live perception result, refreshed each tick by the perception system.
#[derive(Component, Default)]
pub struct AiPerception {
    /// Nearest visible enemy controller entity.
    pub target: Option<Entity>,
    /// Enemy head position (LOS endpoint) when visible.
    pub target_pos: Vec3,
    pub target_dist: f32,
    pub visible: bool,
    /// Cached LOS ray endpoints for debug gizmos: (from, to, hit_enemy).
    pub last_los: Option<(Vec3, Vec3, bool)>,
}

/// Attack pacing & reload bookkeeping.
#[derive(Component, Default)]
pub struct AiCombatTimers {
    /// Gun burst / melee swing cadence countdown.
    pub attack_cooldown: f32,
    /// >0 while "reloading" (Reposition state).
    pub reload_timer: f32,
    /// Jitter for flank/flee direction recompute.
    pub repath_timer: f32,
}

/// Cached steering direction (recomputed on `repath_timer`), so flank/flee paths are stable.
#[derive(Component, Default)]
pub struct AiSteer {
    /// World-space planar direction.
    pub desired: Vec3,
    /// Cached probe segments for debug gizmos: (from, to, color).
    pub last_probes: Vec<(Vec3, Vec3, Color)>,
}

/// Reference to the model root entity (for animation events).
#[derive(Component)]
pub struct AiModel(pub Entity);

/// Animation state tracker for AI peds (avoid re-triggering every frame).
#[derive(Component, Default)]
pub struct AiAnim {
    pub last: Option<String>,
}

// -------------------------------------------------------------------------------------
// Plugin
// -------------------------------------------------------------------------------------

pub struct PedestrianAiPlugin;

impl Plugin for PedestrianAiPlugin {
    fn build(&self, app: &mut App) {
        // Guard-add the shared locomotion plugin (player controller plugin may also add it).
        if !app.is_plugin_added::<CharacterLocomotionPlugin>() {
            app.add_plugins(CharacterLocomotionPlugin);
        }

        app.init_resource::<WarMatrix>()
            .init_resource::<debug_ui::AiDebug>()
            .init_resource::<spawn_ai::PendingAiAdopts>()
            .add_observer(spawn_ai::spawn_ai_pedestrian_observer)
            .add_observer(combat::apply_damage_observer)
            .add_systems(
                Update,
                (
                    spawn_ai::adopt_ai_pedestrian,
                    perception::ai_perception,
                    brain::ai_brain,
                    movement_ai::ai_movement,
                    combat::ai_combat,
                    anim_ai::ai_animation,
                    debug_ui::draw_ai_gizmos,
                )
                    .chain(),
            )
            .add_systems(EguiPrimaryContextPass, debug_ui::ai_debug_ui);
    }
}
