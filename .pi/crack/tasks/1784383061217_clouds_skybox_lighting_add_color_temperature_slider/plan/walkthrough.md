# Walkthrough — Sun Temperature Slider

## Step 1: Settings (`settings.rs`)

Added `sun_temperature: f32` and `auto_temperature: bool` to `CloudSkySettings` after
`time_of_day`. Defaults: `5250.0` K and `auto_temperature: true`.

## Step 2: UI (`ui.rs`)

Added "Sun Temperature (K)" slider (1500–6000) at the top of the Sky section, disabled
when auto mode is on. Added "Auto Temperature (Time of Day)" checkbox below it.

## Step 3: Uniform (`materials.rs`)

Extended `SkyParamsUniform` with `sun_temperature: f32` after `sun_dir`. Updated
`precip_overlay.wgsl` struct to match (bind-group layout lockstep).

## Step 4: Systems (`systems.rs`)

- `make_sky_params()` writes `sun_temperature` to the uniform.
- Added `kelvin_to_rgb()` for CPU-side color conversion.
- Added `auto_sun_temperature_from_time()` curve and `auto_sun_temperature()` system.
- `sync_sun_light()` applies `kelvin_to_rgb(settings.sun_temperature)` to
  `DirectionalLight.color`.

## Step 5: Shader (`skybox_clouds.wgsl`)

Added matching `kelvin_to_rgb()` and replaced hardcoded sun disc / glow colors with
temperature-based `sun_color` and `warm` glow blend.

## Step 6: Plugin registration (`mod.rs`)

Registered `auto_sun_temperature` in Update chain before `sync_sky_uniforms` and
`sync_sun_light`.

## Verification

Running `cargo check --release`, `cargo test`, `cargo clippy --release`, and
`cargo fmt --check`.
