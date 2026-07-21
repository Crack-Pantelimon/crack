use serde::{Deserialize, Serialize};

/// Request to fetch one GLB model from the worker content server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchGlbRequest {
    /// HTTP origin of the worker serving the model.
    pub base_url: String,
    /// Relative path to the `.glb` file on the content server.
    pub glb_path: String,
    /// Stable asset id used for worker-side caching.
    pub asset_id: String,
}

/// GLB bytes returned by a model fetch RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchGlbResponse {
    /// Asset id echoed from the request.
    pub asset_id: String,
    /// Raw GLB file bytes.
    pub glb_bytes: Vec<u8>,
    /// True when the response was served from the worker LRU cache.
    pub from_cache: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_fetch_glb_response_serde_round_trip() {
        let resp = FetchGlbResponse {
            asset_id: "ped_01".to_string(),
            glb_bytes: vec![0x67, 0x6c, 0x54, 0x46],
            from_cache: true,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: FetchGlbResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.asset_id, resp.asset_id);
        assert_eq!(back.glb_bytes, resp.glb_bytes);
        assert_eq!(back.from_cache, resp.from_cache);
    }
}
