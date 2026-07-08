#![allow(deprecated)]
use bevy::audio::{GlobalVolume, Volume};
use bevy::{diagnostic::FrameCount, prelude::*};

use bevy_egui::*;

pub struct UiEguiPlugin;

impl Plugin for UiEguiPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: UiEguiPlugin...");
        web_set_loading_status(true, "Loading UiEguiPlugin...");
        app.add_plugins(EguiPlugin::default())
            .add_plugins(crate::egui_theme::EguiThemePlugin)
            .init_resource::<UiState>()
            .add_systems(EguiPrimaryContextPass, ui_example_system)
            .add_systems(Update, update_web_loading_status);
        info!("done loading: UiEguiPlugin");
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct UiState {
    pub resolution: i32,
    pub ui_scale: i32,
    pub smooth: bool,
    pub show_settings: bool,
    pub draw_map_bboxes: bool,
    pub draw_physics_debug: bool,
    pub draw_car_rays: bool,
    pub draw_rk4_gizmos: bool,
    pub draw_spark_origin_gizmos: bool,
    pub show_lod_configurator: bool,
    pub show_geojson_database: bool,
    pub show_traffic_debug: bool,
    pub show_pedestrian_ai: bool,
    pub show_vehicle_tuning: bool,
    pub show_multiplayer_debug: bool,
    pub show_sound_settings: bool,
    pub master_volume: f32,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            resolution: 75,
            ui_scale: 125,
            smooth: true,
            show_settings: false,
            draw_map_bboxes: false,
            draw_physics_debug: false,
            draw_car_rays: false,
            draw_rk4_gizmos: false,
            draw_spark_origin_gizmos: false,
            show_lod_configurator: false,
            show_geojson_database: false,
            show_traffic_debug: false,
            show_pedestrian_ai: false,
            show_vehicle_tuning: false,
            show_multiplayer_debug: false,
            show_sound_settings: false,
            master_volume: 0.6,
        }
    }
}
impl UiState {
    pub fn with_physics_debug() -> Self {
        Self {
            resolution: 75,
            ui_scale: 125,
            smooth: true,
            show_settings: false,
            draw_map_bboxes: false,
            draw_physics_debug: true,
            draw_car_rays: false,
            draw_rk4_gizmos: false,
            draw_spark_origin_gizmos: false,
            show_lod_configurator: false,
            show_geojson_database: false,
            show_traffic_debug: false,
            show_pedestrian_ai: false,
            show_vehicle_tuning: false,
            show_multiplayer_debug: false,
            show_sound_settings: false,
            master_volume: 0.6,
        }
    }
}

impl UiState {
    fn get_scale_factor_override(&self) -> Option<f32> {
        Some(self.ui_scale as f32 / 100.0 * self.resolution as f32 / 100.0 * 1.6)
    }
}

fn ui_example_system(
    mut ui_state: ResMut<UiState>,
    mut global_volume: ResMut<GlobalVolume>,
    mut contexts: EguiContexts,
    mut window: Single<&mut Window>,
    mut fit_again: Local<i32>,
    mut initialized: Local<bool>,
    time: Res<Time>,
    mut fps: Local<f32>,
    mut edit_state: Option<
        ResMut<crate::plugins::map_plugin::map_material_edit::MapMaterialEditState>,
    >,
    loading_status: Option<Res<crate::plugins::geojson::GameLoadingStatus>>,
    mut osm_overlay: Option<ResMut<crate::plugins::geojson::OsmOverlayState>>,
    mut global_chat: Option<ResMut<crate::plugins::network::global_chat_ui::GlobalChatUiState>>,
    chat_state: Option<Res<crate::plugins::network::ChatState>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        tracing::error!("no ctx in ui_example_system");
        return;
    };

    // --- FPS (EMA over ~30 frames) ---
    let dt = time.delta_secs();
    if dt > 0.0 {
        let instant_fps = 1.0 / dt;
        if *fps == 0.0 {
            *fps = instant_fps;
        } else {
            // exponential moving average, alpha ≈ 1/30
            *fps = *fps * (29.0 / 30.0) + instant_fps * (1.0 / 30.0);
        }
    }
    if !*initialized {
        *initialized = true;
        web_set_resolution(ui_state.resolution);
        window
            .resolution
            .set_scale_factor_override(ui_state.get_scale_factor_override());
        web_fit_canvas_to_parent(ui_state.smooth);
        *fit_again = 3;
    }
    if *fit_again > 0 {
        web_fit_canvas_to_parent(ui_state.smooth);
        *fit_again -= 1;
    }
    let old_ui_state = ui_state.clone();
    let phys_res = format!(
        "{}x{}",
        (window.resolution.physical_size().x),
        (window.resolution.physical_size().y)
    );
    let log_res = format!(
        "{}x{}",
        (window.resolution.size().x).round(),
        (window.resolution.size().y).round()
    );
    let original_screen_res = format!(
        "{}x{}",
        (window.resolution.physical_size().x as f32 / (ui_state.resolution as f32 / 100.0)).round(),
        (window.resolution.physical_size().y as f32 / (ui_state.resolution as f32 / 100.0)).round(),
    );
    let res_txt = format!(
        "Physical: {}\nLogical: {}\nScreen: {}",
        phys_res, log_res, original_screen_res
    );

    if ui_state.show_settings {
        egui::SidePanel::left("side_panel")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Graphics Settings");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            ui_state.show_settings = false;
                        }
                    });
                });
                ui.allocate_space(egui::Vec2::new(1.0, 10.0));

                ui.add(
                    egui::Slider::new(&mut ui_state.resolution, 25..=100)
                        .text("Resolution")
                        .suffix("%")
                        .step_by(5.0)
                        .clamping(egui::SliderClamping::Always),
                );
                if ui.button("Resolution +").clicked() {
                    ui_state.resolution += 5;
                }
                if ui.button("Resolution -").clicked() {
                    ui_state.resolution -= 5;
                }

                ui.allocate_space(egui::Vec2::new(1.0, 5.0));
                ui.checkbox(&mut ui_state.draw_map_bboxes, "Draw Map BBoxes");
                ui.checkbox(&mut ui_state.draw_physics_debug, "Draw Physics Debug");
                ui.checkbox(&mut ui_state.draw_car_rays, "Draw Car Rays & Contacts");
                ui.checkbox(&mut ui_state.draw_rk4_gizmos, "Draw RK4 Prediction Gizmos");
                ui.checkbox(
                    &mut ui_state.draw_spark_origin_gizmos,
                    "Draw Spark Contact Origin Gizmos",
                );

                ui.allocate_space(egui::Vec2::new(1.0, 10.0));

                ui.add(
                    egui::Slider::new(&mut ui_state.ui_scale, 75..=250)
                        .text("UI Scale")
                        .suffix("%")
                        .step_by(5.0)
                        .clamping(egui::SliderClamping::Always),
                );
                if ui.button("UI Scale +").clicked() {
                    ui_state.ui_scale += 5;
                }
                if ui.button("UI Scale -").clicked() {
                    ui_state.ui_scale -= 5;
                }
                ui.allocate_space(egui::Vec2::new(1.0, 10.0));
                ui.add(egui::Checkbox::new(&mut ui_state.smooth, "Smooth"));
                ui.allocate_space(egui::Vec2::new(1.0, 10.0));
                ui.add(egui::Label::new(res_txt));

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |_ui| {
                    // ui.add(egui::Hyperlink::from_label_and_url(
                    //     "powered by egui",
                    //     "https://github.com/emilk/egui/",
                    // ));
                });
            });
    }

    if ui_state.show_sound_settings {
        egui::SidePanel::left("sound_panel")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Sound Settings");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() {
                            ui_state.show_sound_settings = false;
                        }
                    });
                });
                ui.allocate_space(egui::Vec2::new(1.0, 10.0));

                let mut volume_pct = (ui_state.master_volume * 100.0).round() as i32;
                if ui
                    .add(
                        egui::Slider::new(&mut volume_pct, 0..=100)
                            .text("Volume")
                            .suffix("%")
                            .step_by(1.0)
                            .clamping(egui::SliderClamping::Always),
                    )
                    .changed()
                {
                    ui_state.master_volume = volume_pct as f32 / 100.0;
                }
            });
    }

    if ui_state.resolution != old_ui_state.resolution
        || ui_state.ui_scale != old_ui_state.ui_scale
        || ui_state.smooth != old_ui_state.smooth
    {
        // set the resolution in the object
        web_set_resolution(ui_state.resolution);
        window
            .resolution
            .set_scale_factor_override(ui_state.get_scale_factor_override());
        web_fit_canvas_to_parent(ui_state.smooth);
        *fit_again = 3;
    }

    if ui_state.master_volume != old_ui_state.master_volume {
        global_volume.volume = Volume::Linear(ui_state.master_volume);
    }

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        // The top panel is often a good place for a menu bar:
        egui::menu::bar(ui, |ui| {
            egui::menu::menu_button(ui, "Options", |ui| {
                if ui.button("Graphics").clicked() {
                    ui_state.show_settings = !ui_state.show_settings;
                    ui.close();
                }
                if ui.button("Sound").clicked() {
                    ui_state.show_sound_settings = !ui_state.show_sound_settings;
                    ui.close();
                }
            });
            let unread = chat_state.as_ref().map(|c| c.unread_count).unwrap_or(0);
            let online_resp = egui::menu::menu_button(ui, "Online", |ui| {
                if let Some(ref mut chat) = global_chat {
                    let chat_resp = ui.button("Global Chat");
                    // Badge on the sub-entry that produced the notification.
                    crate::egui_theme::draw_notification_badge(ui, chat_resp.rect, unread);
                    if chat_resp.clicked() {
                        chat.show_window = !chat.show_window;
                        ui.close();
                    }
                }
                if ui.button("Multiplayer Debug").clicked() {
                    ui_state.show_multiplayer_debug = !ui_state.show_multiplayer_debug;
                    ui.close();
                }
            });
            // Badge on the top-level "Online" menu button.
            crate::egui_theme::draw_notification_badge(ui, online_resp.response.rect, unread);
            egui::menu::menu_button(ui, "Debug", |ui| {
                if ui.button("Pedestrian AI").clicked() {
                    ui_state.show_pedestrian_ai = !ui_state.show_pedestrian_ai;
                    ui.close();
                }
                if ui.button("Vehicle Tuning").clicked() {
                    ui_state.show_vehicle_tuning = !ui_state.show_vehicle_tuning;
                    ui.close();
                }
                if ui.button("Lod Configurator & Tree Navigator").clicked() {
                    ui_state.show_lod_configurator = !ui_state.show_lod_configurator;
                    ui.close();
                }
                if ui.button("Traffic").clicked() {
                    ui_state.show_traffic_debug = !ui_state.show_traffic_debug;
                    ui.close();
                }
                let geojson_loaded = loading_status
                    .as_ref()
                    .map(|s| s.geojson_loaded)
                    .unwrap_or(false);
                if geojson_loaded {
                    if ui.button("GeoJson Database").clicked() {
                        ui_state.show_geojson_database = !ui_state.show_geojson_database;
                        ui.close();
                    }
                    if let Some(ref mut osm) = osm_overlay {
                        if ui.button("OSM Overlays").clicked() {
                            osm.show_window = !osm.show_window;
                            ui.close();
                        }
                    }
                } else {
                    ui.add_enabled(false, egui::Button::new("GeoJson Database (loading...)"));
                }
                if let Some(ref mut state) = edit_state {
                    if ui.button("Map Material & Lighting Editor").clicked() {
                        state.show_window = !state.show_window;
                        ui.close();
                    }
                }
            });
            egui::menu::menu_button(ui, "Help", |ui| {
                if ui.button("Crash").clicked() {
                    std::process::exit(0);
                }
            });
        });
    });

    let screen_rect = ctx.screen_rect();

    // --- FPS overlay (top-right corner) ---
    egui::Area::new(egui::Id::new("fps_overlay"))
        .fixed_pos(egui::pos2(screen_rect.max.x - 160.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("FPS: {:.0}", *fps))
                    .color(egui::Color32::from_rgb(0, 220, 80))
                    .size(18.0)
                    .strong(),
            );
        });

    // --- Text Overlay ---
    egui::Area::new(egui::Id::new("text_overlay"))
        .fixed_pos(egui::pos2(
            screen_rect.max.x - 160.0,
            screen_rect.max.y - 16.0,
        ))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Right Click = Spawn Car!"))
                    .color(egui::Color32::from_rgb(0, 220, 80))
                    .size(10.0)
                    .strong(),
            );
        });
}

fn web_set_resolution(_res: i32) {
    #[cfg(feature = "web")]
    {
        let document = web_sys::window().unwrap().document().unwrap();

        let canvas_parent_element = document.get_element_by_id("canvas-parent").unwrap();

        canvas_parent_element
            .set_attribute("style", &format!("--resolution: {};", _res))
            .unwrap();
    }
}

fn web_fit_canvas_to_parent(_smooth: bool) {
    #[cfg(feature = "web")]
    {
        let document = web_sys::window().unwrap().document().unwrap();

        use web_sys::HtmlCanvasElement;
        use web_sys::wasm_bindgen::JsCast;

        let canvas_element: HtmlCanvasElement = document
            .get_element_by_id("the-canvas")
            .unwrap()
            .unchecked_into();
        let style = canvas_element.style();
        style.set_property("width", "100%").unwrap();
        style.set_property("height", "100%").unwrap();
        style
            .set_property(
                "image-rendering",
                if _smooth { "smooth" } else { "pixelated" },
            )
            .unwrap();
    }
}

fn update_web_loading_status(time: Res<FrameCount>) {
    if time.0 <= 1 {
        web_set_loading_status(true, "Starting Graphics...");
    } else if time.0 == 5 {
        web_set_loading_status(true, "Graphics Started.");
    } else if time.0 == 10 {
        web_set_loading_status(false, "");
    }
}

pub fn web_set_loading_status(_show: bool, _message: &str) {
    #[cfg(feature = "web")]
    {
        info!(
            "web_set_loading_status(show: {}, message: {})",
            _show, _message
        );
        use web_sys::HtmlDivElement;
        use web_sys::wasm_bindgen::JsCast;

        let document = web_sys::window().unwrap().document().unwrap();
        let loading_screen: HtmlDivElement = document
            .get_element_by_id("loading-screen")
            .unwrap()
            .unchecked_into();
        let loading_screen_text = document.get_element_by_id("loading-screen-text").unwrap();
        let style = loading_screen.style();
        if _show {
            style.set_property("display", "flex").unwrap();
            style.set_property("visibility", "visible").unwrap();
            loading_screen_text.set_text_content(Some(_message));
        } else {
            style.set_property("display", "none").unwrap();
            style.set_property("visibility", "hidden").unwrap();
            loading_screen_text.set_text_content(Some(""));
        }

        // set focus to canvas every time this function is called
        use web_sys::HtmlCanvasElement;
        let canvas_element: HtmlCanvasElement = document
            .get_element_by_id("the-canvas")
            .unwrap()
            .unchecked_into();
        canvas_element.focus().unwrap();
    }
}
