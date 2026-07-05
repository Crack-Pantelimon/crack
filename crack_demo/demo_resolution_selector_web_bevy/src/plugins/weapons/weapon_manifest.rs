//! Weapon manifest parsing.
//!
//! The manifest at `{DATA_BASE_URL}/3d_data/3d_weapons/out2/manifest.txt` is a CSV with a header
//! line and columns `path,is_gun,clip_size,bullet_type,damage,range`, e.g.
//! `gun/ak47.glb,1,30,7.62x39,42,150`. Melee rows have `0` in every column after the path.
//! The path's first segment is the class folder (`gun` or `melee`).
//!
//! # Weapon-local coordinate conventions
//! Every weapon has its **grip point at the origin `(0,0,0)`**.
//! - Guns are **aimed toward +X** (barrel along +X), so `max(x)` of a gun ≈ its length.
//! - Swords / melee weapons have their **blade pointing straight up (+Y)**, so `max(y)` ≈ length.

use bevy::prelude::*;

use crate::plugins::pedestrians::manifest::TextAsset;

/// Gun stats parsed from the manifest CSV.
#[derive(Clone, Debug, PartialEq)]
pub struct GunInfo {
    /// Full loadable URL of the model.
    pub path: String,
    pub clip_size: u32,
    pub bullet_type: String,
    pub damage: f32,
    pub range: f32,
}

/// A selectable weapon. The `String`/`GunInfo.path` is the full loadable URL.
#[derive(Clone, Debug, PartialEq)]
pub enum WeaponId {
    Unarmed,
    Melee(String),
    Gun(GunInfo),
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
    /// The loadable model path, if this weapon has a model.
    pub fn path(&self) -> Option<&str> {
        match self {
            WeaponId::Unarmed => None,
            WeaponId::Melee(p) => Some(p),
            WeaponId::Gun(g) => Some(&g.path),
        }
    }
    /// Gun stats, if this is a gun.
    pub fn gun_info(&self) -> Option<&GunInfo> {
        match self {
            WeaponId::Gun(g) => Some(g),
            _ => None,
        }
    }
    /// A short human-readable label for UI.
    pub fn label(&self) -> String {
        match self.path() {
            None => "Unarmed".to_string(),
            Some(p) => p.rsplit('/').next().unwrap_or(p).replace(".glb", ""),
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
        // CSV columns: path,is_gun,clip_size,bullet_type,damage,range (header line skipped).
        let cols: Vec<&str> = line.split(',').map(str::trim).collect();
        let rel_path = cols[0];
        if rel_path == "path" {
            continue; // header
        }
        // Full loadable path, prefixed with the manifest folder.
        let full = format!("{}{}", bootstrap.folder, rel_path);
        let is_gun = cols
            .get(1)
            .and_then(|c| c.parse::<u32>().ok())
            .map(|v| v == 1)
            // Fallback for malformed rows: classify by folder.
            .unwrap_or_else(|| rel_path.starts_with("gun/"));
        if is_gun {
            guns.push(WeaponId::Gun(GunInfo {
                path: full,
                clip_size: cols.get(2).and_then(|c| c.parse().ok()).unwrap_or(10),
                bullet_type: cols.get(3).unwrap_or(&"9mm").to_string(),
                damage: cols.get(4).and_then(|c| c.parse().ok()).unwrap_or(20.0),
                range: cols.get(5).and_then(|c| c.parse().ok()).unwrap_or(50.0),
            }));
        } else {
            melee.push(WeaponId::Melee(full));
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
