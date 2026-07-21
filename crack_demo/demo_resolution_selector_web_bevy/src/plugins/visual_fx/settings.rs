use bevy::prelude::*;

/// vfx settings.
#[derive(Resource, Clone, Copy, Debug)]
pub struct VfxSettings {
    // category master toggles (default true)
/// car fireball field.
    pub car_fireball: bool,
/// car smoke field.
    pub car_smoke: bool,
/// car black smoke field.
    pub car_black_smoke: bool,
/// gun gizmos field.
    pub gun_gizmos: bool, // keep gizmos (alpha 0.3)
/// gun tracer field.
    pub gun_tracer: bool,
/// gun hit sparks field.
    pub gun_hit_sparks: bool,
/// gun muzzle flash field.
    pub gun_muzzle_flash: bool,
/// gun muzzle smoke field.
    pub gun_muzzle_smoke: bool,
/// car explosion gizmos field.
    pub car_explosion_gizmos: bool, // 3 damage wireframe spheres (default off)
/// disabled car gizmos field.
    pub disabled_car_gizmos: bool,  // green warning sphere around disabled cars (default off)

    // sliders
/// fireball lifetime field.
    pub fireball_lifetime: f32,
/// fireball radius field.
    pub fireball_radius: f32,
/// smoke lifetime field.
    pub smoke_lifetime: f32,
/// smoke opacity field.
    pub smoke_opacity: f32,
/// tracer width field.
    pub tracer_width: f32,
/// spark count scale field.
    pub spark_count_scale: f32,
/// muzzle smoke every field.
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
