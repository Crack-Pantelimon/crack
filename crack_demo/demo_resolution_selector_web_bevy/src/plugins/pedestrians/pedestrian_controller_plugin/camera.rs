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
    /// Low-pass-filtered character position the camera actually follows (attenuates map shake).
    pub follow_target: Option<Vec3>,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: CAM_PITCH,
            follow_target: None,
        }
    }
}

/// Left-mouse drag rotates the follow camera around the character.
pub fn orbit_camera_input(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut rig: ResMut<CameraRig>,
) {
    if !mouse_buttons.pressed(MouseButton::Left) {
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
    let target = match rig.follow_target {
        Some(prev) if prev.distance(real) < CAM_FOLLOW_SNAP_DIST && dt > 0.0 => {
            let alpha = 1.0 - (-dt / CAM_FOLLOW_SMOOTH_TIME).exp();
            prev.lerp(real, alpha)
        }
        _ => real,
    };
    rig.follow_target = Some(target);

    let look = target + Vec3::Y * CAM_LOOK_HEIGHT;
    // Orbit offset: yaw around Y, pitch tilts up/down; camera sits CAM_DISTANCE behind (+Z of rig).
    let offset =
        Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0) * Vec3::new(0.0, 0.0, CAM_DISTANCE);
    cam.translation = look + offset;
    cam.look_at(look, Vec3::Y);
}
