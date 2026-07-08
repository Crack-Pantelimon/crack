use crate::api::FetchArgs;
use crate::glb::{FetchGlbRequest, FetchGlbResponse};
use crate::weapon::{WeaponEntry, WeaponManifestResult};
use std::sync::Arc;
use tokio::sync::RwLock;

static MANIFEST_CACHE: RwLock<Option<Arc<WeaponManifestResult>>> = RwLock::const_new(None);
static WEAPON_CACHE: RwLock<Option<super::lru::LruCache<FetchGlbResponse>>> =
    RwLock::const_new(None);

pub async fn fetch_weapon_manifest(args: FetchArgs) -> anyhow::Result<WeaponManifestResult> {
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
    let folder = format!("{}/3d_data/3d_weapons/out2/", base_url);
    let manifest_url = format!("{}manifest.txt", folder);
    tracing::info!("Worker fetching weapon manifest from {}", manifest_url);

    let text = super::http::http_get_text(&manifest_url).await?;
    let mut weapons = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // CSV columns: path,is_gun,clip_size,bullet_type,damage,range,rpm,automatic
        let cols: Vec<&str> = line.split(',').map(str::trim).collect();
        let rel_path = cols[0];
        if rel_path == "path" {
            continue; // header
        }

        let full_path = format!("{}{}", folder, rel_path);
        let is_gun = cols
            .get(1)
            .and_then(|c| c.parse::<u32>().ok())
            .map(|v| v == 1)
            .unwrap_or_else(|| rel_path.starts_with("gun/"));

        let entry = WeaponEntry {
            path: full_path,
            is_gun,
            clip_size: cols.get(2).and_then(|c| c.parse().ok()).unwrap_or(10),
            bullet_type: cols.get(3).unwrap_or(&"9mm").to_string(),
            damage: cols.get(4).and_then(|c| c.parse().ok()).unwrap_or(20.0),
            range: cols.get(5).and_then(|c| c.parse().ok()).unwrap_or(50.0),
            rpm: cols.get(6).and_then(|c| c.parse().ok()).unwrap_or(300.0),
            automatic: cols.get(7).and_then(|c| c.parse::<u32>().ok()) == Some(1),
        };
        weapons.push(entry);
    }

    let result = WeaponManifestResult { weapons };
    let arc = Arc::new(result);
    *guard = Some(arc.clone());
    Ok((*arc).clone())
}

pub async fn fetch_weapon_model(req: FetchGlbRequest) -> anyhow::Result<FetchGlbResponse> {
    let t0 = _crack_utils::get_timestamp_now_ms();

    // Check cache
    {
        let mut guard = WEAPON_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| super::lru::LruCache::new(50));
        if let Some(mut cached) = cache.get(&req.asset_id) {
            cached.from_cache = true;
            let t1 = _crack_utils::get_timestamp_now_ms();
            tracing::debug!("Weapon cache HIT: {} (took {} ms)", req.asset_id, t1 - t0);
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
        let mut guard = WEAPON_CACHE.write().await;
        let cache = guard.get_or_insert_with(|| super::lru::LruCache::new(50));
        cache.insert(req.asset_id.clone(), response.clone());
    }

    let t1 = _crack_utils::get_timestamp_now_ms();
    tracing::debug!(
        "Weapon fetch completed: {} (total: {} ms, bytes: {})",
        req.asset_id,
        t1 - t0,
        response.glb_bytes.len()
    );

    Ok(response)
}
