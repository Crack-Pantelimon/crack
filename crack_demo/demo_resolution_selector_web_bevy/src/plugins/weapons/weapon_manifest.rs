//! Weapon manifest parsing.
//!
//! The manifest at `{DATA_BASE_URL}/3d_data/3d_weapons/out2/manifest.txt` lists one relative weapon
//! path per line, e.g. `gun/ak47.glb` or `melee/machete2.glb`. The first path segment is the class
//! (`gun` or `melee`).
//!
//! # Weapon-local coordinate conventions
//! Every weapon has its **grip point at the origin `(0,0,0)`**.
//! - Guns are **aimed toward +X** (barrel along +X), so `max(x)` of a gun ≈ its length.
//! - Swords / melee weapons have their **blade pointing straight up (+Y)**, so `max(y)` ≈ length.

use bevy::prelude::*;

use crate::plugins::pedestrians::manifest::TextAsset;

/// A selectable weapon. The `String` is the manifest line (e.g. `"gun/ak47.glb"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeaponId {
    Unarmed,
    Melee(String),
    Gun(String),
}

impl WeaponId {
    pub fn is_unarmed(&self) -> bool {
        matches!(self, WeaponId::Unarmed)
    }
    pub fn is_gun(&self) -> bool {
        matches!(self, WeaponId::Gun(_))
    }
    pub fn is_melee(&self) -> bool {
        matches!(self, WeaponId::Melee(_))
    }
    /// The manifest-relative path, if this weapon has a model.
    pub fn path(&self) -> Option<&str> {
        match self {
            WeaponId::Unarmed => None,
            WeaponId::Melee(p) | WeaponId::Gun(p) => Some(p),
        }
    }
    /// A short human-readable label for UI.
    pub fn label(&self) -> String {
        match self {
            WeaponId::Unarmed => "Unarmed".to_string(),
            WeaponId::Melee(p) | WeaponId::Gun(p) => {
                p.rsplit('/').next().unwrap_or(p).replace(".glb", "")
            }
        }
    }
}

/// Public manifest resource: the parsed weapon lists plus a combined `all` list (Unarmed first).
#[derive(Resource, Default)]
pub struct WeaponManifest {
    pub guns: Vec<WeaponId>,
    pub melee: Vec<WeaponId>,
    /// `[Unarmed]` + guns + melee, in that order — the order the UI/mouse-wheel cycles through.
    pub all: Vec<WeaponId>,
    pub loaded: bool,
}

/// Internal bootstrap state for loading the weapon manifest text.
#[derive(Resource)]
pub struct WeaponManifestBootstrap {
    folder: String,
    handle: Handle<TextAsset>,
}

pub fn start_weapon_manifest_load(mut commands: Commands, asset_server: Res<AssetServer>) {
    let base_url = crate::config::DATA_BASE_URL.trim_end_matches('/');
    let folder = format!("{}/3d_data/3d_weapons/out2/", base_url);
    let manifest_url = format!("{}manifest.txt", folder);
    let handle = asset_server.load::<TextAsset>(manifest_url);
    commands.insert_resource(WeaponManifestBootstrap { folder, handle });
}

pub fn load_weapon_manifest_system(
    bootstrap: Option<Res<WeaponManifestBootstrap>>,
    text_assets: Res<Assets<TextAsset>>,
    mut manifest: ResMut<WeaponManifest>,
) {
    if manifest.loaded {
        return;
    }
    let Some(bootstrap) = bootstrap else {
        return;
    };
    let Some(text) = text_assets.get(&bootstrap.handle) else {
        return;
    };

    let mut guns = Vec::new();
    let mut melee = Vec::new();
    for line in text.text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Full loadable path, prefixed with the manifest folder.
        let full = format!("{}{}", bootstrap.folder, line);
        let class = line.split('/').next().unwrap_or("");
        match class {
            "melee" => melee.push(WeaponId::Melee(full)),
            _ => guns.push(WeaponId::Gun(full)),
        }
    }

    let mut all = vec![WeaponId::Unarmed];
    all.extend(guns.iter().cloned());
    all.extend(melee.iter().cloned());
    manifest.guns = guns;
    manifest.melee = melee;
    manifest.all = all;
    manifest.loaded = true;

    info!(
        "Weapon manifest loaded: {} guns, {} melee.",
        manifest.guns.len(),
        manifest.melee.len()
    );
}
