use super::lru::LruCache;
use crate::tile::*;

static TILE_CACHE: tokio::sync::RwLock<Option<LruCache<FetchTileResponse>>> =
    tokio::sync::RwLock::const_new(None);

fn extract_collider_data(glb_bytes: &[u8]) -> anyhow::Result<MeshColliderData> {
    let gltf = gltf::Gltf::from_slice(glb_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse GLB: {:?}", e))?;
    let blob = gltf
        .blob
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("GLB has no binary blob"))?;

    let mut combined_vertices = Vec::new();
    let mut combined_indices = Vec::new();

    for mesh in gltf.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| match buffer.source() {
                gltf::buffer::Source::Bin => Some(&blob[..]),
                _ => None,
            });

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .map(|iter| iter.collect())
                .unwrap_or_default();

            let indices: Vec<u32> = reader
                .read_indices()
                .map(|iter| iter.into_u32().collect())
                .unwrap_or_default();

            if positions.is_empty() || indices.is_empty() {
                continue;
            }

            let vertex_offset = combined_vertices.len() as u32;
            combined_vertices.extend(positions);

            for chunk in indices.chunks_exact(3) {
                combined_indices.push([
                    chunk[0] + vertex_offset,
                    chunk[1] + vertex_offset,
                    chunk[2] + vertex_offset,
                ]);
            }
        }
    }

    Ok(MeshColliderData {
        vertices: combined_vertices,
        indices: combined_indices,
    })
}

pub async fn fetch_map_tile(req: FetchTileRequest) -> anyhow::Result<FetchTileResponse> {
    let t0 = _crack_utils::get_timestamp_now_ms();

    // Check cache
    {
        let mut guard = TILE_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| LruCache::new(512));
        if let Some(mut cached) = cache.get(&req.tile_id) {
            cached.from_cache = true;
            let t1 = _crack_utils::get_timestamp_now_ms();
            tracing::debug!("Tile cache HIT: {} (took {} ms)", req.tile_id, t1 - t0);
            return Ok(cached);
        }
    }

    // Cache miss: fetch from url
    let url = format!(
        "{}/3d_data_v2/{}",
        req.base_url.trim_end_matches('/'),
        req.glb_path.trim_start_matches('/')
    );
    let glb_bytes = super::http::http_get_bytes(&url).await?;
    let t_fetch = _crack_utils::get_timestamp_now_ms();

    let collider_mesh = match extract_collider_data(&glb_bytes) {
        Ok(mesh) => {
            if mesh.vertices.is_empty() {
                None
            } else {
                Some(mesh)
            }
        }
        Err(e) => {
            tracing::error!(
                "Failed to extract collider data for GLB {}: {:?}",
                req.tile_id,
                e
            );
            None
        }
    };
    let t_parse = _crack_utils::get_timestamp_now_ms();

    let response = FetchTileResponse {
        tile_id: req.tile_id.clone(),
        glb_bytes: glb_bytes.to_vec(),
        collider_mesh,
        from_cache: false,
    };

    // Store in cache
    {
        let mut guard = TILE_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| LruCache::new(512));
        cache.insert(req.tile_id.clone(), response.clone());
    }

    let t1 = _crack_utils::get_timestamp_now_ms();
    tracing::debug!(
        "Tile fetch completed: {} (fetch: {} ms, parse: {} ms, total: {} ms, bytes: {})",
        req.tile_id,
        t_fetch - t0,
        t_parse - t_fetch,
        t1 - t0,
        response.glb_bytes.len()
    );

    Ok(response)
}

pub async fn get_tile_collider(tile_id: &str) -> Option<crate::tile::MeshColliderData> {
    let mut guard = TILE_CACHE.write().await;
    let cache = guard.get_or_insert_with(|| LruCache::new(512));
    cache
        .get(tile_id)
        .and_then(|resp| resp.collider_mesh.clone())
}
