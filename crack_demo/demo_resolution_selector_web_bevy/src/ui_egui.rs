use bevy::{diagnostic::FrameCount, prelude::*};

use bevy_egui::*;

pub struct UiEguiPlugin;

impl Plugin for UiEguiPlugin {
    fn build(&self, app: &mut App) {
        info!("loading: UiEguiPlugin...");
        web_set_loading_status(true, "Loading UiEguiPlugin...");
        app.add_plugins(EguiPlugin::default())
            .init_resource::<UiState>()
            .add_systems(EguiPrimaryContextPass, ui_example_system)
            .add_systems(Update, update_web_loading_status);
        info!("done loading: UiEguiPlugin");
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
struct UiState {
    resolution: i32,
    ui_scale: i32,
    smooth: bool,
    show_settings: bool,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            resolution: 75,
            ui_scale: 125,
            smooth: true,
            show_settings: false,
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
    mut contexts: EguiContexts,
    mut window: Single<&mut Window>,
    mut fit_again: Local<i32>,
    mut initialized: Local<bool>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        tracing::error!("no ctx in ui_example_system");
        return;
    };
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

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    // ui.add(egui::Hyperlink::from_label_and_url(
                    //     "powered by egui",
                    //     "https://github.com/emilk/egui/",
                    // ));
                });
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

    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        // The top panel is often a good place for a menu bar:
        egui::menu::bar(ui, |ui| {
            egui::menu::menu_button(ui, "Options", |ui| {
                if ui.button("Graphics").clicked() {
                    ui_state.show_settings = !ui_state.show_settings;
                    ui.close();
                }
            });
            egui::menu::menu_button(ui, "Help", |ui| {
                if ui.button("Crash").clicked() {
                    std::process::exit(0);
                }
            });
        });
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
