//! Spawning, adopting, and despawning the controlled pedestrian, plus state transitions.

use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::plugins::pedestrians::{ModelRoot, spawn_pedestrian::ModelController};

use super::*;
use crate::plugins::{
    pedestrians::{PedestrianManifest, PedestrianUrl, SpawnPedestrianEvent},
    states::GameControlState,
};

/// Tracks the currently controlled character and its (child) pedestrian model.
#[derive(Resource, Default)]
pub struct ControlledCharacter {
    pub controller: Option<Entity>,
    /// Intermediate node (child of controller, parent of the model) that applies the mesh scale.
    pub scale_node: Option<Entity>,
    pub ped: Option<Entity>,
    /// True after spawning a controller while we wait for the pedestrian model to appear.
    pub awaiting: bool,
}

/// Pending freecam right-click "spawn pedestrian / spawn car" choice popup.
#[derive(Resource, Default)]
pub struct SpawnChoicePopup {
    pub active: bool,
    pub world_pos: Vec3,
    pub screen_pos: Vec2,
}

/// Spawn a controllable pedestrian at `position` (ground point) and enter pedestrian control.
/// `url` picks a specific model; `None` spawns a random one from the manifest.
#[derive(Event)]
pub struct SpawnControlledPedestrianEvent {
    pub position: Vec3,
    pub url: Option<PedestrianUrl>,
    /// Mesh scale, clamped to `[SCALE_MIN, SCALE_MAX]`. `None` picks a random scale in that range.
    pub scale: Option<f32>,
    pub is_exiting_car: bool,
    pub rotation: Option<Quat>,
    /// Carried-over health (e.g. when getting out of a car). `None` spawns at full HP.
    pub health: Option<crate::plugins::pedestrian_ai::faction::Health>,
}

pub fn spawn_controlled_pedestrian_observer(
    trigger: On<SpawnControlledPedestrianEvent>,
    mut commands: Commands,
    manifest: Res<PedestrianManifest>,
    mut controlled: ResMut<ControlledCharacter>,
    mut next_state: ResMut<NextState<GameControlState>>,
) {
    let event = trigger.event();

    let Some(url) = event
        .url
        .clone()
        .or_else(|| manifest.urls.choose(&mut rand::rng()).cloned())
    else {
        warn!("SpawnControlledPedestrianEvent: manifest has no pedestrians yet");
        return;
    };

    // Despawn the previous character (its model child goes with it).
    if let Some(old) = controlled.controller.take() {
        commands.entity(old).despawn();
    }
    controlled.ped = None;
    controlled.scale_node = None;

    let scale = event
        .scale
        .unwrap_or_else(|| SCALE_MIN + rand::random::<f32>() * (SCALE_MAX - SCALE_MIN))
        .clamp(SCALE_MIN, SCALE_MAX);

    let controller_pos = Vec3::new(
        event.position.x,
        event.position.y + CAPSULE_HALF_HEIGHT + 0.2,
        event.position.z,
    );

    let health = event
        .health
        .unwrap_or_else(|| crate::plugins::pedestrian_ai::faction::Health::full(100.0));

    let controller = commands
        .spawn((
            Name::new("PedestrianController"),
            super::character_physics_bundle(
                scale,
                Transform::from_translation(controller_pos)
                    .with_rotation(event.rotation.unwrap_or(Quat::IDENTITY)),
            ),
            AnimState::default(),
            CombatState::default(),
            health,
            crate::plugins::pedestrian_ai::faction::Faction::Neutral,
            url.clone(),
        ))
        .id();

    // Intermediate scale node: child of controller, parent of the model. Scaling here keeps the
    // model's feet at the capsule bottom and does not affect the animation playback.
    let scale_node = commands
        .spawn((
            Name::new("PedestrianScaleNode"),
            ChildOf(controller),
            Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0).with_scale(Vec3::splat(scale)),
            Visibility::default(),
        ))
        .id();

    controlled.controller = Some(controller);
    controlled.scale_node = Some(scale_node);
    controlled.awaiting = true;

    commands.trigger(SpawnPedestrianEvent {
        url,
        position: controller_pos,
        controller,
        parent: scale_node,
    });

    next_state.set(GameControlState::ControllingPedestrian);
}

#[derive(Component)]
pub struct DeathProp {
    pub timer: Timer,
}

pub fn tick_death_props(
    time: Res<Time>,
    mut commands: Commands,
    mut q_props: Query<(Entity, &mut DeathProp)>,
) {
    for (entity, mut prop) in &mut q_props {
        prop.timer.tick(time.delta());
        if prop.timer.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

pub fn setup_death_prop_animations(
    mut commands: Commands,
    q_models: Query<(Entity, &ModelController), Added<ModelRoot>>,
    q_props: Query<(), With<DeathProp>>,
) {
    for (model_ent, controller_ref) in &q_models {
        if q_props.contains(controller_ref.0) {
            commands.entity(model_ent).insert((
                crate::plugins::pedestrians::TargetAnimation {
                    name: "Death01".to_string(),
                    speed: 1.0,
                },
                crate::plugins::pedestrians::PlayOnceAnimation,
            ));
        }
    }
}

/// When the controlled pedestrian dies, leave pedestrian control and return to freecam.
/// Spawn a non-looping death prop pedestrian showing the Death01 animation and despawn it after 10 seconds.
pub fn player_death_to_freecam(
    mut commands: Commands,
    mut controlled: ResMut<ControlledCharacter>,
    q_newly_dying: Query<(Entity, &PedestrianUrl, &Transform, &CharacterScale), Added<crate::plugins::pedestrian_ai::faction::Dying>>,
    mut next_state: ResMut<NextState<GameControlState>>,
) {
    let Some(controller) = controlled.controller else {
        return;
    };
    let Ok((_entity, url, transform, scale)) = q_newly_dying.get(controller) else {
        return;
    };

    let prop_parent = commands.spawn((
        Name::new("DeathPropParent"),
        *transform,
        DeathProp {
            timer: Timer::from_seconds(10.0, TimerMode::Once),
        },
    )).id();

    let prop_scale_node = commands.spawn((
        Name::new("DeathPropScaleNode"),
        ChildOf(prop_parent),
        Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0).with_scale(Vec3::splat(scale.0)),
        Visibility::default(),
    )).id();

    commands.trigger(SpawnPedestrianEvent {
        url: url.clone(),
        position: transform.translation,
        controller: prop_parent,
        parent: prop_scale_node,
    });

    controlled.controller = None;
    controlled.ped = None;
    controlled.scale_node = None;
    controlled.awaiting = false;

    commands.entity(controller).despawn();
    next_state.set(GameControlState::MapFreecam);
}

/// Escape leaves pedestrian control: despawn the character and return to freecam.
pub fn escape_to_freecam(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut controlled: ResMut<ControlledCharacter>,
    mut next_state: ResMut<NextState<GameControlState>>,
    capture_state: Res<crate::plugins::states::MouseCaptureState>,
    mut contexts: bevy_egui::EguiContexts,
) {
    let egui_wants_keyboard = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_keyboard_input()
    } else {
        false
    };
    if egui_wants_keyboard {
        return;
    }

    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if capture_state.is_captured {
        return;
    }
    if let Some(controller) = controlled.controller.take() {
        commands.entity(controller).despawn();
    }
    controlled.ped = None;
    controlled.scale_node = None;
    controlled.awaiting = false;
    next_state.set(GameControlState::MapFreecam);
}
