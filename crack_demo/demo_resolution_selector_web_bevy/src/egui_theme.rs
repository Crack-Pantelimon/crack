//! Global egui theming: custom fonts + a simplistic, GTA-style dark visual style
//! with warm/amber accents instead of the default blue.
//!
//! Fonts live in `public/fonts/` (so the web build serves them too) and are embedded
//! at compile time via `include_bytes!` so native builds work without any file IO.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use std::sync::Arc;

/// Named egui font family used for large HUD readouts (speed, gear, ...).
pub const HUD_FAMILY: &str = "hud";

/// Amber accent used across the UI instead of the stock egui blue.
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(255, 149, 0);
/// Slightly softer amber for secondary highlights.
pub const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(200, 120, 20);

/// egui theme plugin.
pub struct EguiThemePlugin;

impl Plugin for EguiThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(EguiPrimaryContextPass, setup_egui_theme);
    }
}

/// Runs once (guarded by a `Local`) as soon as the primary egui context exists.
fn setup_egui_theme(mut contexts: EguiContexts, mut done: Local<bool>) {
    if *done {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    install_fonts(ctx);
    install_style(ctx);

    *done = true;
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "rajdhani".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../public/fonts/Rajdhani-SemiBold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "rajdhani_bold".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../public/fonts/Rajdhani-Bold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "oswald".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../public/fonts/Oswald-SemiBold.ttf"
        ))),
    );

    // Proportional (default) text uses Rajdhani, falling back to egui's builtin fonts
    // for glyphs Rajdhani lacks.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "rajdhani".to_owned());

    // Oswald as a secondary fallback (nice condensed caps for headings).
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(1, "oswald".to_owned());

    // Dedicated bold, wide family for the big HUD numbers.
    fonts.families.insert(
        egui::FontFamily::Name(HUD_FAMILY.into()),
        vec!["rajdhani_bold".to_owned(), "oswald".to_owned()],
    );

    ctx.set_fonts(fonts);
}

fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let mut v = egui::Visuals::dark();

    let panel = egui::Color32::from_rgb(17, 17, 19);
    let window = egui::Color32::from_rgb(22, 22, 26);
    let text = egui::Color32::from_rgb(228, 228, 230);

    v.panel_fill = panel;
    v.window_fill = window;
    v.extreme_bg_color = egui::Color32::from_rgb(10, 10, 12);
    v.faint_bg_color = egui::Color32::from_rgb(30, 30, 34);
    v.override_text_color = Some(text);

    // Warm accent for links / selection instead of blue.
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    // Subtle amber window borders, moderate rounding for a clean HUD look.
    v.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 55, 45));
    v.window_corner_radius = egui::CornerRadius::same(8);
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: egui::Color32::from_black_alpha(160),
    };

    // Flat, dark widgets with amber active highlight.
    let stroke_col = egui::Color32::from_rgb(70, 70, 76);
    v.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(24, 24, 28);
    v.widgets.noninteractive.weak_bg_fill = egui::Color32::from_rgb(24, 24, 28);
    v.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 50));
    v.widgets.inactive.bg_fill = egui::Color32::from_rgb(38, 38, 43);
    v.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(38, 38, 43);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, stroke_col);
    v.widgets.hovered.bg_fill = egui::Color32::from_rgb(52, 48, 40);
    v.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(52, 48, 40);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);

    let radius = egui::CornerRadius::same(4);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
    }

    style.visuals = v;
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    ctx.set_global_style(style);
}

/// Draws a red circular notification badge with a white count, anchored to the
/// top-right corner of `anchor`. Used on the "Online" menu button and the
/// "Global Chat" entry when unread messages have arrived.
pub fn draw_notification_badge(ui: &egui::Ui, anchor: egui::Rect, count: u32) {
    if count == 0 {
        return;
    }
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };

    let painter = ui.painter();
    let font = egui::FontId::proportional(11.0);
    // Size the pill to the text so multi-digit counts still fit.
    let galley = painter.layout_no_wrap(label.clone(), font.clone(), egui::Color32::WHITE);
    let r = 8.0_f32.max(galley.size().x / 2.0 + 4.0);
    let center = egui::pos2(anchor.right() - 2.0, anchor.top() + 2.0);

    if galley.size().x <= 8.0 {
        painter.circle_filled(center, r, egui::Color32::from_rgb(220, 40, 40));
    } else {
        // Rounded pill for wider labels.
        let rect = egui::Rect::from_center_size(center, egui::vec2(r * 2.0, 16.0));
        painter.rect_filled(
            rect,
            egui::CornerRadius::same(8),
            egui::Color32::from_rgb(220, 40, 40),
        );
    }
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        label,
        font,
        egui::Color32::WHITE,
    );
}
