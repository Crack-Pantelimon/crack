pub mod config;
pub mod state;
pub mod trigger;
pub mod ui;
pub mod db;

use bevy::prelude::*;

pub struct MissionPlugin;

impl Plugin for MissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<state::MissionState>()
           .init_resource::<config::MissionList>()
           .add_systems(Startup, (config::load_missions_config, db::load_mission_progress).chain())
           .add_systems(Update, (
               trigger::check_mission_triggers,
               ui::draw_mission_triggers,
               db::save_mission_progress_system,
           ))
           .add_systems(bevy_egui::EguiPrimaryContextPass, ui::render_mission_hud);
    }
}
