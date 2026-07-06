//! Spawning and adopting AI pedestrians.

use avian3d::prelude::*;
use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::plugins::{
    cars_driving::driving_plugin::GamePhysicsLayer,
    pedestrians::{
        PedestrianManifest, PedestrianUrl, SpawnPedestrianEvent,
        pedestrian_controller_plugin::{
            CAPSULE_HALF_HEIGHT, CAPSULE_LENGTH, CAPSULE_RADIUS, SCALE_MAX, SCALE_MIN,
            CharacterController, CharacterMovementSettings, CharacterScale, CharacterCollisions,
            GroundDetection, LocomotionInput, MovementModifiers,
        },
    },
    weapons::{EquipWeaponEvent, WeaponId, WeaponManifest},
};

use super::{
    AiAnim, AiCombatTimers, AiPedestrian, AiPerception, AiState, AiSteer,
    faction::{Faction, Health, DEFAULT_HP},
};

/// Spawn an AI-driven pedestrian at `position` with the given `faction`.
#[derive(Event)]
pub struct SpawnAiPedestrianEvent {
    pub position: Vec3,
    pub faction: Faction,
    /// `None` picks a random model from the manifest.
    pub url: Option<PedestrianUrl>,
    /// `None` picks a random weapon from the manifest.
    pub weapon: Option<WeaponId>,
}



pub fn spawn_ai_pedestrian_observer(
    trigger: On<SpawnAiPedestrianEvent>,
    mut commands: Commands,
    manifest: Res<PedestrianManifest>,
    weapon_manifest: Res<WeaponManifest>,
) {
    let event = trigger.event();

    let Some(url) = event
        .url
        .clone()
        .or_else(|| manifest.urls.choose(&mut rand::rng()).cloned())
    else {
        warn!("SpawnAiPedestrianEvent: manifest has no pedestrians yet");
        return;
    };

    let weapon = event.weapon.clone().unwrap_or_else(|| {
        if weapon_manifest.all.is_empty() {
            WeaponId::Unarmed
        } else {
            let idx = (_crack_utils::random_u32() as usize) % weapon_manifest.all.len();
            weapon_manifest.all[idx].clone()
        }
    });

    let scale = SCALE_MIN + rand::random::<f32>() * (SCALE_MAX - SCALE_MIN);

    let controller_pos = Vec3::new(
        event.position.x,
        event.position.y + CAPSULE_HALF_HEIGHT + 0.2,
        event.position.z,
    );

    let controller = commands
        .spawn((
            Name::new("AiPedestrianController"),
            CharacterController,
            CharacterScale(scale),
            CharacterMovementSettings::default(),
            CharacterCollisions::default(),
            MovementModifiers::default(),
            LocomotionInput::default(),
            GroundDetection {
                cast_shape: Some(Collider::capsule(CAPSULE_RADIUS * 0.99, CAPSULE_LENGTH)),
                ..default()
            },
            Collider::capsule(CAPSULE_RADIUS, CAPSULE_LENGTH),
            CollisionLayers::new(
                GamePhysicsLayer::Car,
                [
                    GamePhysicsLayer::Map,
                    GamePhysicsLayer::Car,
                    GamePhysicsLayer::Wheel,
                ],
            ),
            CollisionEventsEnabled,
            RigidBody::Kinematic,
            Transform::from_translation(controller_pos),
            Visibility::default(),
        ))
        .id();

    // Insert AI components separately to stay under Bevy's Bundle tuple limit.
    commands.entity(controller).insert((
        AiPedestrian,
        event.faction,
        Health::full(DEFAULT_HP),
        AiState::Idle,
        AiPerception::default(),
        AiCombatTimers::default(),
        AiSteer::default(),
        AiAnim::default(),
    ));

    let scale_node = commands
        .spawn((
            Name::new("AiPedestrianScaleNode"),
            ChildOf(controller),
            Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0).with_scale(Vec3::splat(scale)),
            Visibility::default(),
        ))
        .id();

    // Equip the weapon immediately
    commands.trigger(EquipWeaponEvent {
        character: controller,
        weapon,
    });

    commands.trigger(SpawnPedestrianEvent {
        url,
        position: controller_pos,
        controller,
        parent: scale_node,
    });
}


