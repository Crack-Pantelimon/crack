use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::config::DATA_BASE_URL;

pub fn get_car_asset(car_type: &str, asset_server: &AssetServer) -> Handle<WorldAsset> {
    let path = format!(
        "{}/3d_data/3d_slop_models_clean/cars/{}.glb",
        DATA_BASE_URL, car_type
    );
    asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))
}

pub fn car_list() -> &'static [&'static str] {
    &["dacie-1b", "dacie-2b", "dacie-3b"]
}

pub fn get_random_car_type() -> &'static str {
    car_list().choose(&mut rand::rng()).unwrap()
}
