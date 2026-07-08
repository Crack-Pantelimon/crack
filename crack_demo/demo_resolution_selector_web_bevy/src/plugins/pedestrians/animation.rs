//! Animation graph + per-entity animation control for pedestrians.
//!
//! A single shared [`AnimationGraph`] is built once (see `manifest.rs`) from the first
//! pedestrian asset and reused for every spawned model (their bone names match). Consumers
//! drive a specific pedestrian by triggering a [`PedestrianAnimationControlEvent`].

use bevy::ecs::relationship::Relationship;
use bevy::prelude::*;

use crate::plugins::pedestrians::spawn_pedestrian::{ModelRoot, PedestrianGltf};

/// Metadata about a single animation clip, exposed for UI listing.
#[derive(Clone, Debug)]
pub struct AnimationInfo {
    pub name: String,
    pub duration: f32,
    pub frames: u32,
    pub node: AnimationNodeIndex,
}

/// Shared animation graph + catalog, populated by the manifest bootstrap once the first
/// pedestrian asset is loaded.
#[derive(Resource, Default)]
pub struct PedestrianAnimations {
    pub graph_handle: Handle<AnimationGraph>,
    pub nodes: std::collections::HashMap<String, AnimationNodeIndex>,
    pub catalog: std::collections::BTreeMap<String, AnimationInfo>,
    pub ready: bool,
}

impl PedestrianAnimations {
    /// A sensible default clip to play when none has been requested for a pedestrian.
    pub fn default_animation(&self) -> Option<String> {
        if self.catalog.contains_key("A_TPose") {
            Some("A_TPose".to_string())
        } else {
            self.catalog.keys().next().cloned()
        }
    }
}

/// Control event: make `ped` play `animation` at `speed`.
#[derive(Event, Clone)]
pub struct PedestrianAnimationControlEvent {
    pub ped: Entity,
    pub animation: String,
    pub speed: f32,
}

/// Opt-out marker: put this on a pedestrian [`ModelRoot`] whose animations are driven manually by
/// another system (e.g. the character controller's multi-clip blender). [`play_animations_system`]
/// skips these pedestrians so it does not fight the manual driver.
#[derive(Component)]
pub struct ManualAnimation;

#[derive(Component)]
pub struct PlayOnceAnimation;

/// Tracks a transient one-shot animation playing on this player.
#[derive(Component)]
pub struct ActiveOneShot {
    pub node: AnimationNodeIndex,
    pub name: String,
}

/// Desired animation for a pedestrian, written by the control observer, read by
/// [`play_animations_system`].
#[derive(Component, Clone)]
pub struct TargetAnimation {
    pub name: String,
    pub speed: f32,
}

/// Tracks what is currently playing on an animation player, to avoid redundant restarts.
#[derive(Component)]
pub struct CurrentPlayingAnimation {
    pub name: String,
    pub speed: f32,
}

pub fn pedestrian_animation_control_observer(
    trigger: On<PedestrianAnimationControlEvent>,
    mut commands: Commands,
    mut targets: Query<&mut TargetAnimation>,
) {
    let ev = trigger.event();
    if let Ok(mut target) = targets.get_mut(ev.ped) {
        target.name = ev.animation.clone();
        target.speed = ev.speed;
    } else {
        let anim_name = ev.animation.clone();
        let anim_speed = ev.speed;
        commands
            .entity(ev.ped)
            .queue_silenced(move |mut entity: EntityWorldMut| {
                entity.insert(TargetAnimation {
                    name: anim_name,
                    speed: anim_speed,
                });
            });
    }
}

pub fn setup_animation_players_system(
    mut commands: Commands,
    anims: Res<PedestrianAnimations>,
    players: Query<Entity, (With<AnimationPlayer>, Without<AnimationGraphHandle>)>,
) {
    if !anims.ready {
        return;
    }
    for player_ent in &players {
        commands
            .entity(player_ent)
            .insert(AnimationGraphHandle(anims.graph_handle.clone()));
    }
}

pub fn play_animations_system(
    mut commands: Commands,
    anims: Res<PedestrianAnimations>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    model_roots: Query<
        (&PedestrianGltf, Option<&TargetAnimation>, Has<PlayOnceAnimation>),
        (With<ModelRoot>, Without<ManualAnimation>),
    >,
    mut players: Query<(
        Entity,
        &mut AnimationPlayer,
        Option<&mut CurrentPlayingAnimation>,
        Option<&ActiveOneShot>,
    )>,
    parent_query: Query<&ChildOf>,
) {
    if !anims.ready {
        return;
    }

    for (player_ent, mut player, current_playing, active_oneshot) in players.iter_mut() {
        if let Some(oneshot) = active_oneshot {
            if let Some(active) = player.animation(oneshot.node) {
                if !active.is_finished() {
                    // One-shot still playing, skip target locomotion update.
                    continue;
                }
            }
            // One-shot is finished or missing from player, clean it up.
            commands.entity(player_ent).remove::<ActiveOneShot>();
        }
        // Walk up the hierarchy to the model root that owns this animation player.
        let mut current = player_ent;
        let mut root_data = None;
        loop {
            if let Ok(data) = model_roots.get(current) {
                root_data = Some(data);
                break;
            }
            if let Ok(parent) = parent_query.get(current) {
                current = parent.get();
            } else {
                break;
            }
        }

        let Some((gltf_comp, target, play_once)) = root_data else {
            continue;
        };

        let Some(gltf) = gltf_assets.get(&gltf_comp.handle) else {
            continue;
        };

        // Determine which animation to play: the per-entity target, falling back to a default.
        let desired = target
            .map(|t| t.name.clone())
            .or_else(|| anims.default_animation());
        let Some(desired) = desired else {
            continue;
        };
        let anim_name = if gltf.named_animations.contains_key(desired.as_str()) {
            desired
        } else if let Some(def) = anims.default_animation() {
            def
        } else {
            continue;
        };

        let target_speed = target.map(|t| t.speed).unwrap_or(1.0);

        let should_update = match &current_playing {
            Some(curr) => curr.name != anim_name || curr.speed != target_speed,
            None => true,
        };

        if !should_update {
            continue;
        }

        if let Some(&node_index) = anims.nodes.get(&anim_name) {
            let name_changed = match &current_playing {
                Some(curr) => curr.name != anim_name,
                None => true,
            };

            if name_changed {
                player.stop_all();
                let active = player.play(node_index).set_speed(target_speed);
                if !play_once {
                    active.repeat();
                }
            } else if let Some(active) = player.animation_mut(node_index) {
                active.set_speed(target_speed);
            }

            if let Some(mut curr) = current_playing {
                curr.name = anim_name.clone();
                curr.speed = target_speed;
            } else {
                commands.entity(player_ent).insert(CurrentPlayingAnimation {
                    name: anim_name.clone(),
                    speed: target_speed,
                });
            }
        }
    }
}
