use crate::plugins::cars_driving::driving_plugin::spawn_car::ActivePlayerVehicle;
use crate::plugins::pedestrians::pedestrian_controller_plugin::MainCamera;
use bevy::prelude::*;
use bevy_egui::EguiContexts;

#[derive(Resource, Default)]
pub struct DrivingAim {
    pub aiming: bool,
}

/// Shared drive-by aim signal: RMB held while the mouse is captured (mirrors on-foot `CameraRig.aiming`).
pub fn update_driving_aim(
    mouse: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
    capture_state: Res<crate::plugins::states::MouseCaptureState>,
    mut aim: ResMut<DrivingAim>,
) {
    let egui_focused = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui()
    } else {
        false
    };
    aim.aiming = capture_state.is_captured && !egui_focused && mouse.pressed(MouseButton::Right);
}

pub fn camera_follows_car(
    time: Res<Time>,
    mut camera_query: Query<&mut Transform, (With<MainCamera>, Without<ActivePlayerVehicle>)>,
    car_query: Query<(Entity, &Transform), (With<ActivePlayerVehicle>, Without<MainCamera>)>,
    mut mouse_motion: MessageReader<bevy::input::mouse::MouseMotion>,
    mut local_orbit: Local<Option<(f32, f32)>>, // (yaw, pitch)
    aim: Res<DrivingAim>,
    spatial_query: avian3d::prelude::SpatialQuery,
) {
    let Ok((car_entity, car_transform)) = car_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let dt = time.delta_secs().min(0.1);
    if dt <= 0.0 {
        return;
    }

    // Center point sits well above the car (GTA-style raised chase cam).
    let center = car_transform.translation + Vec3::Y * 2.6;

    // Get car yaw (Y-rotation) in world space
    let (car_yaw, _, _) = car_transform.rotation.to_euler(EulerRot::YXZ);

    // Default behind-the-car positions
    let default_yaw = car_yaw;
    let default_pitch = 20.0f32.to_radians();

    let (mut yaw, mut pitch) = local_orbit.unwrap_or((default_yaw, default_pitch));

    // Free-look only while aiming (RMB held); drain deltas otherwise so release doesn't jerk.
    if aim.aiming {
        let sensitivity = 0.003;
        for event in mouse_motion.read() {
            yaw -= event.delta.x * sensitivity;
            pitch += event.delta.y * sensitivity;
        }
        pitch = pitch.clamp(-80.0f32.to_radians(), 80.0f32.to_radians());
    } else {
        for _ in mouse_motion.read() {}

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
    if let Some(dir) = Dir3::new(offset).ok() {
        let filter = avian3d::prelude::SpatialQueryFilter::from_mask([
            crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer::Map,
        ])
        .with_excluded_entities([car_entity]);
        if let Some(hit) = spatial_query.cast_ray(center, dir, r, true, &filter) {
            let dist = (hit.distance * 0.9).min(r);
            camera_transform.translation = center + offset.normalize() * dist;
        } else {
            camera_transform.translation = center + offset;
        }
    } else {
        camera_transform.translation = center + offset;
    }
    camera_transform.look_at(center, Vec3::Y);
}
