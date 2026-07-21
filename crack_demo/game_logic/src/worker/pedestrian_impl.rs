use crate::api::FetchArgs;
use crate::glb::{FetchGlbRequest, FetchGlbResponse};
use crate::pedestrian::{AnimationMeta, PedestrianManifestResult};
use std::sync::Arc;
use tokio::sync::RwLock;

static MANIFEST_CACHE: RwLock<Option<Arc<PedestrianManifestResult>>> = RwLock::const_new(None);
static CHARACTER_CACHE: RwLock<Option<super::lru::LruCache<FetchGlbResponse>>> =
    RwLock::const_new(None);

/// Fetches and caches the pedestrian manifest from the content server.
pub async fn fetch_pedestrian_manifest(
    args: FetchArgs,
) -> anyhow::Result<PedestrianManifestResult> {
    {
        let guard = MANIFEST_CACHE.read().await;
        if let Some(cache) = &*guard {
            return Ok((**cache).clone());
        }
    }

    let mut guard = MANIFEST_CACHE.write().await;
    if let Some(cache) = &*guard {
        return Ok((**cache).clone());
    }

    let base_url = args.base_url.trim_end_matches('/');
    let folder = format!("{}/3d_data/pedestrian_3d_gen/", base_url);
    let manifest_url = format!("{}manifest.txt", folder);
    tracing::info!("Worker fetching pedestrian manifest from {}", manifest_url);

    let text = super::http::http_get_text(&manifest_url).await?;
    let mut urls = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        urls.push(format!("{}{}", folder, line));
    }

    let mut animations = Vec::new();
    if let Some(first_url) = urls.first() {
        tracing::info!(
            "Worker fetching first GLB for animation bootstrap from {}",
            first_url
        );
        let glb_bytes = super::http::http_get_bytes(first_url).await?;
        let gltf = gltf::Gltf::from_slice(&glb_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse GLB: {:?}", e))?;

        for animation in gltf.animations() {
            let name = animation.name().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let mut duration = 0.0f32;
            for sampler in animation.samplers() {
                let input = sampler.input();
                let max_time = if let Some(max_val) = input.max() {
                    if let Some(arr) = max_val.as_array() {
                        arr.first().and_then(|v| v.as_f64())
                    } else {
                        max_val.as_f64()
                    }
                } else {
                    None
                };
                if let Some(t) = max_time {
                    let t_f32 = t as f32;
                    if t_f32 > duration {
                        duration = t_f32;
                    }
                }
            }
            let frames = (duration * 30.0).round() as u32;
            animations.push(AnimationMeta {
                name,
                duration,
                frames,
            });
        }
        tracing::info!(
            "Worker parsed {} animations from GLB {}",
            animations.len(),
            first_url
        );
    }

    let result = PedestrianManifestResult { urls, animations };
    let arc = Arc::new(result);
    *guard = Some(arc.clone());
    Ok((*arc).clone())
}

/// Fetches one pedestrian GLB model, using the worker LRU cache when possible.
pub async fn fetch_pedestrian_model(req: FetchGlbRequest) -> anyhow::Result<FetchGlbResponse> {
    let t0 = _crack_utils::get_timestamp_now_ms();

    // Check cache
    {
        let mut guard = CHARACTER_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| super::lru::LruCache::new(50));
        if let Some(mut cached) = cache.get(&req.asset_id) {
            cached.from_cache = true;
            let t1 = _crack_utils::get_timestamp_now_ms();
            tracing::debug!(
                "Character cache HIT: {} (took {} ms)",
                req.asset_id,
                t1 - t0
            );
            return Ok(cached);
        }
    }

    // Cache miss: fetch from url
    let url = format!(
        "{}/{}",
        req.base_url.trim_end_matches('/'),
        req.glb_path.trim_start_matches('/')
    );
    let glb_bytes = super::http::http_get_bytes(&url).await?;

    let response = FetchGlbResponse {
        asset_id: req.asset_id.clone(),
        glb_bytes: glb_bytes.to_vec(),
        from_cache: false,
    };

    // Store in cache
    {
        let mut guard = CHARACTER_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| super::lru::LruCache::new(50));
        cache.insert(req.asset_id.clone(), response.clone());
    }

    let t1 = _crack_utils::get_timestamp_now_ms();
    tracing::debug!(
        "Character fetch completed: {} (total: {} ms, bytes: {})",
        req.asset_id,
        t1 - t0,
        response.glb_bytes.len()
    );

    Ok(response)
}
