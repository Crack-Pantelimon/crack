use crate::plugins::map_plugin::MapTree;
use crate::plugins::states::GameControlState;
use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

pub struct CameraControlsPlugin;

#[derive(Resource)]
pub struct ActiveCameraAnimation {
    pub start_pos: Vec3,
    pub start_rot: Quat,
    pub target_pos: Vec3,
    pub target_rot: Quat,
    pub elapsed: f32,
    pub duration: f32,
}

impl Plugin for CameraControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (camera_movement_system, animate_camera_system)
                .run_if(in_state(GameControlState::MapFreecam)),
        );
    }
}

fn animate_camera_system(
    mut commands: Commands,
    time: Res<Time>,
    anim: Option<ResMut<ActiveCameraAnimation>>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
) {
    let Some(mut anim) = anim else {
        return;
    };
    let Some(mut transform) = camera_query.iter_mut().next() else {
        return;
    };

    anim.elapsed += time.delta_secs();
    let t = (anim.elapsed / anim.duration).clamp(0.0, 1.0);
    let t_smooth = t * t * (3.0 - 2.0 * t);

    transform.translation = anim.start_pos.lerp(anim.target_pos, t_smooth);
    transform.rotation = anim.start_rot.slerp(anim.target_rot, t_smooth);

    if t >= 1.0 {
        commands.remove_resource::<ActiveCameraAnimation>();
    }
}

fn query_ground_y(x: f32, z: f32, data_res: &MapTree, spatial_query: &SpatialQuery) -> f32 {
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
    mut last_pos: Local<Option<Vec3>>,
    anim: Option<Res<ActiveCameraAnimation>>,
) {
    if !data_res.parsed {
        return;
    }

    if anim.is_some() {
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

    // 2. Height Above Ground
    let ground_y = query_ground_y(
        transform.translation.x,
        transform.translation.z,
        &data_res,
        &spatial_query,
    );
    let height = (transform.translation.y - ground_y).max(0.1);

    let egui_wants_keyboard = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_keyboard_input()
    } else {
        false
    };

    // 3. Speed proportional to height
    let is_shift = !egui_wants_keyboard && (keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight));
    let speed_multiplier = if is_shift { 5.0 } else { 1.0 };
    let speed = (height * 1.0).clamp(5.0, 500.0) * speed_multiplier;

    // 4. Movement input (only if egui is not focused)
    if !egui_focused && !egui_wants_keyboard {
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
            transform.translation.y += speed * time.delta_secs();
        }
        let is_ctrl =
            keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight);
        if is_ctrl {
            transform.translation.y -= speed * time.delta_secs();
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
            transform.translation.y += scroll_direction * (height * 0.1).max(1.0);
        }
    } else {
        // Drain events to prevent build-up
        for _ in mouse_wheel.read() {}
    }

    // 6. Update position based on new ground_y (prevent going under terrain)
    let new_ground_y = query_ground_y(
        transform.translation.x,
        transform.translation.z,
        &data_res,
        &spatial_query,
    );
    if transform.translation.y < new_ground_y + 1.0 {
        transform.translation.y = new_ground_y + 1.0;
    }
    *last_pos = Some(transform.translation);
}
