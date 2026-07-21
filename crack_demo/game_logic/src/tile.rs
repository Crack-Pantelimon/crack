use serde::{Deserialize, Serialize};

/// Triangle mesh collider extracted from a map tile GLB.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshColliderData {
    /// World-space vertex positions.
    pub vertices: Vec<[f32; 3]>,
    /// Triangle index triples into `vertices`.
    pub indices: Vec<[u32; 3]>,
}

/// Request to fetch one map tile GLB and optional collider mesh.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchTileRequest {
    /// HTTP origin of the worker serving tile assets.
    pub base_url: String,
    /// Relative path to the tile `.glb` on the content server.
    pub glb_path: String,
    /// Stable tile id used for worker-side caching.
    pub tile_id: String,
}

/// Map tile GLB bytes plus an extracted collider when available.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchTileResponse {
    /// Tile id echoed from the request.
    pub tile_id: String,
    /// Raw GLB file bytes.
    pub glb_bytes: Vec<u8>,
    /// Collider mesh built from the GLB, if extraction succeeded.
    pub collider_mesh: Option<MeshColliderData>,
    /// True when the response was served from the worker LRU cache.
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
