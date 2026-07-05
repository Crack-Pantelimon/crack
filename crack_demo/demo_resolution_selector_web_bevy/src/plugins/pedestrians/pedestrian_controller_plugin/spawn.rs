//! Spawning, adopting, and despawning the controlled pedestrian, plus state transitions.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::seq::IndexedRandom;

use super::*;
use crate::plugins::{
    cars_driving::driving_plugin::GamePhysicsLayer,
    pedestrians::{ManualAnimation, ModelRoot, PedestrianManifest, PedestrianUrl, SpawnPedestrianEvent},
    states::GameControlState,
};

/// Tracks the currently controlled character and its (child) pedestrian model.
#[derive(Resource, Default)]
pub struct ControlledCharacter {
    pub controller: Option<Entity>,
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

    let controller_pos = Vec3::new(
        event.position.x,
        event.position.y + CAPSULE_HALF_HEIGHT + 0.2,
        event.position.z,
    );

    let controller = commands
        .spawn((
            Name::new("PedestrianController"),
            CharacterController,
            CharacterMovementSettings::default(),
            CharacterCollisions::default(),
            MovementModifiers::default(),
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
            Transform::from_translation(controller_pos),
            Visibility::default(),
        ))
        .id();

    controlled.controller = Some(controller);
    controlled.awaiting = true;

    commands.trigger(SpawnPedestrianEvent {
        url,
        position: controller_pos,
    });

    next_state.set(GameControlState::ControllingPedestrian);
}

/// Adopts a freshly spawned pedestrian model as the visual child of the pending controller.
pub fn adopt_pedestrian(
    mut commands: Commands,
    mut controlled: ResMut<ControlledCharacter>,
    new_peds: Query<Entity, Added<ModelRoot>>,
) {
    if !controlled.awaiting {
        return;
    }
    let Some(controller) = controlled.controller else {
        return;
    };
    for ped in new_peds.iter() {
        commands.entity(ped).insert((
            ChildOf(controller),
            // Local offset so the model's feet meet the bottom of the capsule.
            Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0),
            // Drive this model's animations manually (skip the shared play_animations_system).
            ManualAnimation,
        ));
        controlled.ped = Some(ped);
        controlled.awaiting = false;
        break;
    }
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
    controlled.awaiting = false;
    next_state.set(GameControlState::MapFreecam);
}
