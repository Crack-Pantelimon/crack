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
