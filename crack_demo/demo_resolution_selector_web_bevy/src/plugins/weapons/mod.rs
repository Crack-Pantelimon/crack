//! Weapons: parse a manifest of gun/melee models and attach the chosen one to a character's right
//! wrist. Combat animations elsewhere read [`EquippedWeapon`] to pick punch / sword / pistol clips.
//!
//! Requires [`crate::plugins::pedestrians::PedestriansPlugin`] (it reuses that plugin's `TextAsset`
//! loader for the manifest and its skeleton classification to find the right-wrist bone).

pub mod weapon_attach;
pub mod weapon_manifest;
pub mod weapon_shooting;

use bevy::prelude::*;

pub use weapon_attach::{
    EquipWeaponEvent, EquippedWeapon, WeaponExtents, WeaponGripOffset, WeaponKind, WeaponModel,
};
pub use weapon_manifest::{GunInfo, WeaponId, WeaponManifest};
pub use weapon_shooting::{
    BulletSpark, BulletSparks, FireGunEvent, GunState, ReloadGunEvent, ShotTracers,
};

use weapon_attach::{
    equip_weapon_observer, finalize_weapon_extents, reconcile_weapon_model,
    update_weapon_transforms,
};
use weapon_manifest::{load_weapon_manifest_system, start_weapon_manifest_load};
use weapon_shooting::{
    draw_bullet_sparks, draw_shot_tracers, fire_gun_observer, reload_gun_observer,
};

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WeaponManifest>()
            .init_resource::<WeaponGripOffset>()
            .init_resource::<ShotTracers>()
            .init_resource::<BulletSparks>()
            .add_observer(equip_weapon_observer)
            .add_observer(fire_gun_observer)
            .add_observer(reload_gun_observer)
            .add_systems(Startup, start_weapon_manifest_load)
            // Chained: reconcile's despawns are applied before finalize runs, so finalize can never
            // queue commands against a weapon entity despawned in the same frame (panic fix).
            .add_systems(
                Update,
                (
                    load_weapon_manifest_system,
                    reconcile_weapon_model,
                    finalize_weapon_extents,
                    update_weapon_transforms,
                )
                    .chain(),
            )
            .add_systems(Update, (draw_shot_tracers, draw_bullet_sparks));
    }
}
