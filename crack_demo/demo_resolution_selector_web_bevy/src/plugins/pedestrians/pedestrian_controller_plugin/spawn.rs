//! Spawning, adopting, and despawning the controlled pedestrian, plus state transitions.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::seq::IndexedRandom;

use super::*;
use crate::plugins::{
    cars_driving::driving_plugin::GamePhysicsLayer,
    pedestrians::{
        PedestrianManifest, PedestrianUrl, SpawnPedestrianEvent,
    },
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

    let controller = commands.spawn((
        Name::new("PedestrianController"),
        CharacterController,
        CharacterScale(scale),
        CharacterMovementSettings::default(),
        CharacterCollisions::default(),
        MovementModifiers::default(),
        LocomotionInput::default(),
        AnimState::default(),
        CombatState::default(),
        GroundDetection {
            cast_shape: Some(Collider::capsule(CAPSULE_RADIUS * 0.99, CAPSULE_LENGTH)),
            ..default()
        },
        Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
        // Same layer convention as the cars so the solver resolves ground/car interactions.
        CollisionLayers::new(
            GamePhysicsLayer::Car,
            [
                GamePhysicsLayer::Map,
                GamePhysicsLayer::Car,
                GamePhysicsLayer::Wheel,
            ],
        ),
        Transform::from_translation(controller_pos)
            .with_rotation(event.rotation.unwrap_or(Quat::IDENTITY)),
        Visibility::default(),
    ))
    .insert((
        CollisionEventsEnabled,
        crate::plugins::pedestrian_ai::faction::Health::full(100.0),
        crate::plugins::pedestrian_ai::faction::Faction::Neutral,
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

/// Escape leaves pedestrian control: despawn the character and return to freecam.
pub fn escape_to_freecam(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut controlled: ResMut<ControlledCharacter>,
    mut next_state: ResMut<NextState<GameControlState>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
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
