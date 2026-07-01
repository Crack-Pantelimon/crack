use crate::plugins::cars_driving::{
    driving_plugin::spawn_car::Car,
    driving_plugin::{CarDriveState, Drive},
};
use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;

pub fn keybinds_control_car(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut q_car: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
            &mut CarDriveState,
            &Car,
        ),
        With<Car>,
    >,
    mut commands: Commands,
    mut next_state: ResMut<NextState<crate::plugins::states::GameControlState>>,
    mut is_reverse_gear: Local<bool>,
) {
    // If escape or F is pressed, exit car
    if keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::KeyF) {
        next_state.set(crate::plugins::states::GameControlState::MapFreecam);
        if let Ok((car_entity, _, _, _, _, _car)) = q_car.single_mut() {
            commands.entity(car_entity).despawn();
        }
        return;
    }

    let Ok((car_entity, mut transform, mut lin_vel, mut ang_vel, mut drive_state, _car)) =
        q_car.single_mut()
    else {
        return;
    };

    let speed_kmh = lin_vel.0.length() * 3.6;

    // Gear switching logic when stationary/stopped (speed < 0.1 km/h)
    if speed_kmh < 1.0 {
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            *is_reverse_gear = true;
        } else if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            *is_reverse_gear = false;
        }
    }

    // Respawn / Reset car
    if keyboard.just_pressed(KeyCode::Space) {
        *is_reverse_gear = false;
        lin_vel.0 = Vec3::ZERO;
        ang_vel.0 = Vec3::ZERO;
        transform.rotation = Quat::IDENTITY;
        if let Some(spawn_pos) = drive_state.spawn_position {
            transform.translation = spawn_pos;
        }
    }

    let mut accelerate = 0.0;
    let mut brake = 0.0;
    let mut steer = 0.0;

    if *is_reverse_gear {
        // In reverse gear: S/Down accelerates backwards, W/Up brakes
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            accelerate = 1.0;
        }
        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            brake = 1.0;
        }
    } else {
        // In drive gear: W/Up accelerates forward, S/Down brakes
        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            accelerate = 1.0;
        }
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            brake = 1.0;
        }
    }

    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        steer -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        steer += 1.0;
    }

    // Save the reverse state to the CarDriveState component for physics and UI consumption
    drive_state.is_reverse = *is_reverse_gear;

    commands.entity(car_entity).trigger(|entity| Drive {
        entity,
        accelerate,
        brake,
        steer,
    });
}
