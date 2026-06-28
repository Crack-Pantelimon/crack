use crate::plugins::map_plugin::MapTree;
use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy_egui::EguiContexts;
use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};

pub struct CameraControlsPlugin;

impl Plugin for CameraControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, camera_movement_system);
    }
}

fn camera_movement_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    data_res: Res<MapTree>,
    spatial_query: SpatialQuery,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
    mut contexts: EguiContexts,
) {
    if !data_res.parsed {
        return;
    }

    let Some(mut transform) = camera_query.iter_mut().next() else {
        return;
    };

    // Check if Egui wants input (skip rotation/keyboard if user interacts with UI)
    let egui_focused = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
    } else {
        false
    };

    // 1. Mouse Drag Rotation
    if !egui_focused && mouse_button.pressed(MouseButton::Left) {
        let (mut yaw, mut pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        let sensitivity = 0.003;
        for event in mouse_motion.read() {
            yaw -= event.delta.x * sensitivity;
            pitch -= event.delta.y * sensitivity;
        }
        pitch = pitch.clamp(-89.9f32.to_radians(), 89.9f32.to_radians());
        transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
    } else {
        // Drain events to prevent build-up
        for _ in mouse_motion.read() {}
    }

    // 2. Height Above Ground calculation
    let ray_dir = Dir3::NEG_Y;
    let height = if let Some(hit) = spatial_query.cast_ray(
        transform.translation,
        ray_dir,
        10000.0,
        true,
        &SpatialQueryFilter::default(),
    ) {
        hit.distance
    } else {
        // Fallback: height above bbox min or a sensible default
        (transform.translation.y - data_res.bbox.min.y).max(1.0)
    };

    // Speed proportional to height
    let speed = (height * 1.0).clamp(5.0, 500.0);

    // 3. Movement input (only if egui is not focused)
    if !egui_focused {
        // Forward/Backward (no vertical component)
        let mut forward = *transform.forward();
        forward.y = 0.0;
        let forward = forward.normalize_or_zero();

        // Left/Right (no vertical component)
        let mut right = *transform.right();
        right.y = 0.0;
        let right = right.normalize_or_zero();

        if keyboard.pressed(KeyCode::KeyW) {
            transform.translation += forward * speed * time.delta_secs();
        }
        if keyboard.pressed(KeyCode::KeyS) {
            transform.translation -= forward * speed * time.delta_secs();
        }
        if keyboard.pressed(KeyCode::KeyA) {
            transform.translation -= right * speed * time.delta_secs();
        }
        if keyboard.pressed(KeyCode::KeyD) {
            transform.translation += right * speed * time.delta_secs();
        }

        // Up/Down keyboard movement
        let mut vertical = 0.0;
        if keyboard.pressed(KeyCode::Space) || keyboard.pressed(KeyCode::KeyE) {
            vertical += 1.0;
        }
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::KeyQ) {
            vertical -= 1.0;
        }
        transform.translation.y += vertical * speed * time.delta_secs();
    }

    // 4. Mouse wheel vertical movement (scrolling always works if not on egui)
    if !egui_focused {
        let mut scroll_y = 0.0;
        for event in mouse_wheel.read() {
            scroll_y += event.y;
        }
        if scroll_y != 0.0 {
            transform.translation.y += scroll_y * speed * 0.05;
        }
    } else {
        // Drain events to prevent build-up
        for _ in mouse_wheel.read() {}
    }
}
