# Plan

## Initial build/check instructions

```bash
# From repository root
cd crack_demo/demo_resolution_selector_web_bevy

# Build the project (checks for compilation errors)
cargo check --release 2>&1 | head -50

# Run any existing tests
cargo test 2>&1 | head -50

# Verify the project runs (if needed for manual testing later)
# cargo run --release
```

## Problem statement

The sky/sun rendering system in `plugins/cloud_sky/` currently uses **hardcoded warm colors** for the sun disc and glow in the shader (`skybox_clouds.wgsl`), and the world `DirectionalLight` color is always pure white (only intensity changes with time of day). There is no user control over sunlight color temperature, nor does the system automatically adjust color temperature based on the time of day.

The task requires:
1. **Manual control**: Add a sunlight temperature slider (1500K–6000K) at the top of the "Sky" UI section
2. **Shader integration**: Use this temperature to color the sun disc and atmospheric glow in the sky shader
3. **World lighting integration**: Apply the temperature to the `DirectionalLight` color in `sync_sun_light()`
4. **Time-of-day automation**: Override the manual temperature based on `time_of_day` — warm at morning/dusk (~2000–3000K), neutral/cool at noon (~5000–6000K), appropriate night values

The relevant files are all in `plugins/cloud_sky/`:
- `settings.rs` — `CloudSkySettings` resource (holds `time_of_day`, needs new `sun_temperature`)
- `materials.rs` — `SkyParamsUniform` struct (passes data to shader)
- `systems.rs` — `sync_sky_uniforms()` and `sync_sun_light()` (push settings to GPU and world lighting)
- `skybox_clouds.wgsl` — Sky shader with `sky_color()` computing sun disc/glow
- `ui.rs` — Egui window with collapsible sections

---

## Changes

### 1. `plugins/cloud_sky/settings.rs` — Add `sun_temperature` field to `CloudSkySettings`

**Lines ~5-45** — Current struct has `time_of_day`, `cloud_*`, `star_*` fields. Add temperature with default ~4000K (neutral noon).

```rust
// BEFORE (approx lines 5-25)
#[derive(Resource, Clone, Copy, Debug)]
pub struct CloudSkySettings {
    pub time_of_day: f32,           // 0.0 - 24.0
    pub cloud_coverage: f32,
    pub cloud_density: f32,
    pub cloud_speed: f32,
    pub star_brightness: f32,
    // ... other fields
}

// AFTER
#[derive(Resource, Clone, Copy, Debug)]
pub struct CloudSkySettings {
    pub time_of_day: f32,           // 0.0 - 24.0
    pub sun_temperature: f32,       // 1500.0 - 6000.0 (Kelvin)
    pub cloud_coverage: f32,
    pub cloud_density: f32,
    pub cloud_speed: f32,
    pub star_brightness: f32,
    // ... other fields
}

impl Default for CloudSkySettings {
    fn default() -> Self {
        Self {
            time_of_day: 12.0,
            sun_temperature: 4000.0,  // NEW: neutral default
            cloud_coverage: 0.5,
            cloud_density: 0.5,
            cloud_speed: 1.0,
            star_brightness: 1.0,
            // ...
        }
    }
}
```

**Motivation**: Central settings resource; adding the field here makes it available to UI, systems, and shader uniformly. Default 4000K is a neutral "late morning" value.

---

### 2. `plugins/cloud_sky/ui.rs` — Add temperature slider at top of "Sky" section

**Lines ~15-50** — Current `cloud_sky_window()` has a collapsible "Sky" section with `time_of_day` slider. Insert temperature slider **above** time-of-day.

```rust
// BEFORE (inside "Sky" collapsing header)
ui.add(egui::Slider::new(&mut settings.time_of_day, 0.0..=24.0).text("Time of Day"));

// AFTER — insert BEFORE time_of_day slider
ui.add(
    egui::Slider::new(&mut settings.sun_temperature, 1500.0..=6000.0)
        .text("Sun Temperature (K)")
        .suffix(" K")
        .logarithmic(false)
);
ui.add(egui::Slider::new(&mut settings.time_of_day, 0.0..=24.0).text("Time of Day"));
```

**Motivation**: User-facing control; placing it at top of "Sky" section matches the requirement. Logarithmic=false gives linear Kelvin steps which is intuitive for temperature.

---

### 3. `plugins/cloud_sky/materials.rs` — Extend `SkyParamsUniform` to include temperature

**Lines ~10-50** — Current uniform has `sun_dir` (Vec4 with day_factor in w). Add temperature as a new component (e.g., in a new Vec4 or extend existing).

```rust
// BEFORE (approx lines 10-30)
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct SkyParamsUniform {
    pub sun_dir: Vec4,        // xyz = direction, w = day_factor (0-1)
    pub cloud_coverage: f32,
    pub cloud_density: f32,
    // ...
}

// AFTER — add temperature field
#[derive(ShaderType, Clone, Copy, Debug)]
pub struct SkyParamsUniform {
    pub sun_dir: Vec4,        // xyz = direction, w = day_factor (0-1)
    pub sun_temperature: f32, // NEW: Kelvin (1500-6000)
    pub cloud_coverage: f32,
    pub cloud_density: f32,
    // ...
}
```

**Motivation**: Uniform buffer is the bridge from CPU settings to GPU shader. Adding `sun_temperature` here makes it available in WGSL.

---

### 4. `plugins/cloud_sky/systems.rs` — Update `sync_sky_uniforms()` and `sync_sun_light()`

**A. `sync_sky_uniforms()` (lines ~80-100)** — Push temperature to uniform:

```rust
// BEFORE
fn sync_sky_uniforms(
    settings: Res<CloudSkySettings>,
    mut uniforms: ResMut<SkyParamsUniform>,
) {
    let (sun_dir, day_factor) = settings.sun_dir_and_day_factor();
    uniforms.sun_dir = Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, day_factor);
    // ... other fields
}

// AFTER
fn sync_sky_uniforms(
    settings: Res<CloudSkySettings>,
    mut uniforms: ResMut<SkyParamsUniform>,
) {
    let (sun_dir, day_factor) = settings.sun_dir_and_day_factor();
    uniforms.sun_dir = Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, day_factor);
    uniforms.sun_temperature = settings.sun_temperature; // NEW
    // ... other fields
}
```

**B. `sync_sun_light()` (lines ~100-120)** — Apply temperature to `DirectionalLight.color`:

```rust
// BEFORE
fn sync_sun_light(
    settings: Res<CloudSkySettings>,
    mut sun_query: Query<&mut DirectionalLight, With<SunLight>>,
    mut ambient_query: Query<&mut AmbientLight>,
) {
    let (_, day_factor) = settings.sun_dir_and_day_factor();
    let illuminance = 30.0 + 3470.0 * day_factor;
    let ambient_brightness = 150.0 + 850.0 * day_factor;
    
    for mut light in &mut sun_query {
        light.illuminance = illuminance;
        // light.color = Color::WHITE; // implicit
    }
    // ...
}

// AFTER — add Kelvin-to-RGB conversion and apply to light.color
fn kelvin_to_rgb(kelvin: f32) -> Color {
    // Standard algorithm: map Kelvin to sRGB (clamped 1000-40000K)
    let k = kelvin.clamp(1000.0, 40000.0) / 100.0;
    let (r, g, b) = if k <= 66.0 {
        (255.0,
         (99.4708 * k.ln() - 161.1195).clamp(0.0, 255.0),
         if k <= 19.0 { 0.0 } else { (138.5177 * (k - 10.0).ln() - 305.0448).clamp(0.0, 255.0) })
    } else {
        ((329.6987 * (k - 60.0).powf(-0.133204759)).clamp(0.0, 255.0),
         (288.12216 * (k - 60.0).powf(-0.0755148492)).clamp(0.0, 255.0),
         255.0)
    };
    Color::srgb(r / 255.0, g / 255.0, b / 255.0)
}

fn sync_sun_light(
    settings: Res<CloudSkySettings>,
    mut sun_query: Query<&mut DirectionalLight, With<SunLight>>,
    mut ambient_query: Query<&mut AmbientLight>,
) {
    let (_, day_factor) = settings.sun_dir_and_day_factor();
    let illuminance = 30.0 + 3470.0 * day_factor;
    let ambient_brightness = 150.0 + 850.0 * day_factor;
    let sun_color = kelvin_to_rgb(settings.sun_temperature); // NEW
    
    for mut light in &mut sun_query {
        light.illuminance = illuminance;
        light.color = sun_color; // NEW: apply temperature color
    }
    // Ambient light could also get a tinted version (optional)
    // ...
}
```

**Motivation**: 
- `sync_sky_uniforms`: Makes temperature available to sky shader for disc/glow coloring
- `sync_sun_light`: Applies temperature to actual world lighting (DirectionalLight), not just sky visuals. The `kelvin_to_rgb` helper is a standard approximation used in graphics.

---

### 5. `plugins/cloud_sky/skybox_clouds.wgsl` — Use temperature for sun disc and glow colors

**Lines ~140-180** — Current `sky_color()` uses hardcoded `vec3(1.0, 0.95, 0.85)` for disc and `mix(vec3(1.0,0.9,0.7), vec3(1.0,0.55,0.25), low_sun)` for glow. Replace with temperature-based color.

```wgsl
// BEFORE (inside sky_color function)
let sun_disc_color = vec3(1.0, 0.95, 0.85);
let sun_disc = sun_disc_color * pow(sundot, 1800.0) * 8.0 * day_factor;

let low_sun = smoothstep(0.0, 0.15, sun_dir.y);
let warm_glow = mix(vec3(1.0, 0.9, 0.7), vec3(1.0, 0.55, 0.25), low_sun);
// ... warm_glow used in horizon/atmosphere

// AFTER — add kelvin_to_rgb function and use it
fn kelvin_to_rgb(kelvin: f32) -> vec3<f32> {
    let k = clamp(kelvin / 100.0, 10.0, 400.0);
    var r: f32;
    var g: f32;
    var b: f32;
    if (k <= 66.0) {
        r = 255.0;
        g = clamp(99.4708 * log(k) - 161.1195, 0.0, 255.0);
        if (k <= 19.0) {
            b = 0.0;
        } else {
            b = clamp(138.5177 * log(k - 10.0) - 305.0448, 0.0, 255.0);
        }
    } else {
        r = clamp(329.6987 * pow(k - 60.0, -0.133204759), 0.0, 255.0);
        g = clamp(288.12216 * pow(k - 60.0, -0.0755148492), 0.0, 255.0);
        b = 255.0;
    }
    return vec3<f32>(r, g, b) / 255.0;
}

// In sky_color():
let sun_color = kelvin_to_rgb(params.sun_temperature);
let sun_disc = sun_color * pow(sundot, 1800.0) * 8.0 * day_factor;

let low_sun = smoothstep(0.0, 0.15, sun_dir.y);
// Blend between temperature color and a cooler overhead color for glow
let overhead_color = vec3<f32>(0.9, 0.95, 1.0); // slight blue tint for high sun
let warm_glow = mix(sun_color, overhead_color, low_sun);
// ... use warm_glow for horizon/atmosphere
```

**Motivation**: The shader currently has no temperature awareness. This change makes the sun disc and atmospheric glow respect the temperature setting. The `kelvin_to_rgb` function mirrors the CPU-side one for consistency.

---

### 6. `plugins/cloud_sky/systems.rs` (or new system) — Auto-override temperature based on `time_of_day`

**Option A: In `sync_sun_light()` or `sync_sky_uniforms()`** — Compute auto-temperature each frame and override `settings.sun_temperature` if "auto mode" is enabled.

**Option B: New system `auto_sun_temperature()`** — Runs after time-of-day changes, updates settings.

Since the requirement says "overwrite this value when we change the time of day", I'll add an **auto-mode toggle** in settings and a system that updates temperature based on time.

**Add to `settings.rs`**:
```rust
// In CloudSkySettings
pub auto_temperature: bool, // default true
```

**Add to `ui.rs`** (in Sky section, near temperature slider):
```rust
ui.checkbox(&mut settings.auto_temperature, "Auto Temperature (Time of Day)");
```

**New system in `systems.rs`** (or inline in existing sync):
```rust
fn auto_sun_temperature(mut settings: ResMut<CloudSkySettings>) {
    if !settings.auto_temperature {
        return;
    }
    let t = settings.time_of_day;
    // Map 0-24h to temperature:
    // Night (0-5, 21-24): ~4000K (moonlight/cool)
    // Dawn (5-7): 2000K -> 3500K (warm)
    // Morning (7-10): 3500K -> 5000K
    // Noon (10-14): 5000K -> 6000K (peak)
    // Afternoon (14-17): 6000K -> 5000K
    // Dusk (17-19): 5000K -> 2500K (warm)
    // Evening (19-21): 2500K -> 4000K
    
    settings.sun_temperature = match t {
        t if t < 5.0 || t >= 21.0 => 4000.0,           // Night
        t if t < 7.0 => lerp(2000.0, 3500.0, (t - 5.0) / 2.0),     // Dawn
        t if t < 10.0 => lerp(3500.0, 5000.0, (t - 7.0) / 3.0),    // Morning
        t if t < 14.0 => lerp(5000.0, 6000.0, (t - 10.0) / 4.0),   // Noon
        t if t < 17.0 => lerp(6000.0, 5000.0, (t - 14.0) / 3.0),   // Afternoon
        t if t < 19.0 => lerp(5000.0, 2500.0, (t - 17.0) / 2.0),   // Dusk
        _ => lerp(2500.0, 4000.0, (t - 19.0) / 2.0),                // Evening
    };
}
```

Register this system to run **before** `sync_sky_uniforms` and `sync_sun_light` in the plugin schedule.

**Motivation**: Implements the "overwrite when time of day changes" requirement. The auto-mode toggle lets users disable it if they want full manual control. The temperature curve matches photographic reality: warm at golden hours, cool at noon, neutral at night.

---

## What NOT to change

| File / Behavior | Reason |
|-----------------|--------|
| `plugins/visual_fx/` (any file) | Wrong subsystem — handles particle effects (fireballs, smoke), not sky/sun |
| `CloudSkySettings.time_of_day` range or behavior | Core time control; only reads are needed |
| `sun_dir_and_day_factor()` math | Sun position logic is correct; only consumes time_of_day |
| `DirectionalLight.illuminance` / `AmbientLight.brightness` formulas | Intensity curves are fine; only `color` needs temperature |
| Sky shader cloud/star rendering | Unrelated to sun disc temperature |
| Egui window structure beyond adding controls | Layout works; just insert new widgets |
| Any other plugin or system | Changes are localized to `cloud_sky` |

---

## Automatic verification

```bash
# 1. Build check — must compile cleanly
cd crack_demo/demo_resolution_selector_web_bevy
cargo check --release 2>&1

# 2. Run tests (if any exist)
cargo test 2>&1

# 3. Clippy lints — no new warnings
cargo clippy --release 2>&1 | grep -v "^$" | head -30

# 4. Format check
cargo fmt --check 2>&1
```

All commands should pass with zero errors. The project uses standard Rust tooling; no custom test runners.

---

## Manual verification

1. **Launch the game** (`cargo run --release`)
2. **Open the Sky UI panel** (typically via a key binding like `F1` or menu — check `ui.rs` for toggle)
3. **Verify the new controls appear at top of "Sky" section**:
   - [ ] "Sun Temperature (K)" slider (1500–6000)
   - [ ] "Auto Temperature (Time of Day)" checkbox
4. **Test manual mode** (uncheck auto):
   - [ ] Drag slider to 2000K → sun disc turns deep orange/red, world light warms
   - [ ] Drag slider to 6000K → sun disc turns pale blue-white, world light cools
   - [ ] Intermediate values (3000K, 4000K, 5000K) show smooth transitions
5. **Test auto mode** (check auto, move time-of-day slider):
   - [ ] 06:00 (dawn) → ~2000–2500K (warm)
   - [ ] 09:00 (morning) → ~4000–4500K
   - [ ] 12:00 (noon) → ~5500–6000K (cool/neutral)
   - [ ] 18:00 (dusk) → ~2500–3000K (warm)
   - [ ] 00:00 (night) → ~4000K (cool moonlight)
   - [ ] Verify slider **updates automatically** as time-of-day changes
6. **Verify world lighting matches sky**:
   - [ ] Place an object in scene; observe its lit color matches sun disc hue
   - [ ] Shadows receive the same color temperature tint
7. **Check persistence** (if settings are saved):
   - [ ] Restart game; verify last temperature/auto-mode retained

---

## Overview / Summary

**Goal**: Add user-controllable and time-of-day-automatic sunlight color temperature (1500K–6000K) to the sky/sun system, affecting both the rendered sun disc/glow in the sky shader and the world `DirectionalLight` color.

**Solution shape**:
1. **Data**: Add `sun_temperature` (+ `auto_temperature`) to `CloudSkySettings`
2. **UI**: Slider + checkbox at top of "Sky" section in Egui
3. **GPU**: Pass temperature via `SkyParamsUniform` → WGSL `kelvin_to_rgb()` → sun disc & glow
4. **World lighting**: CPU `kelvin_to_rgb()` → `DirectionalLight.color` in `sync_sun_light()`
5. **Automation**: New system maps `time_of_day` → temperature curve when auto-mode enabled

**Main risks**:
- **Shader/CPU color mismatch**: Both `kelvin_to_rgb` implementations (Rust + WGSL) must match precisely. Verify by comparing outputs at key temperatures (2000K, 4000K, 6000K).
- **Auto-mode fights manual edits**: User moves slider → auto system overwrites next frame. Mitigation: auto system only runs when `auto_temperature == true` (checkbox unchecked = manual).
- **Night temperature**: 4000K is a reasonable "moonlight" default, but could feel too warm. May need tuning after visual review.
- **Performance**: Negligible — one extra float in uniform, simple math in shader.

**Files touched** (5 total):
- `plugins/cloud_sky/settings.rs` — struct fields + defaults
- `plugins/cloud_sky/ui.rs` — slider + checkbox
- `plugins/cloud_sky/materials.rs` — uniform struct
- `plugins/cloud_sky/systems.rs` — sync functions + auto system + kelvin_to_rgb
- `plugins/cloud_sky/skybox_clouds.wgsl` — shader kelvin_to_rgb + color usage

No other files or plugins require changes.

Remember: DO NOT write or edit any files yet. This is a read-only exploration and planning phase.
