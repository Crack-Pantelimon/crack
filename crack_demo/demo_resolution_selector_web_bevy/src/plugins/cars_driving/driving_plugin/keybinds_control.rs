use bevy::prelude::*;
use avian3d::prelude::{
    LinearVelocity, AngularVelocity
};
use crate::plugins::cars_driving::{driving_plugin::spawn_car::Car, driving_plugin::{CarDriveState, Drive, Strut, Wheel}, };

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
    mut q_struts: Query<(&mut Transform, &mut LinearVelocity, &mut AngularVelocity, &Strut), (Without<Car>, Without<Wheel>)>,
    mut q_wheels: Query<(&mut Transform, &mut LinearVelocity, &mut AngularVelocity, &Wheel), (Without<Car>, Without<Strut>)>,
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
            
            // Define geometry variables
            let half_width = 0.9f32;
            let half_length = 1.8f32;
            let suspension_len = 0.3f32;

            // Reset all struts
            for (mut s_transform, mut s_lin_vel, mut s_ang_vel, strut) in q_struts.iter_mut() {
                let x = if strut.is_left { -half_width } else { half_width };
                let y = 0.3;
                let z = if strut.is_front { -half_length } else { half_length };
                let offset = Vec3::new(x, y, z);
                
                s_transform.translation = spawn_pos + offset - Vec3::Y * suspension_len;
                s_transform.rotation = Quat::IDENTITY;
                s_lin_vel.0 = Vec3::ZERO;
                s_ang_vel.0 = Vec3::ZERO;
            }


            // Reset all wheels
            for (mut w_transform, mut w_lin_vel, mut w_ang_vel, wheel) in q_wheels.iter_mut() {
                let x = if wheel.is_left { -half_width } else { half_width };
                let y = 0.3;
                let z = if wheel.is_front { -half_length } else { half_length };
                let offset = Vec3::new(x, y, z);
                
                w_transform.translation = spawn_pos + offset - Vec3::Y * suspension_len;
                w_transform.rotation = Quat::IDENTITY;
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