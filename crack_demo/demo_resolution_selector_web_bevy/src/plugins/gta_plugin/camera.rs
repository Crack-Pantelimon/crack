use bevy::prelude::*;
use crate::plugins::gta_plugin::car::Car;
use crate::plugins::gta_plugin::GtaSpawnState;

#[derive(Resource)]
pub struct GtaCameraState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for GtaCameraState {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 15.0f32.to_radians(),
            distance: 12.0,
        }
    }
}

pub fn camera_follow_system(
    keyboard: Res<ButtonInput<KeyCode>>,
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

    // Determine target point: spawn_point if timer is active, otherwise the car
    let target_point = if spawn_state.timer.is_some() {
        spawn_state.spawn_point.map(|p| p + Vec3::new(0.0, 1.2, 0.0))
    } else if let Ok(car_transform) = car_query.single() {
        Some(car_transform.translation + Vec3::new(0.0, 1.2, 0.0))
    } else {
        None
    };

    let Some(target_point) = target_point else {
        return;
    };

    // Auto-align camera behind the car if moving and arrow keys are not pressed
    if !arrow_pressed && spawn_state.timer.is_none() {
        if let Ok(car_transform) = car_query.single() {
            let (_, car_yaw, _) = car_transform.rotation.to_euler(EulerRot::YXZ);
            let target_yaw = car_yaw + std::f32::consts::PI;
            
            let diff = (target_yaw - camera_state.yaw + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI) - std::f32::consts::PI;
            
            let follow_speed = 1.5;
            camera_state.yaw += diff * follow_speed * delta;
        }
    }

    // Position camera relative to car/spawn point
    let offset = Vec3::new(
        camera_state.distance * camera_state.pitch.cos() * camera_state.yaw.sin(),
        camera_state.distance * camera_state.pitch.sin(),
        camera_state.distance * camera_state.pitch.cos() * camera_state.yaw.cos(),
    );

    let desired_pos = target_point + offset;

    // Smooth movement (frame-rate independent lerp)
    let lerp_factor = 1.0 - (-8.0 * delta).exp();
    camera_transform.translation = camera_transform.translation.lerp(desired_pos, lerp_factor);
    
    // Look at target point
    let target_rot = Transform::from_translation(camera_transform.translation)
        .looking_at(target_point, Vec3::Y)
        .rotation;
    camera_transform.rotation = camera_transform.rotation.slerp(target_rot, lerp_factor);
}
