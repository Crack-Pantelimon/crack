Q: Where are the sky‑cloud visual assets and related shader code defined in the Bevy project?  
A: The sky‑cloud effect is implemented in the `crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs` file, where the `AdditiveFxMaterial` and `BillboardParams` structs define the cloud texture and its rendering parameters. The actual shader used for drawing the clouds lives in the ` AdditiveFxMaterial` implementation, which creates a simple additive blend material that draws semi‑transparent cloud sprites onto the sky background.

Q: Which Bevy plugin registers the sky‑cloud rendering logic?  
A: The sky‑cloud rendering is registered by the `VisualFXPlugin` found in `crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/mod.rs`. This plugin adds the `AdditiveFxMaterial` to Bevy’s render pipeline and ensures it is loaded when the visual‑effects system starts.

Q: How are sky‑cloud settings (such as density or speed) exposed to the UI?  
A: Settings for the sky‑cloud effect are defined in `crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/settings.rs` as the `VfxSettings` struct. This struct holds fields like `cloud_density` and `cloud_speed`, and the UI controls for these parameters are wired up in `crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/ui.rs` where the Bevy UI widgets bind to instances of `VfxSettings`.

Q: Which Rust module contains the helper functions that create cloud entities?  
A: Cloud entity creation utilities are located in `crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/ui.rs`. This file contains functions such as `spawn_cloud_entity` that build Bevy entities with the appropriate mesh, texture, and material based on the settings from `VfxSettings`.

Q: Are there any shader files or WGSL resources referenced for cloud rendering?  
A: The cloud material uses a custom Bevy shader written in WGSL, which resides in the same `materials.rs` file as a string literal passed to `Shader::new(WgslShader::new(...))`. The shader code handles additive blending and alpha smoothing to achieve the sky‑cloud look.

Q: How does the system update cloud positions over time?  
A: Position updates for cloud entities are performed in the `VisualFXPlugin`’s `update` method, which iterates over all entities with the `AdditiveFxMaterial` component and modifies their `Transform` based on the `cloud_speed` setting from `VfxSettings`. This update runs each frame in the Bevy schedule’s `Update` stage.

Q: Where are the cloud texture assets loaded from?  
A: Cloud textures are stored in the project’s asset directory under `assets/clouds/` and are loaded via Bevy’s `AssetServer` in the `VisualFXPlugin` initialization code. The paths are hard‑coded as `"clouds/cloud01.png"` and similar filenames referenced in the material’s `diffuse_texture` field.

Q: Is there any runtime configuration to toggle the sky‑cloud effect on or off?  
A: Yes, the effect can be toggled via a boolean flag in `VfxSettings` called `enable_clouds`. When this flag is set to `false`, the `VisualFXPlugin` skips the creation of cloud entities and disables the `AdditiveFxMaterial` in the render pipeline, effectively turning off the sky‑cloud visualization. This flag is exposed in the UI under the “Clouds” section.