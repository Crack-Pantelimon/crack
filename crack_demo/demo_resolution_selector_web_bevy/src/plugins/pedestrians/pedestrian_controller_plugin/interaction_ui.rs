//! Freecam right-click "spawn pedestrian / spawn car" choice popup.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use bevy::input::mouse::MouseWheel;
use rand::seq::IndexedRandom;

use super::spawn::{SpawnChoicePopup, SpawnControlledPedestrianEvent};
use super::{AnimState, CAPSULE_HALF_HEIGHT, CombatState, character_physics_bundle};
use crate::plugins::cars_driving::{
    car_info::get_random_car_type, driving_plugin::spawn_car::SpawnCarRequestEvent,
};
use crate::plugins::pedestrian_ai::faction::{Enemies, Health};
use crate::plugins::pedestrian_ai::{
    AiAnim, AiCombatTimers, AiModel, AiPedestrian, AiPerception, AiState, AiSteer, AiThink,
    faction::{DEFAULT_HP, Faction},
};
use crate::plugins::pedestrians::ManualAnimation;
use crate::plugins::weapons::{
    EquipWeaponEvent, EquippedWeapon, GunState, WeaponId, WeaponManifest,
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
    q_car_health: Query<&crate::plugins::cars_driving::driving_plugin::spawn_car::CarHealth>,
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

    // Check if the car is disabled
    if let Ok(car_health) = q_car_health.get(car) {
        if car_health.current < 100.0 {
            return; // Block entering / interacting
        }
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
    q_player_fh: Query<(&Faction, &Health)>,
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

            let (faction, health) = q_player_fh
                .get(entity)
                .map(|(f, h)| (*f, *h))
                .unwrap_or((Faction::Neutral, Health::full(DEFAULT_HP)));

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

#[derive(Component)]
pub struct EjectedDriver {
    pub timer: Timer,
    pub stage: EjectedStage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EjectedStage {
    OnGround,
    StandingUp,
}

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
        });
    }
}

/// Slides the detached driver mesh seat -> door -> beside the car, then despawns it and
/// spawns a fresh controllable pedestrian there (which flips the state back).
pub fn tick_driver_mesh_exit(
    mut commands: Commands,
    time: Res<Time>,
    mut q_exit: Query<(Entity, &mut Transform, &mut DriverMeshExit)>,
    q_health: Query<&Health>,
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
            // Carry the driver's remaining HP out with them so a round-trip through a car is not
            // a free heal.
            let carried_health = q_health.get(mesh_ent).ok().copied();
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

/// Mouse wheel cycles to the next/previous weapon.
pub fn weapon_wheel(
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
    let over_ui = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_egui() || c.egui_wants_pointer_input())
        .unwrap_or(false);

    let mut step = 0i32;
    for ev in wheel.read() {
        if ev.y > 0.0 {
            step += 1;
        } else if ev.y < 0.0 {
            step -= 1;
        }
    }
    let step = step.signum();
    if step == 0 || over_ui {
        return;
    }
    let Some(controller) = controlled.controller else {
        return;
    };

    let n = manifest.all.len() as i32;
    selection.index = (((selection.index as i32 + step) % n + n) % n) as usize;
    commands.trigger(EquipWeaponEvent {
        character: controller,
        weapon: manifest.all[selection.index].clone(),
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
