## Lay of the Land

Based on my exploration, the **sky/sun system** lives in the `cloud_sky` plugin, **not** the `visual_fx` plugin (which handles particle effects like fireballs/smoke). The explorer agent's summary incorrectly pointed to `visual_fx`.

### Key Files & Current Behavior

| File | Role | Key Elements |
|------|------|--------------|
| `plugins/cloud_sky/settings.rs:5-45` | Settings resource | `CloudSkySettings` with `time_of_day: f32` (0-24), `sun_dir_and_day_factor()` computes sun direction + day factor |
| `plugins/cloud_sky/materials.rs:10-50` | Shader uniforms | `SkyParamsUniform { sun_dir: Vec4(xyz=dir, w=day_factor), ... }` |
| `plugins/cloud_sky/skybox_clouds.wgsl:140-180` | Sky shader | `sky_color()` computes sun disc color: hardcoded `vec3(1.0, 0.95, 0.85)` for disc, warm glow uses `mix(vec3(1.0,0.9,0.7), vec3(1.0,0.55,0.25), low_sun)` |
| `plugins/cloud_sky/systems.rs:80-120` | Sync systems | `sync_sky_uniforms()` pushes settings→material; `sync_sun_light()` updates `DirectionalLight.illuminance` and `AmbientLight.brightness` based on `day_factor` |
| `plugins/cloud_sky/ui.rs:15-50` | UI panel | `cloud_sky_window()` with collapsible "Sky" section containing `time_of_day` slider (0-24) |
| `plugins/cloud_sky/mod.rs:18-30` | Plugin registration | Registers `CloudSkySettings` resource, materials, startup/setup systems, UI system |

### How the Sun Disc & Light Currently Work

1. **Time of day** (0-24) → `sun_dir_and_day_factor()` → sun direction (Vec3) + day_factor (0-1)
2. **Sky shader** (`skybox_clouds.wgsl`): Uses `sun_dir` + `day_factor` to render:
   - Sun disc: `vec3(1.0, 0.95, 0.85) * pow(sundot, 1800.0) * 8.0 * day`
   - Warm glow: interpolates between two hardcoded warm colors based on `low_sun`
3. **World lighting** (`sync_sun_light`): Sets `DirectionalLight.illuminance = 30.0 + 3470.0 * day_factor` and `AmbientLight.brightness = 150.0 + 850.0 * day_factor` — **color temperature not used**

---

## Hypotheses on User Intent

1. **Add a manual sunlight temperature slider** (1500K–6000K) in the "Sky" UI section, at the top
2. **Use this temperature** to:
   - Color the sun disc in the shader (replace hardcoded warm colors)
   - Color the directional light in `sync_sun_light()`
3. **Auto-override the temperature** based on `time_of_day`:
   - Morning (~6-9) → warm (~2000-3000K)
   - Noon (~11-14) → neutral/cool (~5000-6000K)  
   - Dusk (~17-20) → warm (~2000-3000K)
   - Night → cool/moonlight (~4000K or custom)

---

## Clarification Questions

Before finalizing the plan, I need to resolve a few ambiguities:


I've read all the relevant code in the cloud sky system. The key files are:

- **`settings.rs`** - `CloudSkySettings` with `time_of_day` slider (0-24h) and `sun_dir_and_day_factor()` 
- **`systems.rs`** - `sync_sun_light()` updates DirectionalLight + AmbientLight from time_of_day; `sync_sky_uniforms()` pushes settings to GPU
- **`skybox_clouds.wgsl`** - Sky shader with `sky_color()` computing sun disc/glow using hardcoded warm colors
- **`ui.rs`** - Egui window with collapsible sections for each setting category

The system already computes sun direction and day factor from `time_of_day`. The sun disc color in the shader is hardcoded to `vec3(1.0, 0.95, 0.85)` with a warm glow that varies by sun elevation.

---

I have 4 clarification questions whose answers will materially change the implementation approach: