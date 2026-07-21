use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera;

/// map material edit plugin.
pub struct MapMaterialEditPlugin;

impl Plugin for MapMaterialEditPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapMaterialEditState>()
            .add_systems(EguiPrimaryContextPass, (map_material_edit_ui,));
        // Run the material/texture fix-ups in `PostUpdate` (after `Update` spawns and the
        // `SpawnScene` schedule that instantiates the tile GLB scenes, but still before the render
        // extract). In `Update` the execution order relative to tile spawning is not deterministic
        // — on the single-threaded web build this let a freshly-swapped tile render one frame with
        // its default (glossier/lighter) GLTF material before the matte values were applied,
        // causing a visible flash. `PostUpdate` guarantees the fix lands before the tile is drawn.
        app.add_systems(
            Last,
            (
                auto_apply_new_materials,
                auto_apply_nearest_sampling_to_images,
            ),
        );
    }
}

/// map material edit state.
#[derive(Resource, Debug, Clone)]
pub struct MapMaterialEditState {
/// show window field.
    pub show_window: bool,

    // Material settings
/// metallic field.
    pub metallic: f32,
/// roughness field.
    pub roughness: f32,
/// reflectance field.
    pub reflectance: f32,
/// ior field.
    pub ior: f32,

    // Lighting settings
/// dir light illuminance field.
    pub dir_light_illuminance: f32,
/// ambient light brightness field.
    pub ambient_light_brightness: f32,
}

impl Default for MapMaterialEditState {
    fn default() -> Self {
        Self {
            show_window: false,
            // Defaults to matte and non-reflective outdoor materials
            metallic: 1.0,
            roughness: 1.00,
            reflectance: 0.0,
            ior: 1.0,
            // Lighting defaults without HDR
            dir_light_illuminance: 3500.0,
            ambient_light_brightness: 1000.0,
        }
    }
}

fn map_material_edit_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<MapMaterialEditState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q_dir_lights: Query<&mut DirectionalLight>,
    mut q_ambient_lights: Query<&mut AmbientLight, With<MainCamera>>,
) {
    if !state.show_window {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut show_window = state.show_window;

    egui::Window::new("Map Material & Lighting Editor")
        .open(&mut show_window)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.heading("Material Settings");

            ui.add(egui::Slider::new(&mut state.metallic, 0.0..=1.0).text("Metallic"));

            ui.add(egui::Slider::new(&mut state.roughness, 0.0..=1.0).text("Roughness"));

            ui.add(egui::Slider::new(&mut state.reflectance, 0.0..=1.0).text("Reflectance"));
            ui.add(egui::Slider::new(&mut state.ior, 1.0..=2.0).text("IOR"));

            ui.separator();
            ui.heading("Lighting Settings");

            ui.add(
                egui::Slider::new(&mut state.dir_light_illuminance, 0.0..=10000.0)
                    .text("Sun Illuminance"),
            );

            ui.add(
                egui::Slider::new(&mut state.ambient_light_brightness, 0.0..=5000.0)
                    .text("Ambient Brightness"),
            );

            ui.allocate_space(egui::Vec2::new(1.0, 10.0));

            if ui.button("Update").clicked() {
                info!("Updating all loaded materials and lighting settings...");

                // 1. Update all standard materials
                for (_, material) in materials.iter_mut() {
                    material.metallic = state.metallic;
                    material.perceptual_roughness = state.roughness;
                    material.reflectance = state.reflectance;
                    material.ior = state.ior;
                }

                // 2. Update directional lights
                for mut light in &mut q_dir_lights {
                    light.illuminance = state.dir_light_illuminance;
                }

                // 3. Update ambient lights
                for mut light in &mut q_ambient_lights {
                    light.brightness = state.ambient_light_brightness;
                }
            }
        });

    state.show_window = show_window;
}

fn auto_apply_new_materials(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    state: Res<MapMaterialEditState>,
) {
    use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
    for event in events.read() {
        if let AssetEvent::Added { id } = event {
            let asset_id = *id;
            if let Some(mut material) = materials.get_mut(asset_id) {
                // Automatically apply our configured matte values to newly loaded materials
                material.metallic = state.metallic;
                material.perceptual_roughness = state.roughness;
                material.reflectance = state.reflectance;
                material.ior = state.ior;

                if let Some(ref texture_handle) = material.base_color_texture {
                    if let Some(mut image) = images.get_mut(texture_handle.id()) {
                        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                            mag_filter: ImageFilterMode::Nearest,
                            min_filter: ImageFilterMode::Nearest,
                            mipmap_filter: ImageFilterMode::Nearest,
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
}

fn auto_apply_nearest_sampling_to_images(
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
) {
    use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
    for event in events.read() {
        if let AssetEvent::Added { id } = event {
            if let Some(mut image) = images.get_mut(*id) {
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    mag_filter: ImageFilterMode::Nearest,
                    min_filter: ImageFilterMode::Nearest,
                    mipmap_filter: ImageFilterMode::Nearest,
                    ..Default::default()
                });
            }
        }
    }
}
