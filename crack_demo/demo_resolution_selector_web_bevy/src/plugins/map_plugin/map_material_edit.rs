use bevy::core_pipeline::Skybox;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

pub struct MapMaterialEditPlugin;

impl Plugin for MapMaterialEditPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapMaterialEditState>()
            .add_systems(EguiPrimaryContextPass, (map_material_edit_ui,));
        app.add_systems(Update, auto_apply_new_materials);
    }
}

#[derive(Resource, Debug, Clone)]
pub struct MapMaterialEditState {
    pub show_window: bool,

    // Material settings
    pub metallic: f32,
    pub roughness: f32,
    pub reflectance: f32,

    // Lighting settings
    pub dir_light_illuminance: f32,
    pub ambient_light_brightness: f32,
    pub skybox_brightness: f32,
}

impl Default for MapMaterialEditState {
    fn default() -> Self {
        Self {
            show_window: false,
            // Defaults to matte and non-reflective outdoor materials
            metallic: 0.0,
            roughness: 0.99,
            reflectance: 0.0,
            // Lighting defaults without HDR
            dir_light_illuminance: 2000.0,
            ambient_light_brightness: 1000.0,
            skybox_brightness: 1000.0,
        }
    }
}

fn map_material_edit_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<MapMaterialEditState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q_dir_lights: Query<&mut DirectionalLight>,
    mut q_ambient_lights: Query<&mut AmbientLight, With<Camera3d>>,
    mut q_skybox: Query<&mut Skybox, With<Camera3d>>,
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

            ui.add(
                egui::Slider::new(&mut state.skybox_brightness, 0.0..=5000.0)
                    .text("Skybox Brightness"),
            );

            ui.allocate_space(egui::Vec2::new(1.0, 10.0));

            if ui.button("Update").clicked() {
                info!("Updating all loaded materials and lighting settings...");

                // 1. Update all standard materials
                for (_, material) in materials.iter_mut() {
                    material.metallic = state.metallic;
                    material.perceptual_roughness = state.roughness;
                    material.reflectance = state.reflectance;
                }

                // 2. Update directional lights
                for mut light in &mut q_dir_lights {
                    light.illuminance = state.dir_light_illuminance;
                }

                // 3. Update ambient lights
                for mut light in &mut q_ambient_lights {
                    light.brightness = state.ambient_light_brightness;
                }

                // 4. Update skybox brightness
                for mut skybox in &mut q_skybox {
                    skybox.brightness = state.skybox_brightness;
                }
            }
        });

    state.show_window = show_window;
}

fn auto_apply_new_materials(
    mut events: MessageReader<AssetEvent<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    state: Res<MapMaterialEditState>,
) {
    for event in events.read() {
        if let AssetEvent::Added { id } = event {
            let asset_id = *id;
            if let Some(mut material) = materials.get_mut(asset_id) {
                // Automatically apply our configured matte values to newly loaded materials
                material.metallic = state.metallic;
                material.perceptual_roughness = state.roughness;
                material.reflectance = state.reflectance;
            }
        }
    }
}
