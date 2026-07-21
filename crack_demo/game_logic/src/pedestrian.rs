use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationMeta {
    pub name: String,
    pub duration: f32,
    pub frames: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedestrianManifestResult {
    pub urls: Vec<String>,
    pub animations: Vec<AnimationMeta>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_pedestrian_manifest_serde_round_trip() {
        let manifest = PedestrianManifestResult {
            urls: vec!["models/ped_01.glb".to_string()],
            animations: vec![AnimationMeta {
                name: "walk".to_string(),
                duration: 1.5,
                frames: 36,
            }],
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: PedestrianManifestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}
