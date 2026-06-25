use crate::plugins::map_plugin::MapTree;
use bevy::prelude::*;

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
    mut camera_query: Query<&mut Transform, With<Camera>>,
) {
    let middle = (data_res.bbox.min + data_res.bbox.max) / 2.0;

    for mut transform in &mut camera_query {
        let current_pos = transform.translation;
        let to_camera = current_pos - middle;
        let distance = to_camera.length();

        let mut desired_pos = current_pos;

        // WASDQE movement - speed proportional to distance to middle point
        let move_speed = distance.max(0.1) * time.delta_secs() * 1.5;

        let forward = transform.forward();
        let right = transform.right();
        let up = transform.up();

        if keyboard.pressed(KeyCode::KeyW) {
            desired_pos += *forward * move_speed;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            desired_pos -= *forward * move_speed;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            desired_pos -= *right * move_speed;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            desired_pos += *right * move_speed;
        }
        if keyboard.pressed(KeyCode::KeyQ) {
            desired_pos += *up * move_speed;
        }
        if keyboard.pressed(KeyCode::KeyE) {
            desired_pos -= *up * move_speed;
        }

        // Arrow rotations around the middle point
        let mut new_to_camera = desired_pos - middle;
        let rot_speed = time.delta_secs() * 2.0;

        if keyboard.pressed(KeyCode::ArrowLeft) {
            new_to_camera = Quat::from_rotation_y(rot_speed) * new_to_camera;
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            new_to_camera = Quat::from_rotation_y(-rot_speed) * new_to_camera;
        }
        if keyboard.pressed(KeyCode::ArrowUp) {
            new_to_camera = Quat::from_axis_angle(*right, rot_speed) * new_to_camera;
        }
        if keyboard.pressed(KeyCode::ArrowDown) {
            new_to_camera = Quat::from_axis_angle(*right, -rot_speed) * new_to_camera;
        }

        desired_pos = middle + new_to_camera;

        // Desired rotation looking at middle
        let desired_transform =
            Transform::from_translation(desired_pos).looking_at(middle, Vec3::Y);

        // Apply smoothing: new_camera_transform = (old_camera_transform + desired_transform) / 2.0
        transform.translation = (transform.translation + desired_transform.translation) / 2.0;
        transform.rotation = transform.rotation.slerp(desired_transform.rotation, 0.5);
    }
}
