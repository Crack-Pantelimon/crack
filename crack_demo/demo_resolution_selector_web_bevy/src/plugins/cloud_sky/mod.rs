//! Procedural sky plugin: blue-sky gradient, sun, day/night cycle, three
//! cloud layer types (cumulus / cirrus / storm), rain & snow overlay and
//! scrolling cloud shadows on the ground — all WebGL2-safe custom WGSL.

use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

pub mod materials;
pub mod settings;
pub mod systems;
pub mod ui;

use materials::{CloudGroundShadowMaterial, PrecipOverlayMaterial, SkyDomeMaterial};
use settings::CloudSkySettings;
use systems::{
    auto_sun_temperature, follow_camera, setup_cloud_sky, sync_sky_uniforms, sync_sun_light,
};
use ui::cloud_sky_window;

pub struct CloudSkyPlugin;

impl Plugin for CloudSkyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "skybox_clouds.wgsl");
        embedded_asset!(app, "precip_overlay.wgsl");
        embedded_asset!(app, "ground_shadow.wgsl");

        app.init_resource::<CloudSkySettings>()
            // Binaries that pull this plugin in via `SetupDebugScenePlugin`
            // don't have `UiEguiPlugin`, so make sure the resource exists.
            .init_resource::<crate::ui_egui::UiState>()
            .add_plugins(MaterialPlugin::<SkyDomeMaterial>::default())
            .add_plugins(MaterialPlugin::<PrecipOverlayMaterial>::default())
            .add_plugins(MaterialPlugin::<CloudGroundShadowMaterial>::default())
            .add_systems(Startup, setup_cloud_sky)
            .add_systems(
                Update,
                (
                    follow_camera,
                    auto_sun_temperature,
                    sync_sky_uniforms,
                    sync_sun_light,
                )
                    .chain(),
            );

        app.add_systems(EguiPrimaryContextPass, cloud_sky_window);
    }
}
