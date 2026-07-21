use serde::{Deserialize, Serialize};

/// Metadata for one skeletal animation clip in a pedestrian GLB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationMeta {
    /// Clip name inside the GLB.
    pub name: String,
    /// Clip duration in seconds.
    pub duration: f32,
    /// Number of keyframes in the clip.
    pub frames: u32,
}

/// Pedestrian model manifest listing downloadable GLBs and animations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedestrianManifestResult {
    /// Relative GLB paths for each pedestrian model.
    pub urls: Vec<String>,
    /// Animation clips shared across pedestrian models.
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
