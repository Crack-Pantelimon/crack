use crate::plugins::cars_driving::driving_plugin::spawn_car::ActivePlayerVehicle;
use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy_egui::EguiContexts;

pub fn camera_follows_car(
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, (With<Camera3d>, Without<ActivePlayerVehicle>)>,
    car_query: Query<(&Transform, &LinearVelocity), (With<ActivePlayerVehicle>, Without<Camera3d>)>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut contexts: EguiContexts,
    mut local_orbit: Local<Option<(f32, f32)>>, // (yaw, pitch)
    capture_state: Res<crate::plugins::states::MouseCaptureState>,
) {
    let Ok((car_transform, linear_velocity)) = car_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }

    let egui_focused = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
    } else {
        false
    };

    // Center point is just above the top of the car
    let center = car_transform.translation + Vec3::Y * 1.5;

    // Get car yaw (Y-rotation) in world space
    let (car_yaw, _, _) = car_transform.rotation.to_euler(EulerRot::YXZ);

    // Default behind-the-car positions
    let default_yaw = car_yaw;
    let default_pitch = 15.0f32.to_radians();

    let (mut yaw, mut pitch) = local_orbit.unwrap_or((default_yaw, default_pitch));

    // Mouse drag or captured mouse updates yaw and pitch
    let drag_active = capture_state.is_captured || (!egui_focused && mouse_button.pressed(MouseButton::Left));
    if drag_active {
        let sensitivity = 0.003;
        for event in mouse_motion.read() {
            yaw -= event.delta.x * sensitivity;
            pitch += event.delta.y * sensitivity;
        }
        pitch = pitch.clamp(-80.0f32.to_radians(), 80.0f32.to_radians());
    } else {
        for _ in mouse_motion.read() {}
    }

    // Auto-centering when speed > 1.0 m/s
    let speed = linear_velocity.0.length();
    if speed > 1.0 {
        let yaw_diff = (default_yaw - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let pitch_diff = default_pitch - pitch;

        let reset_speed = 2.0;
        let decay = (-reset_speed * dt).exp();

        yaw = default_yaw - yaw_diff * decay;
        pitch = default_pitch - pitch_diff * decay;
    }

    *local_orbit = Some((yaw, pitch));

    // Position camera
    let r = 16.0;
    let offset = Vec3::new(
        r * yaw.sin() * pitch.cos(),
        r * pitch.sin(),
        r * yaw.cos() * pitch.cos(),
    );
    camera_transform.translation = center + offset;
    camera_transform.look_at(center, Vec3::Y);
}
