use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponEntry {
    pub path: String,
    pub is_gun: bool,
    pub clip_size: u32,
    pub bullet_type: String,
    pub damage: f32,
    pub range: f32,
    pub rpm: f32,
    pub automatic: bool,
    pub reload_secs: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponManifestResult {
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
