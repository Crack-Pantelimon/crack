//! Spawning and adopting AI pedestrians.

use bevy::prelude::*;
use rand::seq::IndexedRandom;

use avian3d::prelude::LinearVelocity;

use crate::plugins::{
    pedestrians::{
        PedestrianManifest, PedestrianUrl, SpawnPedestrianEvent,
        pedestrian_controller_plugin::{
            CAPSULE_HALF_HEIGHT, CharacterScale, MovementModifiers, SCALE_MAX, SCALE_MIN,
            character_physics_bundle,
        },
    },
    weapons::{EquipWeaponEvent, WeaponId, WeaponManifest},
};

use super::{
    AiAnim, AiCombatTimers, AiPedestrian, AiPerception, AiState, AiSteer, AiThink,
    faction::{DEFAULT_HP, Enemies, Faction, Health},
};
use crate::plugins::cars_driving::driving_plugin::spawn_car::{CAR_SEAT_OFFSETS, CarPassenger};

/// Spawn an AI-driven pedestrian at `position` with the given `faction`.
#[derive(Event)]
pub struct SpawnAiPedestrianEvent {
/// position field.
    pub position: Vec3,
/// faction field.
    pub faction: Faction,
    /// `None` picks a random model from the manifest.
    pub url: Option<PedestrianUrl>,
    /// `None` picks a random weapon from the manifest.
    pub weapon: Option<WeaponId>,
    /// Optional car and seat to spawn the passenger into.
    pub car_seat: Option<(Entity, usize)>,
}

/// spawn ai pedestrian observer.
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

    // Seated car passengers are visual-only. A physics capsule here would (a) sit on the `Car`
    // collision layer and violently shove the car's dynamic body ("explosion"), and (b) as a
    // kinematic body it would not ride along with the car. So passengers get NO collider / rigid
    // body — just a plain child of the car that follows it via transform propagation, exactly
    // like the driver mesh. Ground-spawned AI peds still get the full physics capsule.
    let controller = if let Some((car_entity, seat_idx)) = event.car_seat {
        let local_pos = CAR_SEAT_OFFSETS[seat_idx] + Vec3::new(0.0, CAPSULE_HALF_HEIGHT, 0.0);
        commands
            .spawn((
                Name::new("CarPassengerController"),
                Transform::from_translation(local_pos)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                Visibility::default(),
                ChildOf(car_entity),
                CarPassenger {
                    seat_index: seat_idx,
                    car: car_entity,
                },
                // Components the shared AI animation system reads. The passenger never moves, so
                // velocity stays zero and `ai_animation` plays the seated idle clip for it.
                CharacterScale(scale),
                MovementModifiers::default(),
                LinearVelocity::ZERO,
            ))
            .id()
    } else {
        commands
            .spawn((
                Name::new("AiPedestrianController"),
                character_physics_bundle(scale, Transform::from_translation(controller_pos)),
            ))
            .id()
    };

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
        AiThink::default(),
        Enemies::default(),
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
