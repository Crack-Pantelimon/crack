use bevy::prelude::*;
use rand::seq::IndexedRandom;

use crate::config::DATA_BASE_URL;

/// get car asset.
pub fn get_car_asset(car_type: &str, asset_server: &AssetServer) -> Handle<WorldAsset> {
    let path = format!(
        "{}/3d_data/3d_slop_models_clean/cars/{}.glb",
        DATA_BASE_URL, car_type
    );
    asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))
}

/// get wheel asset.
pub fn get_wheel_asset(wheel_name: &str, asset_server: &AssetServer) -> Handle<WorldAsset> {
    let path = format!(
        "{}/3d_data/3d_slop_models_clean/cars/{}.glb",
        DATA_BASE_URL, wheel_name
    );
    asset_server.load(GltfAssetLabel::Scene(0).from_asset(path))
}

/// car list.
pub fn car_list() -> &'static [&'static str] {
    &["dacia-1c", "dacia-2c", "dacia-3c"]
}

/// get random car type.
pub fn get_random_car_type() -> &'static str {
    car_list().choose(&mut rand::rng()).unwrap()
}
