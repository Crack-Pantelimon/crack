use bevy::prelude::*;
use bevy::input::mouse::MouseWheel;
use crate::plugins::gta_plugin::car::Car;
use crate::plugins::gta_plugin::GtaSpawnState;

#[derive(Resource)]
pub struct GtaCameraState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub smoothed_target: Option<Vec3>,
}

impl Default for GtaCameraState {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 15.0f32.to_radians(),
            distance: 12.0,
            smoothed_target: None,
        }
    }
}

pub fn camera_follow_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    time: Res<Time>,
    mut camera_state: ResMut<GtaCameraState>,
    spawn_state: Res<GtaSpawnState>,
    car_query: Query<&Transform, With<Car>>,
    mut camera_query: Query<&mut Transform, (With<Camera>, Without<Car>)>,
) {
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let delta = time.delta_secs();

    // Rotate camera using Arrow keys
    let mut arrow_pressed = false;
    let rotation_speed = 2.0;

    if keyboard.pressed(KeyCode::ArrowLeft) {
        camera_state.yaw += rotation_speed * delta;
        arrow_pressed = true;
    }
    if keyboard.pressed(KeyCode::ArrowRight) {
        camera_state.yaw -= rotation_speed * delta;
        arrow_pressed = true;
    }
    if keyboard.pressed(KeyCode::ArrowUp) {
        camera_state.pitch = (camera_state.pitch + rotation_speed * delta).clamp(5.0f32.to_radians(), 75.0f32.to_radians());
        arrow_pressed = true;
    }
    if keyboard.pressed(KeyCode::ArrowDown) {
        camera_state.pitch = (camera_state.pitch - rotation_speed * delta).clamp(5.0f32.to_radians(), 75.0f32.to_radians());
        arrow_pressed = true;
    }

    // Zoom control using Equal/Minus keys
    let zoom_speed = 15.0;
    if keyboard.pressed(KeyCode::Equal) {
        camera_state.distance = (camera_state.distance - zoom_speed * delta).clamp(2.0, 60.0);
    }
    if keyboard.pressed(KeyCode::Minus) {
        camera_state.distance = (camera_state.distance + zoom_speed * delta).clamp(2.0, 60.0);
    }

    // Zoom control using mouse scroll wheel
    for event in mouse_wheel.read() {
        camera_state.distance = (camera_state.distance - event.y * 1.5).clamp(2.0, 60.0);
    }

    // Determine target point: spawn_point if timer is active, otherwise the car
    let target_point = if spawn_state.timer.is_some() {
        spawn_state.spawn_point.map(|p| p + Vec3::new(0.0, 1.2, 0.0))
    } else if let Ok(car_transform) = car_query.single() {
        Some(car_transform.translation + Vec3::new(0.0, 1.2, 0.0))
    } else {
        None
    };

    let Some(actual_target) = target_point else {
        return;
    };

    // Smoothly update the camera follow target (filters out trimesh bumps & snap jitter)
    let smoothed_target = match camera_state.smoothed_target {
        Some(curr) => {
            let next = curr.lerp(actual_target, 1.0 - (-15.0 * delta).exp());
            camera_state.smoothed_target = Some(next);
            next
        }
        None => {
            camera_state.smoothed_target = Some(actual_target);
            actual_target
        }
    };

    // Auto-align camera behind the car if moving and arrow keys are not pressed
    let is_moving_input = keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::KeyD);
    if !arrow_pressed && spawn_state.timer.is_none() && is_moving_input {
        if let Ok(car_transform) = car_query.single() {
            let forward = car_transform.forward();
            let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
            let car_yaw = if forward_xz.length_squared() > 0.001 {
                forward_xz.x.atan2(forward_xz.z)
            } else {
                0.0
            };
            let target_yaw = car_yaw + std::f32::consts::PI;
            
            let diff = (target_yaw - camera_state.yaw + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI) - std::f32::consts::PI;
            
            let follow_speed = 4.0;
            camera_state.yaw += diff * follow_speed * delta;
        }
    }

    // Position camera relative to smoothed target
    let offset = Vec3::new(
        camera_state.distance * camera_state.pitch.cos() * camera_state.yaw.sin(),
        camera_state.distance * camera_state.pitch.sin(),
        camera_state.distance * camera_state.pitch.cos() * camera_state.yaw.cos(),
    );

    let desired_pos = smoothed_target + offset;
    camera_transform.translation = desired_pos;
    
    // Look at smoothed target point
    let target_rot = Transform::from_translation(camera_transform.translation)
        .looking_at(smoothed_target, Vec3::Y)
        .rotation;
    camera_transform.rotation = target_rot;
}
