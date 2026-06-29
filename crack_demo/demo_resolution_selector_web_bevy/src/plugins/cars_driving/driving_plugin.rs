use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

pub struct DrivingPlugin<S: States> {
    pub state: S
}

impl<S: States> Plugin for DrivingPlugin<S> {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (camera_follows_car, keybinds_control_car).run_if(in_state(self.state)));
                app.add_systems(EguiPrimaryContextPass, (driving_ui, speedometer_ui).run_if(in_state(self.state)));
    }
}