use bevy::prelude::*;
use avian3d::prelude::{
    LinearVelocity, AngularVelocity
};
use crate::plugins::cars_driving::{
    driving_plugin::spawn_car::Car,
    driving_plugin::{
        CarDriveState, Drive, Wheel,
    },
};

pub fn keybinds_control_car(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut q_car: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
            &CarDriveState,
            &Car,
        ),
        With<Car>,
    >,
    mut q_wheels: Query<(&mut Transform, &mut LinearVelocity, &mut AngularVelocity, &Wheel), (Without<Car>,)>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<crate::plugins::states::GameControlState>>,
) {
    // If escape or F is pressed, exit car
    if keyboard.just_pressed(KeyCode::Escape) || keyboard.just_pressed(KeyCode::KeyF) {
        next_state.set(crate::plugins::states::GameControlState::MapFreecam);
        if let Ok((car_entity, _, _, _, _, car)) = q_car.single() {
            for &child_entity in &car.physics_children {
                commands.entity(child_entity).despawn();
            }
            commands.entity(car_entity).despawn();
        }
        return;
    }

    let Ok((car_entity, mut transform, mut lin_vel, mut ang_vel, drive_state, _car)) = q_car.single_mut() else {
        return;
    };

    // Respawn / Reset car
    if keyboard.just_pressed(KeyCode::Space) {
        lin_vel.0 = Vec3::ZERO;
        ang_vel.0 = Vec3::ZERO;
        transform.rotation = Quat::IDENTITY;
        if let Some(spawn_pos) = drive_state.spawn_position {
            transform.translation = spawn_pos;
            
            let car_half_width = drive_state.car_half_width;
            let car_half_length = drive_state.car_half_length;
            let car_half_height = drive_state.car_half_height;

            let wheel_rot = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);

            // Reset all wheels
            for (mut w_transform, mut w_lin_vel, mut w_ang_vel, wheel) in q_wheels.iter_mut() {
                let x_offset = if wheel.is_left { -car_half_width } else { car_half_width + if wheel.is_front { 0.1 } else { 0.0 } };
                let y_offset = -car_half_height + drive_state.wheel_y_offset;
                let z_offset = if wheel.is_front { -car_half_length } else { car_half_length };
                let offset = Vec3::new(x_offset, y_offset, z_offset);
                
                w_transform.translation = spawn_pos + offset;
                w_transform.rotation = wheel_rot;
                w_lin_vel.0 = Vec3::ZERO;
                w_ang_vel.0 = Vec3::ZERO;
            }
        }
    }

    let mut accelerate = 0.0;
    let mut brake = 0.0;
    let mut steer = 0.0;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        accelerate = 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        brake = 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        steer -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        steer += 1.0;
    }

    commands.entity(car_entity).trigger(|entity| Drive {
        entity,
        accelerate,
        brake,
        steer,
    });
}