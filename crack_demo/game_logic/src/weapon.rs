use serde::{Deserialize, Serialize};

/// One weapon definition from the worker weapon manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponEntry {
    /// Relative path to the weapon `.glb` model.
    pub path: String,
    /// True when the weapon fires projectiles (gun) vs melee/other.
    pub is_gun: bool,
    /// Magazine capacity in rounds.
    pub clip_size: u32,
    /// Ammunition type identifier.
    pub bullet_type: String,
    /// Base damage per hit.
    pub damage: f32,
    /// Effective range in world units.
    pub range: f32,
    /// Rounds per minute fire rate.
    pub rpm: f32,
    /// True when holding fire repeats shots automatically.
    pub automatic: bool,
    /// Seconds to complete a reload animation.
    pub reload_secs: f32,
}

/// Weapon manifest listing every downloadable weapon model and stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponManifestResult {
    /// All weapons available to clients.
    pub weapons: Vec<WeaponEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_weapon_manifest_serde_round_trip() {
        let manifest = WeaponManifestResult {
            weapons: vec![WeaponEntry {
                path: "models/rifle.glb".to_string(),
                is_gun: true,
                clip_size: 30,
                bullet_type: "5.56".to_string(),
                damage: 25.0,
                range: 300.0,
                rpm: 600.0,
                automatic: true,
                reload_secs: 2.5,
            }],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: WeaponManifestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}
