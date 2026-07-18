use bevy::prelude::*;

/// All the tunables of the cloud skybox demo, driven by the egui sliders.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct CloudSkySettings {
    /// Hour of the day, 0..24. Sunrise ~6, noon 13, sunset ~19.
    pub time_of_day: f32,
    /// Sunlight color temperature in Kelvin (1500..6000). In auto mode this is
    /// overwritten every frame from `time_of_day`.
    pub sun_temperature: f32,
    /// When true, `auto_sun_temperature()` drives `sun_temperature` from
    /// `time_of_day` and the manual slider is disabled.
    pub auto_temperature: bool,
    /// Wind scroll speed of the cloud layers (uv units / second).
    pub wind_speed: f32,
    /// Wind direction in degrees (0 = +X, 90 = +Z).
    pub wind_direction_deg: f32,
    /// Global scale multiplier for the cloud noise.
    pub cloud_scale: f32,

    /// Coverage of the puffy low-altitude cumulus layer (0..1).
    pub cumulus_amount: f32,
    /// FBM octaves for the cumulus layer (1..8).
    pub cumulus_detail: f32,

    /// Coverage of the wispy high-altitude cirrus veil (0..1).
    pub cirrus_amount: f32,
    /// FBM octaves for the cirrus layer (1..8).
    pub cirrus_detail: f32,

    /// Coverage of the dark low storm layer (0..1).
    pub storm_amount: f32,
    /// FBM octaves for the storm layer (1..8).
    pub storm_detail: f32,

    /// Rain streak intensity (0..1).
    pub rain_intensity: f32,
    /// Snow flake intensity (0..1).
    pub snow_intensity: f32,

    /// Opacity of the cloud shadows projected on the ground (0..1).
    pub cloud_shadow_intensity: f32,
}

impl Default for CloudSkySettings {
    fn default() -> Self {
        Self {
            time_of_day: 14.5,
            sun_temperature: 5250.0,
            auto_temperature: true,
            wind_speed: 0.02,
            wind_direction_deg: 45.0,
            cloud_scale: 0.23,
            cumulus_amount: 0.55,
            cumulus_detail: 7.0,
            cirrus_amount: 0.45,
            cirrus_detail: 7.0,
            storm_amount: 0.5,
            storm_detail: 7.6,
            rain_intensity: 0.0,
            snow_intensity: 0.0,
            cloud_shadow_intensity: 0.35,
        }
    }
}

impl CloudSkySettings {
    /// Direction from a point on the ground toward the sun, plus a 0..1
    /// "day factor" (0 = deep night, 1 = full day), derived from `time_of_day`.
    pub fn sun_dir_and_day_factor(&self) -> (Vec3, f32) {
        let t = (self.time_of_day - 6.0) / 12.0 * std::f32::consts::PI;
        let elevation_sin = t.sin();
        // Azimuth sweeps east -> west over the day.
        let azimuth = (self.time_of_day - 12.0) / 12.0 * std::f32::consts::PI;
        let cos_el = elevation_sin.max(0.02).sqrt(); // cheap flattening near horizon
        let sun_dir = Vec3::new(
            azimuth.sin() * cos_el,
            elevation_sin.max(-1.0),
            -azimuth.cos() * cos_el,
        )
        .normalize_or_zero();
        let day_factor = smoothstep(-0.05, 0.15, elevation_sin);
        (sun_dir, day_factor)
    }

    /// Wind scroll vector in uv units / second.
    pub fn wind_vec(&self) -> Vec2 {
        let rad = self.wind_direction_deg.to_radians();
        Vec2::new(rad.cos(), rad.sin()) * self.wind_speed
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
