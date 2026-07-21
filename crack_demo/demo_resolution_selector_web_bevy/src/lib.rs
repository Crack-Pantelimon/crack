//! Bevy demo crate: resolution selector, game plugins, and web builds.

/// Shared app bootstrap: window, assets, and headless test helpers.
pub mod basic_app;
/// Build-time URLs and feature-gated constants.
pub mod config;
/// Shared egui styling for in-game overlays.
pub mod egui_theme;
/// Default plugin bundle wiring the full Pantelimon game scene.
pub mod main_game_plugin;
/// Gameplay feature plugins: traffic, pedestrians, map, audio, and FX.
pub mod plugins;
/// egui UI shell and notification helpers.
pub mod ui_egui;
/// Texture loading and debug-scene setup utilities.
pub mod utils;
