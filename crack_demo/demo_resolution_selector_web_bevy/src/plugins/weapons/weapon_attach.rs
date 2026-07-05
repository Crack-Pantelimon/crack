//! Equipping a weapon: attaching/detaching the model on the character's right wrist, and computing
//! its extents.

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use super::weapon_manifest::WeaponId;
use crate::plugins::pedestrians::skeleton::PedestrianSkeleton;

/// The local axis (in wrist-bone space) along which the grip offset is applied.
const GRIP_OFFSET_AXIS: Vec3 = Vec3::Y;

/// The logical weapon a character has equipped. Set immediately on equip so animation reacts even
/// before the model finishes loading.
#[derive(Component, Clone)]
pub struct EquippedWeapon(pub WeaponId);

/// Request to equip `weapon` on `character` (the character/controller entity).
#[derive(Event)]
pub struct EquipWeaponEvent {
    pub character: Entity,
    pub weapon: WeaponId,
}

/// Distance the weapon grip is offset from the wrist bone (UI slider, 0.05..=0.5).
#[derive(Resource)]
pub struct WeaponGripOffset(pub f32);

impl Default for WeaponGripOffset {
    fn default() -> Self {
        Self(0.15)
    }
}

/// Tracks which weapon model is currently spawned for a character.
#[derive(Component, Default)]
pub struct WeaponModelState {
    pub spawned_for: Option<WeaponId>,
    pub entity: Option<Entity>,
}

/// Marker on a spawned weapon model entity.
#[derive(Component)]
pub struct WeaponModel;

/// Marker while a weapon's extents have not yet been computed.
#[derive(Component)]
pub struct PendingWeaponExtents;

/// A weapon's coordinate extents (in weapon-local space): `max_x` ≈ gun length, `max_y` ≈ blade length.
#[derive(Component, Debug)]
pub struct WeaponExtents {
    pub max_x: f32,
    pub max_y: f32,
}

pub fn equip_weapon_observer(trigger: On<EquipWeaponEvent>, mut commands: Commands) {
    let ev = trigger.event();
    commands
        .entity(ev.character)
        .insert(EquippedWeapon(ev.weapon.clone()));
    // Guns carry ammo state (a fresh full clip); anything else has none.
    match &ev.weapon {
        WeaponId::Gun(info) => {
            commands.entity(ev.character).insert(super::GunState {
                rounds: info.clip_size,
                clip_size: info.clip_size,
            });
        }
        _ => {
            commands.entity(ev.character).remove::<super::GunState>();
        }
    }
}

/// Finds the right-wrist bone entity under `character` (the ped model is a descendant).
fn find_right_hand(
    character: Entity,
    children_query: &Query<&Children>,
    skeletons: &Query<&PedestrianSkeleton>,
) -> Option<Entity> {
    let mut stack = vec![character];
    while let Some(entity) = stack.pop() {
        if let Ok(skel) = skeletons.get(entity) {
            if let Some(hand) = skel.right_hand {
                return Some(hand);
            }
        }
        if let Ok(children) = children_query.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }
    }
    None
}

/// Spawns/despawns the weapon model to match each character's `EquippedWeapon`.
pub fn reconcile_weapon_model(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut characters: Query<(Entity, &EquippedWeapon, Option<&mut WeaponModelState>)>,
    children_query: Query<&Children>,
    skeletons: Query<&PedestrianSkeleton>,
    pending: Query<(), With<PendingWeaponExtents>>,
) {
    for (character, equipped, state) in &mut characters {
        let equipped_id = equipped.0.clone();

        // Already showing the right model?
        if let Some(state) = &state {
            if state.spawned_for.as_ref() == Some(&equipped_id) {
                continue;
            }
            // The previous switch is still in flight (model loading / extents pending): wait for
            // it to finish before switching again. This prevents despawning an entity that
            // `finalize_weapon_extents` is concurrently working on (fast mouse-wheel panic).
            if let Some(current) = state.entity {
                if pending.get(current).is_ok() {
                    continue;
                }
            }
        }

        // For a real weapon we need the wrist bone; wait until the skeleton is classified.
        let wrist = if equipped_id.is_unarmed() {
            None
        } else {
            match find_right_hand(character, &children_query, &skeletons) {
                Some(w) => Some(w),
                None => continue, // skeleton not ready yet — retry next frame
            }
        };

        // Despawn the previous model.
        if let Some(state) = &state {
            if let Some(old) = state.entity {
                commands.entity(old).despawn();
            }
        }

        // Spawn the new model (Unarmed has none).
        let new_entity = match (equipped_id.path().map(str::to_string), wrist) {
            (Some(url), Some(wrist)) => {
                let handle =
                    asset_server.load::<WorldAsset>(GltfAssetLabel::Scene(0).from_asset(url));
                let entity = commands
                    .spawn((
                        Name::new("Weapon"),
                        ChildOf(wrist),
                        WorldAssetRoot(handle),
                        Transform::IDENTITY,
                        Visibility::default(),
                        InheritedVisibility::default(),
                        WeaponModel,
                        PendingWeaponExtents,
                    ))
                    .id();
                Some(entity)
            }
            _ => None,
        };

        let new_state = WeaponModelState {
            spawned_for: Some(equipped_id),
            entity: new_entity,
        };
        match state {
            Some(mut s) => *s = new_state,
            None => {
                commands.entity(character).insert(new_state);
            }
        }
    }
}

/// Recursively collects `(entity, mesh handle)` for all `Mesh3d` descendants.
fn collect_mesh_descendants(
    entity: Entity,
    children_query: &Query<&Children>,
    mesh_query: &Query<&Mesh3d>,
    out: &mut Vec<(Entity, Handle<Mesh>)>,
) {
    if let Ok(mesh3d) = mesh_query.get(entity) {
        out.push((entity, mesh3d.0.clone()));
    }
    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            collect_mesh_descendants(child, children_query, mesh_query, out);
        }
    }
}

/// Once a weapon's meshes load, compute its extents (max x/y in weapon-local space) and log them.
pub fn finalize_weapon_extents(
    mut commands: Commands,
    pending: Query<(Entity, &Children), With<PendingWeaponExtents>>,
    children_query: Query<&Children>,
    mesh_query: Query<&Mesh3d>,
    global_transforms: Query<&GlobalTransform>,
    meshes: Res<Assets<Mesh>>,
) {
    for (weapon_root, children) in &pending {
        let mut mesh_entities = Vec::new();
        for child in children.iter() {
            collect_mesh_descendants(child, &children_query, &mesh_query, &mut mesh_entities);
        }
        if mesh_entities.is_empty() {
            continue;
        }
        if mesh_entities
            .iter()
            .any(|(_, h)| meshes.get(h).is_none())
        {
            continue; // meshes still loading
        }

        let Ok(root_gt) = global_transforms.get(weapon_root) else {
            continue;
        };
        let root_inv = root_gt.to_matrix().inverse();

        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for (ent, handle) in &mesh_entities {
            let Ok(mesh_gt) = global_transforms.get(*ent) else {
                continue;
            };
            let Some(mesh) = meshes.get(handle) else {
                continue;
            };
            if let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            {
                for pos in positions {
                    let local = root_inv.transform_point3(mesh_gt.transform_point(Vec3::from(*pos)));
                    min = min.min(local);
                    max = max.max(local);
                }
            }
        }

        let extents = WeaponExtents {
            max_x: max.x,
            max_y: max.y,
        };
        info!(
            "Weapon extents: gun_length(max_x)={:.3} blade_length(max_y)={:.3}",
            extents.max_x, extents.max_y
        );
        commands
            .entity(weapon_root)
            .insert(extents)
            .remove::<PendingWeaponExtents>();
    }
}

/// Keeps every weapon's grip offset from the wrist in sync with the live slider value.
pub fn apply_grip_offset(
    grip: Res<WeaponGripOffset>,
    mut weapons: Query<&mut Transform, With<WeaponModel>>,
) {
    for mut transform in &mut weapons {
        transform.translation = GRIP_OFFSET_AXIS * grip.0;
    }
}
