//! Third-person follow camera that trails behind the controlled character.
//!
//! The camera sits over the right shoulder. Its orientation is controlled manually by **left-mouse
//! drag** (yaw + pitch). Holding **right mouse** zooms in for aim (guns play the aim animation in
//! `drive_character_animation`; unarmed/melee get the zoom only). Combat fires on the mouse-*down*
//! edge, so a click jabs/shoots and the following drag rotates the camera.

use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};
use bevy_egui::EguiContexts;

use super::*;
use spawn::ControlledCharacter;

/// Marks the single primary game camera. Gameplay systems that need "the" camera (as opposed to
/// a secondary render camera for mirrors, picture-in-picture, or a minimap) should query this
/// instead of `With<Camera3d>`, which would match every 3D camera in the scene.
#[derive(Component)]
pub struct MainCamera;

/// Orbit state for the follow camera, driven by left-mouse drag.
#[derive(Resource)]
pub struct CameraRig {
/// yaw field.
    pub yaw: f32,
/// pitch field.
    pub pitch: f32,
    /// True while RMB is held (and not over egui); drives aim zoom and narrower shoulder offset.
    pub aiming: bool,
    /// Smoothed orbit distance; lerps between [`CAM_DISTANCE`] and [`CAM_AIM_DISTANCE`].
    pub current_distance: f32,
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
            aiming: false,
            current_distance: CAM_DISTANCE,
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
    mouse: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
    controlled: Res<ControlledCharacter>,
    mut rig: ResMut<CameraRig>,
    controller: Query<&GlobalTransform, With<CharacterController>>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
    spatial_query: avian3d::prelude::SpatialQuery,
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

    let over_ui = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
        .unwrap_or(false);
    rig.aiming = !over_ui && mouse.pressed(MouseButton::Right);

    let target_distance = if rig.aiming {
        CAM_AIM_DISTANCE
    } else {
        CAM_DISTANCE
    };
    if dt > 0.0 {
        let alpha = 1.0 - (-dt * CAM_ZOOM_SPEED).exp();
        rig.current_distance = rig.current_distance.lerp(target_distance, alpha);
    } else {
        rig.current_distance = target_distance;
    }

    let shoulder_x = if rig.aiming {
        CAM_AIM_SHOULDER_X
    } else {
        CAM_SHOULDER_X
    };
    let look_height = if rig.aiming {
        CAM_AIM_LOOK_HEIGHT
    } else {
        CAM_LOOK_HEIGHT
    };
    let shoulder_offset = Quat::from_rotation_y(rig.yaw) * Vec3::new(shoulder_x, 0.0, 0.0);

    // Camera position from the (slow) follow target + manual orbit; look at the (fast) look target.
    let anchor = pos_target + Vec3::Y * look_height + shoulder_offset;
    let distance = rig.current_distance;
    let offset =
        Quat::from_euler(EulerRot::YXZ, rig.yaw, rig.pitch, 0.0) * Vec3::new(0.0, 0.0, distance);
    if let Some(dir) = Dir3::new(offset).ok() {
        let filter = avian3d::prelude::SpatialQueryFilter::from_mask([
            crate::plugins::cars_driving::driving_plugin::GamePhysicsLayer::Map,
        ])
        .with_excluded_entities([controller_ent]);
        if let Some(hit) = spatial_query.cast_ray(anchor, dir, distance, true, &filter) {
            let dist = (hit.distance * 0.9).min(distance);
            cam.translation = anchor + offset.normalize() * dist;
        } else {
            cam.translation = anchor + offset;
        }
    } else {
        cam.translation = anchor + offset;
    }
    // Apply the same shoulder offset to the look target as to the anchor so the view is a
    // *parallel* over-the-shoulder shift. Without this the camera toes back in toward the
    // character centre and the offset cancels visually (character stays dead-centre).
    cam.look_at(look_pos + Vec3::Y * look_height + shoulder_offset, Vec3::Y);
}
