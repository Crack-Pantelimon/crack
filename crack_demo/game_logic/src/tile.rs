use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshColliderData {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchTileRequest {
    pub base_url: String,
    pub glb_path: String,
    pub tile_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchTileResponse {
    pub tile_id: String,
    pub glb_bytes: Vec<u8>,
    pub collider_mesh: Option<MeshColliderData>,
    pub from_cache: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_fetch_tile_response_serde_round_trip() {
        let resp = FetchTileResponse {
            tile_id: "tile_02".to_string(),
            glb_bytes: vec![1, 2, 3],
            collider_mesh: Some(MeshColliderData {
                vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
                indices: vec![[0, 1, 2]],
            }),
            from_cache: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: FetchTileResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(serde_json::to_string(&back).unwrap(), json);
    }
}
