use bevy::prelude::*;
use bevy::asset::embedded_asset;
use bevy_egui::EguiPrimaryContextPass;

pub mod settings;
pub mod materials;
pub mod spawn;
pub mod car_explosion;
pub mod smoke_emitter;
pub mod gun_fx;
pub mod clouds;
pub mod ui;

pub use car_explosion::CarExplosionEvent;
pub use gun_fx::GunFxEvent;
pub use smoke_emitter::SmokeEmitter;

use settings::VfxSettings;
use materials::{AdditiveFxMaterial, BlendFxMaterial, CloudMaterial};
use spawn::{setup_vfx_meshes, despawn_expired_fx, tick_vfx_drift};
use car_explosion::{draw_explosion_flashes, car_explosion_observer, ExplosionFlashes};
use smoke_emitter::tick_smoke_emitters;
use gun_fx::{gun_fx_observer, tick_gun_smoke_emitters};
use clouds::{setup_clouds, sync_cloud_uniforms};
use ui::vfx_controls_window;

pub struct VisualFXPlugin;

impl Plugin for VisualFXPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "billboard_fx.wgsl");
        embedded_asset!(app, "clouds.wgsl");

        app.init_resource::<VfxSettings>()
            .init_resource::<ExplosionFlashes>()
            .add_plugins(MaterialPlugin::<AdditiveFxMaterial>::default())
            .add_plugins(MaterialPlugin::<BlendFxMaterial>::default())
            .add_plugins(MaterialPlugin::<CloudMaterial>::default())
            .add_observer(car_explosion_observer)
            .add_observer(gun_fx_observer)
            .add_systems(Startup, (setup_vfx_meshes, setup_clouds))
            .add_systems(Update, (
                despawn_expired_fx,
                tick_vfx_drift,
                tick_smoke_emitters,
                tick_gun_smoke_emitters,
                draw_explosion_flashes,
                sync_cloud_uniforms,
                debug_spawn_fireball_on_keypress,
            ))
            .add_systems(EguiPrimaryContextPass, vfx_controls_window);
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
