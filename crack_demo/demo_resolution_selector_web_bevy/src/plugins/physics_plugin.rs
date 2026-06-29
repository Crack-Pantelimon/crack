use avian3d::prelude::*;
use bevy::prelude::*;
use crate::ui_egui::UiState;

pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default())
            .add_plugins(PhysicsDebugPlugin::default())
            .insert_resource(Time::<Fixed>::from_hz(40.0))
            .add_systems(Update, sync_physics_debug_config);
    }
}

pub fn sync_physics_debug_config(
    ui_state: Res<UiState>,
    mut gizmo_store: ResMut<GizmoConfigStore>,
) {
    let (gizmo_config, _) = gizmo_store.config_mut::<PhysicsGizmos>();
    gizmo_config.enabled = ui_state.draw_physics_debug;
}
