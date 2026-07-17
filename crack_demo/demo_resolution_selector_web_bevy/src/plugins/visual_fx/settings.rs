use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug)]
pub struct VfxSettings {
    // category master toggles (default true)
    pub car_fireball: bool,
    pub car_smoke: bool,
    pub car_black_smoke: bool,
    pub gun_gizmos: bool, // keep gizmos (alpha 0.3)
    pub gun_tracer: bool,
    pub gun_hit_sparks: bool,
    pub gun_muzzle_flash: bool,
    pub gun_muzzle_smoke: bool,
    pub car_explosion_gizmos: bool, // 3 damage wireframe spheres (default off)
    pub disabled_car_gizmos: bool,  // green warning sphere around disabled cars (default off)

    // sliders
    pub fireball_lifetime: f32,
    pub fireball_radius: f32,
    pub smoke_lifetime: f32,
    pub smoke_opacity: f32,
    pub tracer_width: f32,
    pub spark_count_scale: f32,
    pub muzzle_smoke_every: u32,
}

impl Default for VfxSettings {
    fn default() -> Self {
        Self {
            car_fireball: true,
            car_smoke: true,
            car_black_smoke: true,
            gun_gizmos: true,
            gun_tracer: true,
            gun_hit_sparks: true,
            gun_muzzle_flash: true,
            gun_muzzle_smoke: true,
            car_explosion_gizmos: false, // default off
            disabled_car_gizmos: false,  // default off
            fireball_lifetime: 0.6,
            fireball_radius: 4.0,
            smoke_lifetime: 1.5,
            smoke_opacity: 0.8,
            tracer_width: 0.04,
            spark_count_scale: 1.0,
            muzzle_smoke_every: 3,
        }
    }
}
