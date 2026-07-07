use crate::api::*;
use api_asscrack::implement_api_group2;

pub mod http;
pub mod manifest_impl;
pub mod models;
pub mod osm_impl;

implement_api_group2! { GameLogicApiGroup, [
    (FetchMapManifest, manifest_impl::fetch_map_manifest),
    (FetchOsmData, osm_impl::fetch_osm_data),
    (ComputeLodChanges, compute_lod_changes_api),
    (RunGameMigrations, models::run_game_migrations),
] }

async fn compute_lod_changes_api(
    req: crate::lod::LodComputeRequest,
) -> anyhow::Result<crate::lod::LodComputeResponse> {
    let manifest = manifest_impl::get_manifest_cache().await?;
    Ok(crate::lod::compute_lod_changes(&manifest, &req))
}
