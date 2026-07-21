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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_fetch_args_serde_round_trip() {
        let args = FetchArgs {
            base_url: "http://localhost:8080".to_string(),
        };
        let json = serde_json::to_string(&args).unwrap();
        let back: FetchArgs = serde_json::from_str(&json).unwrap();
        assert_eq!(back.base_url, args.base_url);
    }
}
