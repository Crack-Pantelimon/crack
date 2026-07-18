## Overview  

The exploration reveals the visual‑effects subsystem that drives the sky, sun disc, and associated shaders.  The core data container is **`VfxSettings`** in `settings.rs`, which already holds a wide range of toggles and numeric sliders (e.g., `fireball_lifetime`, `smoke_opacity`, `tracer_width`).  This struct is marked with `#[derive(Resource, Clone, Copy, Debug)]` and is loaded as a shared resource by the `VisualFXPlugin`, making it easy to expose additional parameters such as *sunlight temperature* without major refactorings.  The existing slider infrastructure (`egui::Slider`) can be leveraged to add a new temperature control that ranges from 1500 K to 6000 K, and the value can be fed into the shader uniforms that already handle the disc’s colour and intensity.

In `materials.rs` the rendering pipeline is defined through three key types:  

* `BillboardParams` – carries per‑instance parameters (colour, lifetime, radius, kind, etc.).  
* `AdditiveFxMaterial` – used for glowing, additive blends (e.g., fireballs).  
* `BlendFxMaterial` – used for transparent effects like smoke.  

Both material types read the `kind` field from the `FxKind` enum, which currently enumerates fireball, smoke, black‑smoke, muzzle‑flash, spark‑burst, and tracer.  Adding a “sunlight temperature” parameter would most likely affect the colour calculation inside the fragment shader (`billboard_fx.wgsl`), so the change can be confined to updating the uniform data passed from `BillboardParams` and adjusting the shader logic accordingly.

The plugin registration in `mod.rs` wires everything together: it loads the embedded WGSL shader, initializes the `VfxSettings` resource, registers observers for car‑explosion and gun‑effect events, and hooks the UI panel (`vfx_controls_window`) into the Egui pass.  Because the UI is already driven by an egui window, inserting a new temperature slider would automatically appear in the same collapsible section, keeping the user experience consistent.

**Key take‑aways for the requested modification**  

1. Extend `VfxSettings` with a `sun_temperature: f32` field (default 4000 K) and expose it via an `egui::Slider`.  
2. Propagate the value into `BillboardParams` when building the material, and modify the fragment shader to use the temperature for colour grading (e.g., map temperature → RGB).  
3. Update the UI section in `ui.rs` (or the yet‑to‑be‑read UI file) to include the new slider, mirroring the pattern used for other sliders.  
4. Optionally, tie the temperature to the in‑game time of day by mutating the setting in the schedule that updates day/night cycles.

These changes are localized to a handful of files already identified in the repository, requiring no architectural overhaul.

---  

### Specific file references
- `workspace/crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/settings.rs:5-25`
- `workspace/crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs:1-38`
- `workspace/crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/mod.rs:1-45`