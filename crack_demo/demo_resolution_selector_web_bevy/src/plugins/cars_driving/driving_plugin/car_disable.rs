//! Disabling cars that have taken too much damage in the gang wars.

use bevy::ecs::query::Has;
use bevy::prelude::*;

use super::spawn_car::{ActivePlayerVehicle, CAR_DISABLE_HP, Car, CarHealth, DisabledCar};
use crate::plugins::pedestrian_ai::faction::{Faction, Health};
use crate::plugins::pedestrians::pedestrian_controller_plugin::{
    DriverMesh, SpawnControlledPedestrianEvent, eject_driver_as_ai,
};

use crate::plugins::weapons::{EquippedWeapon, GunState};

/// When a car's [`CarHealth`] falls to/below [`CAR_DISABLE_HP`], mark it [`DisabledCar`] (which
/// stops it, blocks entry, and shows a green sphere) and eject its driver. An AI driver stands up
/// as a fresh AI ped; the player is handed back a controllable pedestrian with their carried HP.
pub fn disable_low_health_cars(
    mut commands: Commands,
    time: Res<Time>,
    q_cars: Query<
        (
            Entity,
            &GlobalTransform,
            &CarHealth,
            Has<ActivePlayerVehicle>,
            Option<&Children>,
        ),
        (With<Car>, Without<DisabledCar>),
    >,
    q_driver: Query<
        (
            Entity,
            &Faction,
            &Health,
            &Transform,
            Option<&EquippedWeapon>,
            Option<&GunState>,
        ),
        With<DriverMesh>,
    >,
) {
    for (car_ent, car_gt, health, is_player, children) in q_cars.iter() {
        if health.current > CAR_DISABLE_HP {
            continue;
        }

        commands
            .entity(car_ent)
            .insert(DisabledCar)
            .remove::<ActivePlayerVehicle>();

        commands.trigger(crate::plugins::visual_fx::CarExplosionEvent {
            position: car_gt.translation(),
        });

        commands
            .entity(car_ent)
            .insert(crate::plugins::visual_fx::SmokeEmitter {
                next_spawn_time: time.elapsed_secs(),
                active_until: time.elapsed_secs() + 15.0,
            });

        // Locate the seated driver mesh, if any.
        let mut driver = None;
        if let Some(children) = children {
            for child in children.iter() {
                if let Ok((d_ent, faction, dhealth, tf, ew, gs)) = q_driver.get(child) {
                    driver = Some((
                        d_ent,
                        *faction,
                        *dhealth,
                        tf.scale.x,
                        ew.cloned(),
                        gs.cloned(),
                    ));
                    break;
                }
            }
        }

        let Some((d_ent, faction, dhealth, scale, ew, gs)) = driver else {
            continue;
        };

        if is_player {
            // The player's car died under them: drop the seated mesh and respawn them as a
            // controllable pedestrian beside the wreck, keeping their remaining HP.
            let car_tf = car_gt.compute_transform();
            let exit_pos = car_tf.translation + car_tf.rotation * Vec3::new(-2.0, 0.2, 0.0);
            let exit_rot = car_tf.rotation * Quat::from_rotation_y(std::f32::consts::PI);
            if let Ok(mut cmds) = commands.get_entity(d_ent) {
                cmds.despawn();
            }
            commands.trigger(SpawnControlledPedestrianEvent {
                position: exit_pos,
                url: None,
                scale: Some(scale),
                is_exiting_car: false,
                rotation: Some(exit_rot),
                health: Some(dhealth),
                weapon: ew,
                gun_state: gs,
            });
        } else {
            eject_driver_as_ai(&mut commands, car_gt, d_ent, faction, dhealth, scale);
        }
    }
}

/// Draws a green warning sphere around every disabled car.
pub fn draw_disabled_car_gizmos(
    mut gizmos: Gizmos,
    q: Query<&GlobalTransform, With<DisabledCar>>,
    settings: Res<crate::plugins::visual_fx::settings::VfxSettings>,
) {
    if !settings.disabled_car_gizmos {
        return;
    }
    for gt in q.iter() {
        gizmos.sphere(gt.translation(), 2.5, Color::srgb(0.1, 1.0, 0.2));
    }
}
