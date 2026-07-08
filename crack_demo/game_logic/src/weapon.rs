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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaponManifestResult {
    pub weapons: Vec<WeaponEntry>,
}
