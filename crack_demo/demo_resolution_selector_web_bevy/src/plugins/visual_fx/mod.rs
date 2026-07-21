use bevy::asset::embedded_asset;
use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

/// car explosion submodule.
pub mod car_explosion;
/// gun fx submodule.
pub mod gun_fx;
/// materials submodule.
pub mod materials;
/// settings submodule.
pub mod settings;
/// smoke emitter submodule.
pub mod smoke_emitter;
/// spawn submodule.
pub mod spawn;
/// ui submodule.
pub mod ui;

pub use car_explosion::CarExplosionEvent;
pub use gun_fx::GunFxEvent;
pub use smoke_emitter::SmokeEmitter;

use car_explosion::{ExplosionFlashes, car_explosion_observer, draw_explosion_flashes};
use gun_fx::{GunFxCounter, gun_fx_observer, tick_gun_smoke_emitters};
use materials::{AdditiveFxMaterial, BlendFxMaterial};
use settings::VfxSettings;
use smoke_emitter::tick_smoke_emitters;
use spawn::{despawn_expired_fx, setup_vfx_meshes, tick_vfx_drift};
use ui::vfx_controls_window;

/// demo submodule.
pub mod demo;

/// visual fxplugin.
pub struct VisualFXPlugin;

impl Plugin for VisualFXPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "billboard_fx.wgsl");

        app.init_resource::<VfxSettings>()
            .init_resource::<ExplosionFlashes>()
            .init_resource::<GunFxCounter>()
            .add_plugins(MaterialPlugin::<AdditiveFxMaterial>::default())
            .add_plugins(MaterialPlugin::<BlendFxMaterial>::default())
            .add_observer(car_explosion_observer)
            .add_observer(gun_fx_observer)
            .add_systems(Startup, setup_vfx_meshes)
            .add_systems(
                Update,
                (
                    despawn_expired_fx,
                    tick_vfx_drift,
                    tick_smoke_emitters,
                    tick_gun_smoke_emitters,
                    draw_explosion_flashes,
                ),
            );

        #[cfg(debug_assertions)]
        app.add_systems(Update, debug_spawn_fireball_on_keypress);

        app.add_systems(EguiPrimaryContextPass, vfx_controls_window);
    }
}

fn debug_spawn_fireball_on_keypress(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    meshes: Option<Res<spawn::VfxMeshes>>,
    mut additive_mats: ResMut<Assets<AdditiveFxMaterial>>,
) {
    if keys.just_pressed(KeyCode::KeyV) {
        if let Some(meshes) = meshes {
            info!("Spawning validation fireball at origin!");
            let now = time.elapsed_secs();
            let params = materials::BillboardParams {
                color: Vec4::new(1.0, 0.6, 0.1, 1.0),
                spawn_time: now,
                lifetime: 1.5,
                start_radius: 1.0,
                end_radius: 4.0,
                seed: rand::random::<f32>(),
                kind: materials::FxKind::Fireball as u32,
                _pad: 0.0,
            };
            spawn::spawn_additive_billboard_fx(
                &mut commands,
                &mut additive_mats,
                &meshes,
                &time,
                Vec3::new(0.0, 1.0, 0.0),
                params,
            );
        }
    }
}
