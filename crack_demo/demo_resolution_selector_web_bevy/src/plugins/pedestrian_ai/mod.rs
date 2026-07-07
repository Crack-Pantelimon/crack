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

pub use faction::{DEATH_ANIM_TIME, DEFAULT_HP, Dying, Enemies, Faction, Health, WarMatrix};
pub use spawn_ai::SpawnAiPedestrianEvent;

/// Emitted whenever any pedestrian (AI or player) dies, so grudge lists can be pruned.
#[derive(Message, Clone, Copy)]
pub struct PedestrianDied {
    pub entity: Entity,
}

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
    /// True when the current `target` is an enemy-driven car rather than a pedestrian.
    pub target_is_car: bool,
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

/// Per-entity think throttle: the heavy AI systems (perception raycasts, brain, combat) only run
/// for an entity when `ready` is true, which happens once every `period` seconds. `period` is
/// randomized per ped (0.1–0.2 s) and the initial `timer` is jittered so a freshly spawned crowd
/// does not all think on the same frame (thundering herd).
#[derive(Component)]
pub struct AiThink {
    pub timer: f32,
    pub period: f32,
    pub ready: bool,
}

impl Default for AiThink {
    fn default() -> Self {
        let period = 0.1 + rand::random::<f32>() * 0.1; // 0.1..0.2 s
        Self {
            timer: rand::random::<f32>() * period,
            period,
            ready: true,
        }
    }
}

/// Advances each ped's think throttle once per frame; sets `ready` on the frames it may run.
pub fn tick_ai_think(time: Res<Time>, mut q: Query<&mut AiThink>) {
    let dt = time.delta_secs();
    for mut think in &mut q {
        think.timer -= dt;
        if think.timer <= 0.0 {
            think.ready = true;
            think.timer += think.period;
        } else {
            think.ready = false;
        }
    }
}

/// Prunes dead pedestrians out of every grudge list when a [`PedestrianDied`] event fires.
pub fn prune_enemies_on_death(
    mut deaths: MessageReader<PedestrianDied>,
    mut q_enemies: Query<&mut Enemies>,
) {
    let dead: Vec<Entity> = deaths.read().map(|d| d.entity).collect();
    if dead.is_empty() {
        return;
    }
    for mut enemies in &mut q_enemies {
        enemies.0.retain(|e| !dead.contains(e));
    }
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
            .add_message::<PedestrianDied>()
            .add_observer(spawn_ai::spawn_ai_pedestrian_observer)
            .add_observer(combat::apply_damage_observer)
            .add_systems(
                Update,
                (
                    tick_ai_think,
                    prune_enemies_on_death,
                    perception::ai_perception,
                    brain::ai_brain,
                    movement_ai::ai_movement,
                    combat::ai_combat,
                    anim_ai::ai_animation,
                    debug_ui::draw_ai_gizmos,
                )
                    .chain(),
            )
            // Death handling: play the death clip once, then despawn the corpse. Runs regardless
            // of control state so both AI peds and the player pedestrian are handled.
            .add_systems(
                Update,
                (combat::start_ai_death_animation, combat::tick_dying),
            )
            .add_systems(EguiPrimaryContextPass, debug_ui::ai_debug_ui);
    }
}
