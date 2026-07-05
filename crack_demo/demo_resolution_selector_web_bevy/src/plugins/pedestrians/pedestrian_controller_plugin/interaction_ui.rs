//! Freecam right-click "spawn pedestrian / spawn car" choice popup.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use super::spawn::{SpawnChoicePopup, SpawnControlledPedestrianEvent};
use crate::plugins::cars_driving::{
    car_info::get_random_car_type, driving_plugin::spawn_car::SpawnCarRequestEvent,
};

/// On right-click in freecam, raycast to the map and open the choice popup at that point.
pub fn handle_freecam_right_click(
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform)>,
    spatial_query: SpatialQuery,
    mut contexts: EguiContexts,
    mut popup: ResMut<SpawnChoicePopup>,
) {
    if !mouse_button.just_pressed(MouseButton::Right) {
        return;
    }
    if let Ok(ctx) = contexts.ctx_mut() {
        if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
            return;
        }
    }
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };
    if let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        10000.0,
        true,
        &SpatialQueryFilter::default(),
    ) {
        popup.active = true;
        popup.world_pos = ray.origin + *ray.direction * hit.distance;
        popup.screen_pos = cursor_pos;
    }
}

/// Draws the choice popup and dispatches the chosen spawn.
pub fn spawn_choice_popup_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut popup: ResMut<SpawnChoicePopup>,
) {
    if !popup.active {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut close = false;
    egui::Area::new(egui::Id::new("spawn_choice_popup"))
        .fixed_pos(egui::pos2(popup.screen_pos.x, popup.screen_pos.y))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label("Spawn here:");
                if ui.button("🚶 Controllable pedestrian").clicked() {
                    commands.trigger(SpawnControlledPedestrianEvent {
                        position: popup.world_pos,
                        url: None,
                        scale: None,
                        is_exiting_car: false,
                        rotation: None,
                    });
                    close = true;
                }
                if ui.button("🚗 Car").clicked() {
                    commands.trigger(SpawnCarRequestEvent {
                        position: popup.world_pos,
                        car_type: get_random_car_type().to_string(),
                        rotation: None,
                    });
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if close {
        popup.active = false;
    }
}

use crate::plugins::cars_driving::driving_plugin::spawn_car::ActivePlayerVehicle;

#[derive(Component)]
pub struct EnteringCarTimer {
    pub car_entity: Entity,
    pub timer: Timer,
}

#[derive(Component)]
pub struct ExitingCarTimer(pub Timer);

pub fn detect_car_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    q_player: Query<(Entity, &GlobalTransform), (With<crate::plugins::pedestrians::pedestrian_controller_plugin::CharacterController>, Without<EnteringCarTimer>)>,
    q_cars: Query<(Entity, &GlobalTransform), With<crate::plugins::cars_driving::driving_plugin::spawn_car::Car>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::KeyF) {
        if let Some((ped_entity, ped_tf)) = q_player.iter().next() {
            let mut closest_car = None;
            let mut min_dist = 3.0;
            
            for (car_entity, car_tf) in q_cars.iter() {
                let dist = ped_tf.translation().distance(car_tf.translation());
                if dist < min_dist {
                    min_dist = dist;
                    closest_car = Some(car_entity);
                }
            }
            
            if let Some(car) = closest_car {
                // Instead of immediately entering, add a timer for the animation
                commands.entity(ped_entity).insert(EnteringCarTimer {
                    car_entity: car,
                    timer: Timer::from_seconds(1.2, TimerMode::Once),
                });
            }
        }
    }
}

pub fn tick_entering_car(
    mut commands: Commands,
    time: Res<Time>,
    mut q_player: Query<(Entity, &mut EnteringCarTimer, &mut Transform)>,
    q_cars: Query<&GlobalTransform, With<crate::plugins::cars_driving::driving_plugin::spawn_car::Car>>,
    mut next_state: ResMut<NextState<crate::plugins::states::GameControlState>>,
) {
    for (entity, mut entering, mut ped_transform) in q_player.iter_mut() {
        // Interpolate position to the car door
        if let Ok(car_gt) = q_cars.get(entering.car_entity) {
            let car_tf = car_gt.compute_transform();
            
            // Assume door is to the left (negative X in local space) and seat is in the middle
            let door_pos = car_tf.translation + car_tf.rotation * Vec3::new(-1.2, 0.0, 0.0);
            let seat_pos = car_tf.translation + car_tf.rotation * Vec3::new(-0.4, 0.2, 0.0);
            
            let progress = entering.timer.fraction();
            let target_pos = if progress < 0.5 {
                door_pos
            } else {
                seat_pos
            };
            
            ped_transform.translation = ped_transform.translation.lerp(target_pos, time.delta_secs() * 5.0);
            
            // Face the driver orientation (rotated 180 deg around Y relative to car)
            let target_rot = car_tf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
            ped_transform.rotation = ped_transform.rotation.slerp(target_rot, time.delta_secs() * 5.0);
        }

        entering.timer.tick(time.delta());
        if entering.timer.just_finished() {
            // Only now despawn the pedestrian and transfer control
            if let Ok(mut entity_cmds) = commands.get_entity(entity) {
                entity_cmds.despawn();
            }
            if let Ok(mut car_cmds) = commands.get_entity(entering.car_entity) {
                car_cmds.insert(ActivePlayerVehicle);
            }
            next_state.set(crate::plugins::states::GameControlState::DrivingCar);
        }
    }
}

pub fn tick_exiting_car(
    mut commands: Commands,
    time: Res<Time>,
    mut q_player: Query<(Entity, &mut ExitingCarTimer)>,
    controlled: Res<crate::plugins::pedestrians::pedestrian_controller_plugin::spawn::ControlledCharacter>,
) {
    // Wait until the pedestrian model is fully loaded before ticking the timer,
    // otherwise the animation won't be seen.
    if controlled.awaiting {
        return;
    }
    for (entity, mut exiting) in q_player.iter_mut() {
        exiting.0.tick(time.delta());
        if exiting.0.just_finished() {
            if let Ok(mut entity_cmds) = commands.get_entity(entity) {
                entity_cmds.remove::<ExitingCarTimer>();
            }
        }
    }
}

pub fn handle_exit_car(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    q_active_car: Query<(Entity, &GlobalTransform), With<ActivePlayerVehicle>>,
    mut next_state: ResMut<NextState<crate::plugins::states::GameControlState>>,
) {
    if keys.just_pressed(KeyCode::KeyF) {
        if let Some((car_entity, car_tf)) = q_active_car.iter().next() {
            commands.entity(car_entity).remove::<ActivePlayerVehicle>();
            
            let (_, car_rot, car_trans) = car_tf.to_scale_rotation_translation();
            let right_dir = car_rot * Vec3::X;
            let exit_pos = car_trans + right_dir * 2.0 + Vec3::Y * 0.5;
            
            commands.trigger(SpawnControlledPedestrianEvent {
                position: exit_pos,
                url: None,
                scale: None,
                is_exiting_car: true,
                rotation: Some(car_rot * Quat::from_rotation_y(std::f32::consts::PI)),
            });
            
            next_state.set(crate::plugins::states::GameControlState::ControllingPedestrian);
        }
    }
}

