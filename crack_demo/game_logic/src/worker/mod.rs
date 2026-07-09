use crate::api::*;
use api_asscrack::implement_api_group2;

pub mod http;
pub mod lru;
pub mod manifest_impl;
pub mod models;
pub mod osm_impl;
pub mod pedestrian_impl;
pub mod tile_impl;
pub mod weapon_impl;

implement_api_group2! { GameLogicApiGroup, [
    (FetchMapManifest, manifest_impl::fetch_map_manifest),
    (FetchFakeMapTiles, manifest_impl::fetch_fake_map_tiles),
    (FetchOsmData, osm_impl::fetch_osm_data),
    (ComputeLodChanges, compute_lod_changes_api),
    (RunGameMigrations, models::run_game_migrations),
    (FetchMapTile, tile_impl::fetch_map_tile),
    (FetchPedestrianManifest, pedestrian_impl::fetch_pedestrian_manifest),
    (FetchPedestrianModel, pedestrian_impl::fetch_pedestrian_model),
    (FetchWeaponManifest, weapon_impl::fetch_weapon_manifest),
    (FetchWeaponModel, weapon_impl::fetch_weapon_model),
] }

async fn compute_lod_changes_api(
    req: crate::lod::LodComputeRequest,
) -> anyhow::Result<crate::lod::LodComputeResponse> {
    let manifest = manifest_impl::get_manifest_cache().await?;
    Ok(crate::lod::compute_lod_changes(&manifest, &req).await)
}
