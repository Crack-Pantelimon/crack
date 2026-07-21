//! Freecam right-click "spawn pedestrian / spawn car" choice popup.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use bevy::input::mouse::MouseWheel;
use rand::seq::IndexedRandom;

use super::spawn::{SpawnChoicePopup, SpawnControlledPedestrianEvent};
use super::{AnimState, CAPSULE_HALF_HEIGHT, CombatState, MainCamera, character_physics_bundle};
use crate::plugins::cars_driving::{
    car_info::get_random_car_type,
    driving_plugin::spawn_car::{SpawnCarPassenger, SpawnCarRequestEvent},
};
use crate::plugins::pedestrian_ai::faction::{Enemies, Health};
use crate::plugins::pedestrian_ai::{
    AiAnim, AiCombatTimers, AiModel, AiPedestrian, AiPerception, AiState, AiSteer, AiThink,
    faction::{DEFAULT_HP, Faction},
};
use crate::plugins::pedestrians::ManualAnimation;
use crate::plugins::weapons::{
    EquipWeaponEvent, EquippedWeapon, FireGunEvent, GunState, ReloadGunEvent, WeaponCooldown,
    WeaponId, WeaponManifest,
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
                        health: None,
                        weapon: None,
                        gun_state: None,
                    });
                    close = true;
                }
                if ui.button("🚗 Car").clicked() {
                    commands.trigger(SpawnCarRequestEvent {
                        position: popup.world_pos,
                        car_type: get_random_car_type().to_string(),
                        rotation: None,
                        passengers: vec![
                            None, // driver seat empty (player will enter)
                            Some(SpawnCarPassenger {
                                url: None,
                                weapon: None,
                                faction: Faction::Neutral,
                            }),
                            Some(SpawnCarPassenger {
                                url: None,
                                weapon: None,
                                faction: Faction::Neutral,
                            }),
                            Some(SpawnCarPassenger {
                                url: None,
                                weapon: None,
                                faction: Faction::Neutral,
                            }),
                        ],
                    });
                    close = true;
                }
                if ui.button("🚦 Traffic car").clicked() {
                    commands.trigger(crate::plugins::traffic::SpawnTrafficCarEvent {
                        position: popup.world_pos,
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
use super::{CharacterController, CharacterScale, SCALE_MAX, SCALE_MIN};
use crate::plugins::cars_driving::driving_plugin::camera_follow::DrivingAim;
use crate::plugins::cars_driving::driving_plugin::spawn_car::{
    ActivePlayerVehicle, Car, DisabledCar,
};
use crate::plugins::pedestrians::spawn_pedestrian::{ModelController, ModelRoot};
use crate::plugins::pedestrians::{PedestrianAnimations, PedestrianManifest, SpawnPedestrianEvent};
use crate::plugins::states::GameControlState;

/// Where the driver mesh sits inside the car (car-local), tunable from the Debug menu.
#[derive(Resource)]
pub struct CarSeatOffset {
/// offset field.
    pub offset: Vec3,
/// y rot field.
    pub y_rot: f32,
}

impl Default for CarSeatOffset {
    fn default() -> Self {
        Self {
            offset: Vec3::new(-0.4, 0.2, 0.5),
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
/// car field.
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

#[derive(Component, Debug, Clone)]
pub struct PendingEnterCar {
    pub car: Entity,
    pub until: f32,
}

/// F while looking at a car through the crosshair (hit point within ~1.2m of the
/// character) starts the enter-car sequence.
pub fn detect_car_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    q_player: Query<
        (Entity, &GlobalTransform),
        (With<CharacterController>, Without<EnteringCarTimer>),
    >,
    q_pending: Query<&PendingEnterCar>,
    camera_query: Query<&GlobalTransform, With<MainCamera>>,
    q_cars: Query<(), With<Car>>,
    parents: Query<&ChildOf>,
    spatial_query: SpatialQuery,
    mut commands: Commands,
    q_people: Query<Entity, With<CharacterController>>,
    q_disabled: Query<(), With<DisabledCar>>,
    q_children: Query<&Children>,
    q_driver: Query<(Entity, &DriverMesh, &Faction, &Health, &Transform)>,
    q_car_gt: Query<&GlobalTransform>,
    mut contexts: EguiContexts,
) {
    let egui_wants_keyboard = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_keyboard_input()
    } else {
        false
    };
    if egui_wants_keyboard {
        return;
    }

    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some((ped_entity, ped_tf)) = q_player.iter().next() else {
        return;
    };

    // Clean up expired PendingEnterCar
    if let Ok(pending) = q_pending.get(ped_entity) {
        if time.elapsed_secs() > pending.until {
            commands.entity(ped_entity).remove::<PendingEnterCar>();
        }
    }

    // F2c: Check if we have a pending car and it's valid/empty
    let mut resolved_car = None;
    if let Ok(pending) = q_pending.get(ped_entity) {
        let car = pending.car;
        if time.elapsed_secs() <= pending.until {
            if let Ok(car_gt) = q_car_gt.get(car) {
                // F2d: check proximity to body instead of raycast hit
                if ped_tf.translation().distance(car_gt.translation()) <= 3.0 {
                    if q_disabled.get(car).is_err() {
                        let mut has_driver = false;
                        if let Ok(children) = q_children.get(car) {
                            for child in children.iter() {
                                if q_driver.get(child).is_ok() {
                                    has_driver = true;
                                    break;
                                }
                            }
                        }
                        if !has_driver {
                            resolved_car = Some(car);
                        }
                    }
                }
            }
        }
    }

    if let Some(car) = resolved_car {
        commands.entity(ped_entity).remove::<PendingEnterCar>();
        commands.entity(ped_entity).insert(EnteringCarTimer {
            car_entity: car,
            timer: Timer::from_seconds(1.2, TimerMode::Once),
        });
        return;
    }

    let Some(cam) = camera_query.iter().next() else {
        return;
    };

    // Shot from the camera through the screen-center crosshair (same convention as guns).
    let origin = cam.translation();
    let dir = cam.forward();

    // F2a: Widen excluded set to exclude all active pedestrians (players and AI)
    let mut excluded = vec![ped_entity];
    for entity in q_people.iter() {
        excluded.push(entity);
    }
    let filter = SpatialQueryFilter::default().with_excluded_entities(excluded);
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

    // F2d: character must be standing next to the car itself (proximity to root, 3.0m threshold)
    let Ok(car_gt) = q_car_gt.get(car) else {
        return;
    };
    if ped_tf.translation().distance(car_gt.translation()) > 3.0 {
        return;
    }

    // F2b: replace magic-number health gate with DisabledCar check
    if q_disabled.get(car).is_ok() {
        return; // wrecked cars are not enterable
    }

    // Check if the car has a driver
    let mut driver_info = None;
    if let Ok(children) = q_children.get(car) {
        for child in children.iter() {
            if let Ok((d_ent, _, faction, health, tf)) = q_driver.get(child) {
                driver_info = Some((d_ent, *faction, *health, *tf));
                break;
            }
        }
    }

    if let Some((d_ent, faction, health, tf)) = driver_info {
        // First F next to an occupied car ejects that driver; the player must press F again on the
        // now-empty car to actually get in.
        let car_gt = q_car_gt.get(car).cloned().unwrap_or(*ped_tf);
        eject_driver_as_ai(&mut commands, &car_gt, d_ent, faction, health, tf.scale.x);
        // F2c: Set pending enter car on player so they can press F to enter next time reliably
        commands.entity(ped_entity).insert(PendingEnterCar {
            car,
            until: time.elapsed_secs() + 5.0,
        });
    } else {
        commands.entity(ped_entity).insert(EnteringCarTimer {
            car_entity: car,
            timer: Timer::from_seconds(1.2, TimerMode::Once),
        });
    }
}

pub fn tick_entering_car(
    mut commands: Commands,
    time: Res<Time>,
    mut q_player: Query<(
        Entity,
        &mut EnteringCarTimer,
        &mut Transform,
        &CharacterScale,
    )>,
    q_cars: Query<&GlobalTransform, With<Car>>,
    q_drivers: Query<(Entity, &DriverMesh)>,
    seat: Res<CarSeatOffset>,
    mut controlled: ResMut<ControlledCharacter>,
    mut next_state: ResMut<NextState<GameControlState>>,
    q_player_fh: Query<(
        &Faction,
        &Health,
        Option<&EquippedWeapon>,
        Option<&GunState>,
    )>,
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

            let (faction, health, equipped_weapon, gun_state) = q_player_fh
                .get(entity)
                .map(|(f, h, ew, gs)| (*f, *h, ew.cloned(), gs.cloned()))
                .unwrap_or((Faction::Neutral, Health::full(DEFAULT_HP), None, None));

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
                        faction,
                        health,
                    ));
                    if let Some(ew) = equipped_weapon {
                        model_cmds.insert(ew);
                    }
                    if let Some(gs) = gun_state {
                        model_cmds.insert(gs);
                    }
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

/// ejected driver.
#[derive(Component)]
pub struct EjectedDriver {
/// timer field.
    pub timer: Timer,
/// stage field.
    pub stage: EjectedStage,
}

/// ejected stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EjectedStage {
/// on ground variant.
    OnGround,
/// standing up variant.
    StandingUp,
}

/// tick ejected driver system.
pub fn tick_ejected_driver_system(
    mut commands: Commands,
    time: Res<Time>,
    mut q_ejected: Query<(Entity, &mut EjectedDriver)>,
) {
    for (entity, mut ejected) in q_ejected.iter_mut() {
        ejected.timer.tick(time.delta());
        if ejected.timer.just_finished() {
            match ejected.stage {
                EjectedStage::OnGround => {
                    ejected.stage = EjectedStage::StandingUp;
                    ejected.timer = Timer::from_seconds(1.2, TimerMode::Once);
                }
                EjectedStage::StandingUp => {
                    commands.entity(entity).remove::<EjectedDriver>();
                }
            }
        }
    }
}

/// Ejects a seated driver out of a car, turning the existing (already-loaded) driver mesh into a
/// standalone AI pedestrian that plays the on-ground -> stand-up recovery sequence before it can
/// act. Reuses [`character_physics_bundle`] so the ejected ped is physically identical to a
/// normally-spawned AI ped.
pub fn eject_driver_as_ai(
    commands: &mut Commands,
    car_gt: &GlobalTransform,
    driver_mesh_entity: Entity,
    driver_faction: Faction,
    driver_health: Health,
    scale: f32,
) {
    let car_tf = car_gt.compute_transform();
    let exit_pos = car_tf.translation + car_tf.rotation * Vec3::new(-2.0, 0.2, 0.0);
    let exit_rot = car_tf.rotation * Quat::from_rotation_y(PI);

    // Detach the model from the car and strip its seated-driver bookkeeping. `ManualAnimation` is
    // removed so the shared AI animation system (via `AiModel`) drives its clips again.
    commands
        .entity(driver_mesh_entity)
        .remove::<ChildOf>()
        .remove::<DriverMesh>()
        .remove::<ManualAnimation>()
        .remove::<Faction>()
        .remove::<Health>();

    let controller = commands
        .spawn((
            Name::new("EjectedAiPedestrian"),
            character_physics_bundle(
                scale,
                Transform::from_translation(exit_pos).with_rotation(exit_rot),
            ),
            AnimState::default(),
            CombatState::default(),
        ))
        .insert((
            AiPedestrian,
            driver_faction,
            driver_health,
            AiState::Idle,
            AiPerception::default(),
            AiCombatTimers::default(),
            AiSteer::default(),
            AiAnim::default(),
            AiThink::default(),
            Enemies::default(),
            EjectedDriver {
                timer: Timer::from_seconds(1.5, TimerMode::Once),
                stage: EjectedStage::OnGround,
            },
        ))
        .id();

    let scale_node = commands
        .spawn((
            Name::new("EjectedAiScaleNode"),
            ChildOf(controller),
            Transform::from_xyz(0.0, -CAPSULE_HALF_HEIGHT, 0.0).with_scale(Vec3::splat(scale)),
            Visibility::default(),
        ))
        .id();

    commands
        .entity(driver_mesh_entity)
        .insert((ChildOf(scale_node), Transform::IDENTITY));

    commands
        .entity(controller)
        .insert(AiModel(driver_mesh_entity));
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
    mut contexts: EguiContexts,
) {
    let egui_wants_keyboard = if let Ok(ctx) = contexts.ctx_mut() {
        ctx.egui_wants_keyboard_input()
    } else {
        false
    };
    if egui_wants_keyboard {
        return;
    }

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
            health: None,
            weapon: None,
            gun_state: None,
        });
    }
}

/// Slides the detached driver mesh seat -> door -> beside the car, then despawns it and
/// spawns a fresh controllable pedestrian there (which flips the state back).
pub fn tick_driver_mesh_exit(
    mut commands: Commands,
    time: Res<Time>,
    mut q_exit: Query<(Entity, &mut Transform, &mut DriverMeshExit)>,
    q_driver_info: Query<(&Health, Option<&EquippedWeapon>, Option<&GunState>)>,
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
            // Carry the driver's remaining HP and weapon state out with them so a round-trip through a car is not
            // a free heal.
            let (carried_health, carried_weapon, carried_gun_state) = q_driver_info
                .get(mesh_ent)
                .map(|(h, w, g)| (Some(*h), w.cloned(), g.cloned()))
                .unwrap_or((None, None, None));
            if let Ok(mut mesh_cmds) = commands.get_entity(mesh_ent) {
                mesh_cmds.despawn();
            }
            commands.trigger(SpawnControlledPedestrianEvent {
                position: spawn_pos,
                url: None,
                scale: None,
                is_exiting_car: false,
                rotation: Some(spawn_rot),
                health: carried_health,
                weapon: carried_weapon,
                gun_state: carried_gun_state,
            });
        }
    }
}

/// Top-left FPS-style weapon HUD: weapon name in green, and below it in larger font
/// `<bullets_remaining>/<bullets_clip>` (with the zero in red if out of ammo).
pub fn weapon_hud_ui(
    mut contexts: EguiContexts,
    controlled: Res<ControlledCharacter>,
    equipped: Query<(&EquippedWeapon, Option<&GunState>, &Health)>,
    state: Res<State<GameControlState>>,
) {
    if *state.get() != GameControlState::ControllingPedestrian {
        return;
    }
    let Some(controller) = controlled.controller else {
        return;
    };
    let Ok((equipped_weapon, gun_state, health)) = equipped.get(controller) else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let weapon_name = equipped_weapon.0.label();

    egui::Area::new(egui::Id::new("weapon_hud_area"))
        .fixed_pos(egui::pos2(16.0, 16.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                // Top line: Weapon name in green text (FPS style)
                ui.label(
                    egui::RichText::new(&weapon_name)
                        .color(egui::Color32::from_rgb(0, 255, 0))
                        .size(18.0)
                        .strong(),
                );

                // Display player health
                let hp_color = if health.current < 30.0 {
                    egui::Color32::from_rgb(255, 50, 50)
                } else {
                    egui::Color32::from_rgb(0, 255, 0)
                };
                ui.label(
                    egui::RichText::new(format!("HP: {:.0}/{:.0}", health.current, health.max))
                        .color(hp_color)
                        .size(20.0)
                        .strong(),
                );

                // Bottom line (if gun): ammo display in larger font
                if let Some(gun) = gun_state {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let font_size = 32.0;
                        let green = egui::Color32::from_rgb(0, 255, 0);
                        let red = egui::Color32::from_rgb(255, 50, 50);

                        if gun.rounds == 0 {
                            ui.label(egui::RichText::new("0").color(red).size(font_size).strong());
                        } else {
                            ui.label(
                                egui::RichText::new(gun.rounds.to_string())
                                    .color(green)
                                    .size(font_size)
                                    .strong(),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!("/{}", gun.clip_size))
                                .color(green)
                                .size(font_size)
                                .strong(),
                        );
                    });
                }
            });
        });
}

/// Selected weapon index into `WeaponManifest.all`.
#[derive(Resource, Default)]
pub struct WeaponSelection {
    pub index: usize,
}

/// Equip a random weapon whenever a new character is spawned.
pub fn equip_on_new_character(
    mut commands: Commands,
    controlled: Res<ControlledCharacter>,
    manifest: Res<WeaponManifest>,
    mut selection: ResMut<WeaponSelection>,
    mut last: Local<Option<Entity>>,
    q_equipped: Query<&EquippedWeapon>,
) {
    if !manifest.loaded {
        return;
    }
    let Some(controller) = controlled.controller else {
        *last = None;
        return;
    };
    if *last == Some(controller) {
        return;
    }
    *last = Some(controller);

    if q_equipped.get(controller).is_ok() {
        return;
    }

    // Pick a random real weapon (skip Unarmed at index 0), fall back to Unarmed.
    let weapon = manifest.all[1..]
        .choose(&mut rand::rng())
        .cloned()
        .unwrap_or(WeaponId::Unarmed);
    selection.index = manifest.all.iter().position(|w| *w == weapon).unwrap_or(0);
    commands.trigger(EquipWeaponEvent {
        character: controller,
        weapon,
    });
}

/// Reads a debounced scroll step (-1/0/+1) from the mouse wheel, ignoring input while the
/// pointer is over egui and enforcing a fixed switch cooldown. Shared by [`weapon_wheel`] and
/// [`driving_weapon_wheel`].
fn read_scroll_step(
    time: &Time,
    next_switch: &mut f32,
    wheel: &mut MessageReader<MouseWheel>,
    contexts: &mut EguiContexts,
) -> Option<i32> {
    let over_ui = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
        .unwrap_or(false);
    if over_ui {
        wheel.clear();
        return None;
    }
    let mut step = 0i32;
    for ev in wheel.read() {
        if ev.y > 0.0 {
            step += 1;
        } else if ev.y < 0.0 {
            step -= 1;
        }
    }
    let step = step.signum();
    if step == 0 {
        return None;
    }
    let now = time.elapsed_secs();
    if now < *next_switch {
        return None;
    }
    *next_switch = now + 0.15;
    Some(step)
}

/// Cycles `current_index` by `step` within the weapon manifest. When `guns_only` is set (the
/// driving weapon wheel), cycles only through gun entries, snapping onto the nearest gun if the
/// current selection is unarmed/melee. Shared by [`weapon_wheel`] and [`driving_weapon_wheel`].
fn cycle_weapon(
    manifest: &WeaponManifest,
    current_index: usize,
    step: i32,
    guns_only: bool,
) -> usize {
    if guns_only {
        let gun_indices: Vec<usize> = manifest
            .all
            .iter()
            .enumerate()
            .filter(|(_, w)| w.is_gun())
            .map(|(i, _)| i)
            .collect();
        if gun_indices.is_empty() {
            return current_index;
        }
        let n = gun_indices.len() as i32;
        let next_pos = match gun_indices.iter().position(|&i| i == current_index) {
            Some(pos) => (((pos as i32 + step) % n + n) % n) as usize,
            None => {
                if step > 0 {
                    0
                } else {
                    gun_indices.len() - 1
                }
            }
        };
        gun_indices[next_pos]
    } else {
        let n = manifest.all.len() as i32;
        if n == 0 {
            return current_index;
        }
        (((current_index as i32 + step) % n + n) % n) as usize
    }
}

/// Mouse wheel cycles to the next/previous weapon.
pub fn weapon_wheel(
    time: Res<Time>,
    mut next_switch: Local<f32>,
    mut commands: Commands,
    mut wheel: MessageReader<MouseWheel>,
    mut contexts: EguiContexts,
    controlled: Res<ControlledCharacter>,
    manifest: Res<WeaponManifest>,
    mut selection: ResMut<WeaponSelection>,
) {
    if !manifest.loaded || manifest.all.is_empty() {
        wheel.clear();
        return;
    }
    let Some(step) = read_scroll_step(&time, &mut next_switch, &mut wheel, &mut contexts) else {
        return;
    };
    let Some(controller) = controlled.controller else {
        return;
    };

    selection.index = cycle_weapon(&manifest, selection.index, step, false);
    commands.trigger(EquipWeaponEvent {
        character: controller,
        weapon: manifest.all[selection.index].clone(),
    });
}

/// Requests a seated player-driver mesh (with a weapon) for a freshly-spawned controllable car.
/// Cars spawned straight into [`GameControlState::DrivingCar`] from the freecam menu have no
/// driver mesh, so without this the player had no weapon until getting out and back in.
#[derive(Event)]
pub struct SpawnPlayerDriverEvent {
/// car field.
    pub car: Entity,
}

/// Placeholder controller that owns a loading driver model. When the model's [`ModelRoot`]
/// appears, [`finalize_car_drivers`] converts the model into a seated [`DriverMesh`] on the car.
#[derive(Component)]
pub struct PendingCarDriver {
    pub car: Entity,
    pub weapon: WeaponId,
    pub scale: f32,
}

/// Kicks off loading a random pedestrian model + weapon for the car's driver seat.
pub fn spawn_player_driver_observer(
    trigger: On<SpawnPlayerDriverEvent>,
    mut commands: Commands,
    ped_manifest: Res<PedestrianManifest>,
    weapon_manifest: Res<WeaponManifest>,
    seat: Res<CarSeatOffset>,
    q_car: Query<&GlobalTransform, With<Car>>,
) {
    let car = trigger.event().car;
    let Some(url) = ped_manifest.urls.choose(&mut rand::rng()).cloned() else {
        warn!("SpawnPlayerDriverEvent: pedestrian manifest empty");
        return;
    };

    // Prefer a gun so driveby works immediately; fall back to any non-Unarmed weapon, then Unarmed.
    let guns: Vec<WeaponId> = weapon_manifest
        .all
        .iter()
        .filter(|w| w.is_gun())
        .cloned()
        .collect();
    let weapon = guns
        .choose(&mut rand::rng())
        .cloned()
        .or_else(|| weapon_manifest.all.get(1).cloned())
        .unwrap_or(WeaponId::Unarmed);

    let scale = SCALE_MIN + rand::random::<f32>() * (SCALE_MAX - SCALE_MIN);

    // World position of the driver seat so the loading model appears roughly in place.
    let seat_world = q_car
        .get(car)
        .map(|gt| {
            let t = gt.compute_transform();
            t.translation + t.rotation * seat.offset
        })
        .unwrap_or(Vec3::ZERO);

    let placeholder = commands
        .spawn((
            Name::new("PendingCarDriver"),
            Transform::from_translation(seat_world),
            Visibility::default(),
            PendingCarDriver {
                car,
                weapon: weapon.clone(),
                scale,
            },
        ))
        .id();

    commands.trigger(SpawnPedestrianEvent {
        url,
        position: seat_world,
        controller: placeholder,
        parent: placeholder,
    });
}

/// Once a pending-driver model has loaded, seat it in the car as a [`DriverMesh`] and equip its
/// weapon — reaching the same state a driver has after [`tick_entering_car`].
pub fn finalize_car_drivers(
    mut commands: Commands,
    seat: Res<CarSeatOffset>,
    q_new_models: Query<(Entity, &ModelController), Added<ModelRoot>>,
    q_pending: Query<&PendingCarDriver>,
) {
    for (model_ent, controller_ref) in &q_new_models {
        let Ok(pending) = q_pending.get(controller_ref.0) else {
            continue;
        };
        let car = pending.car;

        // Re-parent the model onto the car *before* despawning the placeholder, so the
        // placeholder's recursive despawn does not take the model with it.
        commands.entity(model_ent).insert((
            ChildOf(car),
            DriverMesh {
                car,
                anim_node: None,
            },
            Transform::from_translation(seat.offset)
                .with_rotation(Quat::from_rotation_y(seat.y_rot))
                .with_scale(Vec3::splat(pending.scale)),
            Faction::Neutral,
            Health::full(DEFAULT_HP),
        ));
        commands.trigger(EquipWeaponEvent {
            character: model_ent,
            weapon: pending.weapon.clone(),
        });
        if let Ok(mut cmds) = commands.get_entity(controller_ref.0) {
            cmds.despawn();
        }
    }
}

/// Driveby: while driving, LMB fires the seated driver's gun out of the car. RMB aims (the
/// arm-IK system extends the driver's arm to the crosshair). Only the player driver — the
/// [`DriverMesh`] parented to the active car — fires; passengers stay armed but passive.
pub fn driveby_fire(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    aim: Res<DrivingAim>,
    mut contexts: EguiContexts,
    mut commands: Commands,
    q_active_car: Query<Entity, With<ActivePlayerVehicle>>,
    mut q_driver: Query<(
        Entity,
        &DriverMesh,
        &GlobalTransform,
        &EquippedWeapon,
        &mut GunState,
        Option<&mut WeaponCooldown>,
    )>,
) {
    let Some(car) = q_active_car.iter().next() else {
        return;
    };
    let over_ui = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
        .unwrap_or(false);
    if over_ui {
        return;
    }

    let Some((driver_ent, _, driver_gt, equipped, mut gun, mut cooldown)) =
        q_driver.iter_mut().find(|(_, d, _, _, _, _)| d.car == car)
    else {
        return;
    };
    if !equipped.0.is_gun() {
        return;
    }

    // R reloads a partly-spent clip.
    if keys.just_pressed(KeyCode::KeyR) && gun.reload_timer <= 0.0 && gun.rounds < gun.clip_size {
        commands.trigger(ReloadGunEvent {
            shooter: driver_ent,
        });
        return;
    }

    let automatic = equipped.0.automatic();
    let fire_pressed = aim.aiming
        && (mouse.just_pressed(MouseButton::Left)
            || (automatic && mouse.pressed(MouseButton::Left)));
    let cooldown_ready = cooldown.as_ref().map_or(true, |cd| cd.0 <= 0.0);
    if !fire_pressed || !cooldown_ready || gun.reload_timer > 0.0 {
        return;
    }

    let cooldown_secs = 60.0 / equipped.0.rpm();
    if gun.rounds > 0 {
        commands.trigger(FireGunEvent {
            shooter: driver_ent,
        });
    } else {
        // Empty: click, and auto-reload after a few dry fires (mirrors the on-foot logic).
        gun.empty_click_count += 1;
        commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
            fx: crate::plugins::audio::audio_fx::AudioFxEventType::EmptyClick,
            position: driver_gt.translation(),
            follow: None,
        });
        if gun.empty_click_count >= 3 {
            gun.empty_click_count = 0;
            commands.trigger(ReloadGunEvent {
                shooter: driver_ent,
            });
        }
    }

    if let Some(cd) = cooldown.as_mut() {
        cd.0 = cooldown_secs;
    } else {
        commands
            .entity(driver_ent)
            .insert(WeaponCooldown(cooldown_secs));
    }
}

/// Crosshair while driving an armed car (the on-foot `crosshair_ui` is gone with the controller).
pub fn driving_crosshair_ui(
    aim: Res<DrivingAim>,
    mut contexts: EguiContexts,
    q_active_car: Query<Entity, With<ActivePlayerVehicle>>,
    q_driver: Query<(&DriverMesh, &EquippedWeapon)>,
) {
    if !aim.aiming {
        return;
    }
    let Some(car) = q_active_car.iter().next() else {
        return;
    };
    let has_gun = q_driver.iter().any(|(d, w)| d.car == car && w.0.is_gun());
    if !has_gun {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("driving_crosshair"),
    ));
    let center = ctx.content_rect().center();
    let color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 178);
    painter.circle_stroke(center, 10.0, egui::Stroke::new(1.5, color));
    painter.circle_filled(center, 2.0, color);
}

/// Mouse wheel cycles the seated driver's weapon while driving (mirrors the on-foot
/// [`weapon_wheel`], but targets the active car's [`DriverMesh`] instead of the controller).
pub fn driving_weapon_wheel(
    time: Res<Time>,
    mut next_switch: Local<f32>,
    mut commands: Commands,
    mut wheel: MessageReader<MouseWheel>,
    mut contexts: EguiContexts,
    manifest: Res<WeaponManifest>,
    mut selection: ResMut<WeaponSelection>,
    q_active_car: Query<Entity, With<ActivePlayerVehicle>>,
    q_driver: Query<(Entity, &DriverMesh)>,
) {
    if !manifest.loaded || manifest.all.is_empty() {
        wheel.clear();
        return;
    }
    let Some(step) = read_scroll_step(&time, &mut next_switch, &mut wheel, &mut contexts) else {
        return;
    };
    let Some(car) = q_active_car.iter().next() else {
        return;
    };
    let Some((driver_ent, _)) = q_driver.iter().find(|(_, d)| d.car == car) else {
        return;
    };

    // Only guns make sense from the driver's seat: cycle through gun entries exclusively,
    // skipping unarmed/melee entirely.
    selection.index = cycle_weapon(&manifest, selection.index, step, true);
    commands.trigger(EquipWeaponEvent {
        character: driver_ent,
        weapon: manifest.all[selection.index].clone(),
    });
}

/// Top-left weapon HUD while driving (mirrors the on-foot [`weapon_hud_ui`], but reads the
/// active car's [`DriverMesh`]): green weapon name, HP, and `<rounds>/<clip_size>`.
pub fn driving_weapon_hud_ui(
    mut contexts: EguiContexts,
    q_active_car: Query<Entity, With<ActivePlayerVehicle>>,
    q_driver: Query<(&DriverMesh, &EquippedWeapon, Option<&GunState>, &Health)>,
) {
    let Some(car) = q_active_car.iter().next() else {
        return;
    };
    let Some((_, equipped_weapon, gun_state, health)) =
        q_driver.iter().find(|(d, _, _, _)| d.car == car)
    else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let weapon_name = equipped_weapon.0.label();

    egui::Area::new(egui::Id::new("driving_weapon_hud_area"))
        .fixed_pos(egui::pos2(16.0, 16.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(&weapon_name)
                        .color(egui::Color32::from_rgb(0, 255, 0))
                        .size(18.0)
                        .strong(),
                );

                let hp_color = if health.current < 30.0 {
                    egui::Color32::from_rgb(255, 50, 50)
                } else {
                    egui::Color32::from_rgb(0, 255, 0)
                };
                ui.label(
                    egui::RichText::new(format!("HP: {:.0}/{:.0}", health.current, health.max))
                        .color(hp_color)
                        .size(20.0)
                        .strong(),
                );

                if let Some(gun) = gun_state {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let font_size = 32.0;
                        let green = egui::Color32::from_rgb(0, 255, 0);
                        let red = egui::Color32::from_rgb(255, 50, 50);

                        if gun.rounds == 0 {
                            ui.label(egui::RichText::new("0").color(red).size(font_size).strong());
                        } else {
                            ui.label(
                                egui::RichText::new(gun.rounds.to_string())
                                    .color(green)
                                    .size(font_size)
                                    .strong(),
                            );
                        }
                        ui.label(
                            egui::RichText::new(format!("/{}", gun.clip_size))
                                .color(green)
                                .size(font_size)
                                .strong(),
                        );
                    });
                }
            });
        });
}

/// White (70% alpha) crosshair at screen center when holding a gun.
pub fn crosshair_ui(
    mut contexts: EguiContexts,
    controlled: Res<ControlledCharacter>,
    guns: Query<&GunState>,
    state: Res<State<GameControlState>>,
) {
    if *state.get() != GameControlState::ControllingPedestrian {
        return;
    }
    let Some(controller) = controlled.controller else {
        return;
    };
    if guns.get(controller).is_err() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));
    let center = ctx.content_rect().center();
    let color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 178);
    painter.circle_stroke(center, 10.0, egui::Stroke::new(1.5, color));
    painter.circle_filled(center, 2.0, color);
}
