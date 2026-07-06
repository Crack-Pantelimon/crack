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

use std::f32::consts::PI;

use super::animation::node_for;
use super::spawn::ControlledCharacter;
use super::{CharacterController, CharacterScale};
use crate::plugins::cars_driving::driving_plugin::spawn_car::{ActivePlayerVehicle, Car};
use crate::plugins::pedestrians::PedestrianAnimations;
use crate::plugins::states::GameControlState;

/// Where the driver mesh sits inside the car (car-local), tunable from the Debug menu.
#[derive(Resource)]
pub struct CarSeatOffset {
    pub offset: Vec3,
    pub y_rot: f32,
}

impl Default for CarSeatOffset {
    fn default() -> Self {
        Self {
            offset: Vec3::new(-0.4, 0.2, 0.0),
            y_rot: PI,
        }
    }
}

#[derive(Component)]
pub struct EnteringCarTimer {
    pub car_entity: Entity,
    pub timer: Timer,
}

/// The pedestrian's visual model, re-parented into the car while driving.
#[derive(Component)]
pub struct DriverMesh {
    pub car: Entity,
    /// The animation graph node currently playing on this mesh's player.
    pub anim_node: Option<AnimationNodeIndex>,
}

/// An in-progress "get out of car" move: the detached driver mesh slides seat -> door ->
/// spot beside the car while `Sitting_Exit` plays, then a fresh pedestrian takes over.
#[derive(Component)]
pub struct DriverMeshExit {
    pub timer: Timer,
    pub from_pos: Vec3,
    pub from_rot: Quat,
    pub door_pos: Vec3,
    pub exit_pos: Vec3,
    pub exit_rot: Quat,
}

/// F while looking at a car through the crosshair (hit point within ~1.2m of the
/// character) starts the enter-car sequence.
pub fn detect_car_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    q_player: Query<
        (Entity, &GlobalTransform),
        (With<CharacterController>, Without<EnteringCarTimer>),
    >,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    q_cars: Query<(), With<Car>>,
    parents: Query<&ChildOf>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some((ped_entity, ped_tf)) = q_player.iter().next() else {
        return;
    };
    let Some(cam) = camera_query.iter().next() else {
        return;
    };

    // Shot from the camera through the screen-center crosshair (same convention as guns).
    let origin = cam.translation();
    let dir = cam.forward();
    let filter = SpatialQueryFilter::default().with_excluded_entities([ped_entity]);
    let Some(hit) = spatial_query.cast_ray(origin, dir, 30.0, true, &filter) else {
        return;
    };

    // The hit collider must belong to a car (colliders live on GLB child meshes).
    let mut car_root = None;
    let mut cur = hit.entity;
    loop {
        if q_cars.get(cur).is_ok() {
            car_root = Some(cur);
            break;
        }
        match parents.get(cur) {
            Ok(child_of) => cur = child_of.0,
            Err(_) => break,
        }
    }
    let Some(car) = car_root else {
        return;
    };

    // ...and the character must be standing next to it.
    let hit_point = origin + *dir * hit.distance;
    if ped_tf.translation().distance(hit_point) > 1.2 {
        return;
    }

    commands.entity(ped_entity).insert(EnteringCarTimer {
        car_entity: car,
        timer: Timer::from_seconds(1.2, TimerMode::Once),
    });
}

pub fn tick_entering_car(
    mut commands: Commands,
    time: Res<Time>,
    mut q_player: Query<(Entity, &mut EnteringCarTimer, &mut Transform, &CharacterScale)>,
    q_cars: Query<&GlobalTransform, With<Car>>,
    q_drivers: Query<(Entity, &DriverMesh)>,
    seat: Res<CarSeatOffset>,
    mut controlled: ResMut<ControlledCharacter>,
    mut next_state: ResMut<NextState<GameControlState>>,
) {
    for (entity, mut entering, mut ped_transform, char_scale) in q_player.iter_mut() {
        // Interpolate position to the car door, then onto the seat
        if let Ok(car_gt) = q_cars.get(entering.car_entity) {
            let car_tf = car_gt.compute_transform();

            // Door is to the left (negative X in car-local space), seat near the middle
            let door_pos = car_tf.translation + car_tf.rotation * Vec3::new(-1.2, 0.0, 0.0);
            let seat_pos = car_tf.translation + car_tf.rotation * seat.offset;

            let progress = entering.timer.fraction();
            let target_pos = if progress < 0.5 { door_pos } else { seat_pos };

            ped_transform.translation = ped_transform
                .translation
                .lerp(target_pos, time.delta_secs() * 5.0);

            // Face the driver orientation (rotated 180 deg around Y relative to car)
            let target_rot = car_tf.rotation * Quat::from_rotation_y(seat.y_rot);
            ped_transform.rotation = ped_transform
                .rotation
                .slerp(target_rot, time.delta_secs() * 5.0);
        }

        entering.timer.tick(time.delta());
        if entering.timer.just_finished() {
            // A driver mesh may already be seated (control was released with Escape);
            // remove it before seating the new one.
            for (old_driver, driver) in q_drivers.iter() {
                if driver.car == entering.car_entity {
                    if let Ok(mut cmds) = commands.get_entity(old_driver) {
                        cmds.despawn();
                    }
                }
            }

            // Steal the visual model from the controller and seat it in the car; the
            // physics capsule and controller components despawn with the controller.
            if let Some(ped_model) = controlled.ped {
                if let Ok(mut model_cmds) = commands.get_entity(ped_model) {
                    model_cmds.insert((
                        ChildOf(entering.car_entity),
                        DriverMesh {
                            car: entering.car_entity,
                            anim_node: None,
                        },
                        Transform::from_translation(seat.offset)
                            .with_rotation(Quat::from_rotation_y(seat.y_rot))
                            .with_scale(Vec3::splat(char_scale.0)),
                    ));
                }
            }
            if controlled.controller == Some(entity) {
                controlled.controller = None;
            }
            controlled.ped = None;
            controlled.scale_node = None;
            controlled.awaiting = false;

            if let Ok(mut entity_cmds) = commands.get_entity(entity) {
                entity_cmds.despawn();
            }
            if let Ok(mut car_cmds) = commands.get_entity(entering.car_entity) {
                car_cmds.insert(ActivePlayerVehicle);
            }
            next_state.set(GameControlState::DrivingCar);
        }
    }
}

/// Plays the driving loop on seated driver meshes, or `Sitting_Exit` while getting out.
pub fn drive_driver_mesh_animation(
    anims: Res<PedestrianAnimations>,
    mut q_driver: Query<(Entity, &mut DriverMesh, Has<DriverMeshExit>)>,
    mut players: Query<(Entity, &mut AnimationPlayer)>,
    parents: Query<&ChildOf>,
) {
    if !anims.ready {
        return;
    }
    for (driver_ent, mut driver, exiting) in q_driver.iter_mut() {
        let candidates: &[&str] = if exiting {
            &["Sitting_Exit"]
        } else {
            &["Driving_Loop", "Sitting_Idle_Loop", "Sitting_Enter"]
        };
        let Some(node) = node_for(&anims, candidates) else {
            continue;
        };
        if driver.anim_node == Some(node) {
            continue;
        }

        // Find the AnimationPlayer that descends from this driver mesh.
        let mut found = None;
        for (player_ent, _) in players.iter() {
            let mut cur = player_ent;
            loop {
                if cur == driver_ent {
                    found = Some(player_ent);
                    break;
                }
                match parents.get(cur) {
                    Ok(child_of) => cur = child_of.0,
                    Err(_) => break,
                }
            }
            if found.is_some() {
                break;
            }
        }
        let Some(player_ent) = found else {
            continue;
        };
        let Ok((_, mut player)) = players.get_mut(player_ent) else {
            continue;
        };

        player.stop_all();
        let active = player.play(node);
        if exiting {
            active.seek_to(0.0);
        } else {
            active.repeat();
        }
        driver.anim_node = Some(node);
    }
}

/// Live-applies the Debug menu seat offset to seated driver meshes.
pub fn apply_seat_offset(
    seat: Res<CarSeatOffset>,
    mut q_driver: Query<&mut Transform, (With<DriverMesh>, Without<DriverMeshExit>)>,
) {
    if !seat.is_changed() {
        return;
    }
    for mut tf in q_driver.iter_mut() {
        tf.translation = seat.offset;
        tf.rotation = Quat::from_rotation_y(seat.y_rot);
    }
}

/// Debug menu (only while driving): sliders for the driver seat offset.
pub fn car_seat_debug_ui(
    mut contexts: EguiContexts,
    mut seat: ResMut<CarSeatOffset>,
    q_driver: Query<(), With<DriverMesh>>,
) {
    if q_driver.is_empty() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new("Debug: Car Seat")
        .default_open(false)
        .show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut seat.offset.x, -2.0..=2.0).text("Seat X"));
            ui.add(egui::Slider::new(&mut seat.offset.y, -2.0..=2.0).text("Seat Y"));
            ui.add(egui::Slider::new(&mut seat.offset.z, -2.0..=2.0).text("Seat Z"));
            ui.add(egui::Slider::new(&mut seat.y_rot, -PI..=PI).text("Y rotation"));
        });
}

/// F while driving: release the car (it keeps its physics and coasts), detach the driver
/// mesh, and animate it out of the car before handing control back to a pedestrian.
pub fn handle_exit_car(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    q_active_car: Query<(Entity, &GlobalTransform), With<ActivePlayerVehicle>>,
    q_driver: Query<(Entity, &GlobalTransform, &DriverMesh)>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some((car_entity, car_tf)) = q_active_car.iter().next() else {
        return;
    };
    if let Ok(mut car_cmds) = commands.get_entity(car_entity) {
        car_cmds.remove::<ActivePlayerVehicle>();
    }

    let (_, car_rot, car_trans) = car_tf.to_scale_rotation_translation();
    let door_pos = car_trans + car_rot * Vec3::new(-1.2, 0.2, 0.0);
    let exit_pos = car_trans + car_rot * Vec3::new(-2.0, 0.2, 0.0);
    let exit_rot = car_rot * Quat::from_rotation_y(PI);

    let mut found_driver = false;
    for (mesh_ent, mesh_gt, driver) in q_driver.iter() {
        if driver.car != car_entity {
            continue;
        }
        found_driver = true;
        let world_tf = mesh_gt.compute_transform();
        if let Ok(mut mesh_cmds) = commands.get_entity(mesh_ent) {
            mesh_cmds.remove::<ChildOf>();
            mesh_cmds.insert((
                world_tf,
                DriverMeshExit {
                    timer: Timer::from_seconds(1.2, TimerMode::Once),
                    from_pos: world_tf.translation,
                    from_rot: world_tf.rotation,
                    door_pos,
                    exit_pos,
                    exit_rot,
                },
            ));
        }
    }

    // If the model never loaded there is nothing to animate: hand over immediately.
    if !found_driver {
        commands.trigger(SpawnControlledPedestrianEvent {
            position: exit_pos,
            url: None,
            scale: None,
            is_exiting_car: false,
            rotation: Some(exit_rot),
        });
    }
}

/// Slides the detached driver mesh seat -> door -> beside the car, then despawns it and
/// spawns a fresh controllable pedestrian there (which flips the state back).
pub fn tick_driver_mesh_exit(
    mut commands: Commands,
    time: Res<Time>,
    mut q_exit: Query<(Entity, &mut Transform, &mut DriverMeshExit)>,
) {
    for (mesh_ent, mut tf, mut exit) in q_exit.iter_mut() {
        exit.timer.tick(time.delta());
        let t = exit.timer.fraction();

        let pos = if t < 0.5 {
            exit.from_pos.lerp(exit.door_pos, t / 0.5)
        } else {
            exit.door_pos.lerp(exit.exit_pos, (t - 0.5) / 0.5)
        };
        tf.translation = pos;
        tf.rotation = exit.from_rot.slerp(exit.exit_rot, (t * 2.0).min(1.0));

        if exit.timer.just_finished() {
            let spawn_pos = exit.exit_pos;
            let spawn_rot = exit.exit_rot;
            if let Ok(mut mesh_cmds) = commands.get_entity(mesh_ent) {
                mesh_cmds.despawn();
            }
            commands.trigger(SpawnControlledPedestrianEvent {
                position: spawn_pos,
                url: None,
                scale: None,
                is_exiting_car: false,
                rotation: Some(spawn_rot),
            });
        }
    }
}

