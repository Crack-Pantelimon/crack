## Summary  

The sky‑cloud visual system isn’t part of the `visual_fx` particle‑effects plugin at all – it lives in a dedicated **`cloud_sky`** plugin. All of the clouds, sky dome, precipitation overlay, and ground‑shadow effects are rendered by a single procedural WGSL shader (`skybox_clouds.wgsl`) that is embedded in `materials.rs`.  

Key points:

* **Shader & assets** – The entire effect is procedural; there are no texture files. The three embedded WGSL shaders (`skybox_clouds.wgsl`, `precip_overlay.wgsl`, `ground_shadow.wgsl`) live in `materials.rs` and are referenced via `embedded://…`.  
* **Plugin registration** – `CloudSkyPlugin` (`plugins/cloud_sky/mod.rs`) adds the required material plugins, spawns the sky‑dome sphere and ground‑shadow quad, and sets up the update/render systems. It is hooked into the app via `MainScenePlugin` in `main_game_plugin.rs`.  
* **Settings & UI** – `CloudSkySettings` (`settings.rs`) exposes fields such as `time_of_day`, `wind_speed`, `wind_direction_deg`, and intensity flags. The UI window (`ui.rs`) binds sliders/checkboxes to these fields; the visibility of the control window is toggled by `UiState.show_clouds_sky`. There is **no separate `enable_clouds` flag** – the sky dome is always present as the background; turning the UI off only hides the controls.  
* **Runtime updates** – The sky dome follows the camera each frame (`follow_camera` system). Cloud animation and sun position are computed in the shader using `u.wind.xy * globals.time` and a `time_of_day` uniform that is updated by `sync_sky_uniforms`. All parameters are refreshed from the `CloudSkySettings` resource every frame.  
* **Runtime toggle** – The only way to hide the whole effect is to disable the `cloud_sky` plugin (not exposed in the UI). The UI toggle merely shows/hides the settings panel; the background sky remains rendered.  

### Specific file references  

- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/mod.rs:33-52`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/settings.rs:8-30`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/ui.rs:12-60`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/system.rs:108-135`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/materials.rs:67-68`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/materials.rs:112-115`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/system.rs:164-185`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/skybox_clouds.wgsl:47-83`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/skybox_clouds.wgsl:112`  
- `crack_demo/demo_resolution_selector_web_bevy/src/plugins/cloud_sky/generate_cloud_shadow_image` (location inside `systems.rs` – lines where the CPU‑generated shadow texture is created)  

These paths and line ranges correspond to the actual locations where the sky‑cloud system is implemented, configured, and updated.