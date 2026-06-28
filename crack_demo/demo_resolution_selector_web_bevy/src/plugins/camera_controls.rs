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

fn query_ground_y(
    x: f32,
    z: f32,
    data_res: &MapTree,
    spatial_query: &SpatialQuery,
) -> f32 {
    let start_y = data_res.bbox.max.y + 1.0;
    let ray_origin = Vec3::new(x, start_y, z);
    let ray_dir = Dir3::NEG_Y;
    let max_dist = (data_res.bbox.max.y + 1.0 - data_res.bbox.min.y) + 10.0;

    if let Some(hit) = spatial_query.cast_ray(
        ray_origin,
        ray_dir,
        max_dist,
        true,
        &SpatialQueryFilter::default(),
    ) {
        start_y - hit.distance
    } else {
        data_res.bbox.min.y
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
    mut last_offset: Local<Option<f32>>,
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
    let rotate_active = !egui_focused && mouse_button.pressed(MouseButton::Left);
    if rotate_active {
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

    // 2. Height Offset Tracking
    let ground_y = query_ground_y(transform.translation.x, transform.translation.z, &data_res, &spatial_query);
    let mut offset = match *last_offset {
        Some(val) => val,
        None => (transform.translation.y - ground_y).clamp(0.1, 500.0),
    };

    // 3. Speed proportional to offset
    // Shift makes it faster if held down
    let is_shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
    let speed_multiplier = if is_shift { 5.0 } else { 1.0 };
    let speed = (offset * 1.0).clamp(5.0, 500.0) * speed_multiplier;

    // 4. Movement input (only if egui is not focused)
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

        // Up/Down keyboard movement (Space for Up, Ctrl for Down)
        if keyboard.pressed(KeyCode::Space) {
            offset += speed * time.delta_secs();
        }
        let is_ctrl = keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
        if is_ctrl {
            offset -= speed * time.delta_secs();
        }
    }

    // 5. Mouse wheel vertical movement (scrolling always works if not on egui)
    if !egui_focused {
        let mut scroll_direction = 0.0;
        for event in mouse_wheel.read() {
            if event.y > 0.0 {
                scroll_direction += 1.0;
            } else if event.y < 0.0 {
                scroll_direction -= 1.0;
            }
        }
        if scroll_direction != 0.0 {
            // Scroll by a preset amount scaled by current offset
            offset += scroll_direction * (offset * 0.1).max(1.0);
        }
    } else {
        // Drain events to prevent build-up
        for _ in mouse_wheel.read() {}
    }

    // Clamp the offset to 0.1 - 500m
    offset = offset.clamp(0.1, 500.0);
    *last_offset = Some(offset);

    // 6. Update position based on new ground_y and offset
    let new_ground_y = query_ground_y(transform.translation.x, transform.translation.z, &data_res, &spatial_query);
    transform.translation.y = new_ground_y + offset;
}
