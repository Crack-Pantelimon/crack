//! Third-person follow camera that trails behind the controlled character.
//!
//! The camera position follows the character, but its orientation is controlled manually by
//! **left-mouse drag** (yaw + pitch). Combat fires on the mouse-*down* edge, so a click jabs/shoots
//! and the following drag rotates the camera.

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};

use super::*;
use spawn::ControlledCharacter;

/// Orbit state for the follow camera, driven by left-mouse drag.
#[derive(Resource)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    /// Low-pass-filtered character position the camera *position* follows (attenuates map shake).
    pub follow_target: Option<Vec3>,
    /// Low-pass-filtered character position the camera *looks at*. Smoothed 2x faster than
    /// `follow_target` so the character stays framed even while the position glides smoothly.
    pub look_target: Option<Vec3>,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: CAM_PITCH,
            follow_target: None,
            look_target: None,
        }
    }
}

/// Left-mouse drag or captured mouse motion rotates the follow camera around the character.
pub fn orbit_camera_input(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut rig: ResMut<CameraRig>,
    capture_state: Res<crate::plugins::states::MouseCaptureState>,
) {
    let active = capture_state.is_captured || mouse_buttons.pressed(MouseButton::Left);
    if !active {
        return;
    }
    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }
    rig.yaw -= delta.x * CAM_ORBIT_SENSITIVITY;
    rig.pitch = (rig.pitch - delta.y * CAM_ORBIT_SENSITIVITY).clamp(CAM_PITCH_MIN, CAM_PITCH_MAX);
}

pub fn follow_camera(
    time: Res<Time>,
    controlled: Res<ControlledCharacter>,
    mut rig: ResMut<CameraRig>,
    controller: Query<&GlobalTransform, With<CharacterController>>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    let Some(controller_ent) = controlled.controller else {
        return;
    };
    let Ok(controller_gt) = controller.get(controller_ent) else {
        return;
    };
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };

    // Low-pass the character position so the camera doesn't inherit the controller's jitter on the
    // rough map. This attenuation applies only to the character-driven follow target — the orbit
    // yaw/pitch (user input) below is applied instantly and never smoothed.
    let real = controller_gt.translation();
    let dt = time.delta_secs();

    // Position-follow target: slow smoothing to swallow the controller's map jitter.
    let pos_target = match rig.follow_target {
        Some(prev) if prev.distance(real) < CAM_FOLLOW_SNAP_DIST && dt > 0.0 => {
            let alpha = 1.0 - (-dt / CAM_FOLLOW_SMOOTH_TIME).exp();
            prev.lerp(real, alpha)
        }
        _ => real,
    };
    rig.follow_target = Some(pos_target);

    // Look-at target: smoothed 2x faster so the camera keeps the character framed while strafing.
    let look_smooth_time = CAM_FOLLOW_SMOOTH_TIME * 0.5;
    let look_pos = match rig.look_target {
        Some(prev) if prev.distance(real) < CAM_FOLLOW_SNAP_DIST && dt > 0.0 => {
            let alpha = 1.0 - (-dt / look_smooth_time).exp();
            prev.lerp(real, alpha)
        }
        _ => real,
    };
    rig.look_target = Some(look_pos);

    // Camera position from the (slow) follow target + manual orbit; look at the (fast) look target.
    let anchor = pos_target + Vec3::Y * CAM_LOOK_HEIGHT;
    let offset = Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0)
        * Vec3::new(0.0, 0.0, CAM_DISTANCE);
    cam.translation = anchor + offset;
    cam.look_at(look_pos + Vec3::Y * CAM_LOOK_HEIGHT, Vec3::Y);
}
