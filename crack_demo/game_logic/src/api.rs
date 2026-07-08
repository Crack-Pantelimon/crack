use api_asscrack::declare_api_group2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchArgs {
    pub base_url: String,
}

declare_api_group2! { GameLogicApiGroup, [
    (FetchMapManifest, FetchArgs, crate::map::MapManifestResult),
    (FetchFakeMapTiles, FetchArgs, Vec<crate::map::FakeMapTile>),
    (FetchOsmData, FetchArgs, crate::osm::OsmDataResult),
    (ComputeLodChanges, crate::lod::LodComputeRequest, crate::lod::LodComputeResponse),
    (RunGameMigrations, (), ()),
    (FetchMapTile, crate::tile::FetchTileRequest, crate::tile::FetchTileResponse),
    (FetchPedestrianManifest, FetchArgs, crate::pedestrian::PedestrianManifestResult),
    (FetchPedestrianModel, crate::glb::FetchGlbRequest, crate::glb::FetchGlbResponse),
    (FetchWeaponManifest, FetchArgs, crate::weapon::WeaponManifestResult),
    (FetchWeaponModel, crate::glb::FetchGlbRequest, crate::glb::FetchGlbResponse),
] }
