//! Standalone demo harness for the [`PedestrianControllerPlugin`].
//!
//! Sets up a flat debug scene with some physics cubes, then auto-spawns a random controllable
//! pedestrian once the manifest loads. All the controller logic lives in the reusable library
//! module `plugins::pedestrians::pedestrian_controller_plugin` (also used by the main game).
//!
//! Controls: WASD move · Space jump · C crouch · Shift sprint · LMB jab · RMB(hold) aim ·
//! LMB+RMB shoot · Esc back to freecam. In freecam, right-click to open the spawn popup.

use avian3d::prelude::*;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use bevy_egui::{egui, EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use rand::seq::IndexedRandom;

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        cars_driving::{car_info::get_car_asset, car_info::get_random_car_type},
        cars_driving::driving_plugin::GamePhysicsLayer,
        pedestrians::{
            pedestrian_controller_plugin::{
                ControlledCharacter, PedestrianControllerPlugin, SpawnControlledPedestrianEvent,
            },
            PedestrianManifest, PedestriansPlugin,
        },
        states::GameControlState,
        weapons::{
            EquipWeaponEvent, GunState, WeaponGripOffset, WeaponId, WeaponManifest, WeaponsPlugin,
        },
    },
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

/// The weapon currently selected in the demo UI (index into `WeaponManifest.all`).
#[derive(Resource, Default)]
struct WeaponSelection {
    index: usize,
}

/// Approximate car body extents (matches `CarDriveState` defaults) used for the mass density.
const CAR_SIZE: Vec3 = Vec3::new(1.8, 1.0, 3.04);
const CAR_MASS: f32 = 1200.0;

fn main() {
    make_basic_app("Pedestrian Controller")
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(PhysicsDebugPlugin::default())
        .init_state::<GameControlState>()
        .add_plugins(PedestriansPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(PedestrianControllerPlugin)
        .add_plugins(WeaponsPlugin)
        .init_resource::<WeaponSelection>()
        .add_systems(Startup, (spawn_physics_cubes, spawn_random_cars))
        .add_systems(
            Update,
            (
                demo_auto_spawn,
                equip_on_new_character,
                weapon_wheel.run_if(in_state(GameControlState::ControllingPedestrian)),
            ),
        )
        .add_systems(EguiPrimaryContextPass, (weapon_ui, crosshair_ui))
        .run();
}

/// White (70% alpha) crosshair — a dot with a circle around it — at the screen center. Shown while
/// controlling a pedestrian that holds a gun; hitscan shots go where it points.
fn crosshair_ui(
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
    // White with ~70% alpha.
    let color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 178);
    painter.circle_stroke(center, 10.0, egui::Stroke::new(1.5, color));
    painter.circle_filled(center, 2.0, color);
}

/// Equip a random weapon whenever a new character is spawned.
fn equip_on_new_character(
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
    commands.trigger(EquipWeaponEvent { character: controller, weapon });
}

/// Mouse wheel cycles to the next/previous weapon.
fn weapon_wheel(
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
    // Never switch more than one weapon per frame, no matter how hard the wheel was rolled.
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

/// egui window: grip-offset slider on top, then the weapon list.
fn weapon_ui(
    mut commands: Commands,
    mut contexts: EguiContexts,
    controlled: Res<ControlledCharacter>,
    manifest: Res<WeaponManifest>,
    guns: Query<&GunState>,
    mut grip: ResMut<WeaponGripOffset>,
    mut selection: ResMut<WeaponSelection>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new("Weapons")
        .default_pos(egui::pos2(12.0, 50.0))
        .default_size(egui::vec2(240.0, 360.0))
        .show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut grip.0, 0.05..=0.5).text("Grip offset"));
            if let Some(gun) = controlled.controller.and_then(|c| guns.get(c).ok()) {
                ui.label(format!(
                    "Ammo: {} / {}  (R to reload)",
                    gun.rounds, gun.clip_size
                ));
            }
            ui.separator();
            if !manifest.loaded {
                ui.label("Loading weapons…");
                return;
            }
            ui.label("Weapon (mouse wheel to cycle):");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, weapon) in manifest.all.iter().enumerate() {
                    if ui
                        .selectable_label(selection.index == i, weapon.label())
                        .clicked()
                    {
                        selection.index = i;
                        if let Some(controller) = controlled.controller {
                            commands.trigger(EquipWeaponEvent {
                                character: controller,
                                weapon: weapon.clone(),
                            });
                        }
                    }
                }
            });
        });
}

/// Scatter a few non-drivable prop cars (mesh + collider only) over the demo ground.
fn spawn_random_cars(mut commands: Commands, asset_server: Res<AssetServer>) {
    let volume = CAR_SIZE.x * CAR_SIZE.y * CAR_SIZE.z;
    let density = CAR_MASS / volume;

    for _ in 0..6 {
        let x = rand::random::<f32>() * 24.0 - 12.0;
        let z = rand::random::<f32>() * 24.0 - 12.0;
        let pos = Vec3::new(x, 3.0, z);
        let rot = Quat::from_rotation_y(rand::random::<f32>() * std::f32::consts::TAU);
        let car_asset = get_car_asset(get_random_car_type(), &asset_server);

        commands.spawn((
            Name::new("PropCar"),
            Transform::from_translation(pos).with_rotation(rot),
            RigidBody::Dynamic,
            MassPropertiesBundle::from_shape(
                &Cuboid::new(CAR_SIZE.x, CAR_SIZE.y, CAR_SIZE.z),
                density,
            ),
            WorldAssetRoot(car_asset),
            ColliderConstructorHierarchy::new(ColliderConstructor::ConvexDecompositionFromMesh)
                .with_default_layers(CollisionLayers::new(
                    [GamePhysicsLayer::Car],
                    [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
                )),
            CollisionLayers::new(
                [GamePhysicsLayer::Car],
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
            Visibility::default(),
            InheritedVisibility::default(),
        ));
    }
}

/// Auto-spawn a random controllable pedestrian at the origin once the manifest is ready.
fn demo_auto_spawn(
    mut commands: Commands,
    manifest: Res<PedestrianManifest>,
    mut done: Local<bool>,
) {
    if *done || !manifest.loaded {
        return;
    }
    commands.trigger(SpawnControlledPedestrianEvent {
        position: Vec3::new(0.0, 5.0, 0.0),
        url: None,
        scale: None,
        is_exiting_car: false,
        rotation: None,
    });
    *done = true;
}

/// A few dynamic cubes to walk into and shove around.
fn spawn_physics_cubes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let material = materials.add(Color::srgb_u8(124, 144, 255));

    for i in 0..6 {
        let x = rand::random::<f32>() * 12.0 - 6.0;
        let z = rand::random::<f32>() * 12.0 - 6.0;
        commands.spawn((
            Name::new("PhysicsCube"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, 3.0 + i as f32 * 1.5, z),
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0, 1.0),
            // The debug ground only collides with Car/Wheel layers, so cubes must be on Car.
            CollisionLayers::new(
                GamePhysicsLayer::Car,
                [GamePhysicsLayer::Map, GamePhysicsLayer::Car],
            ),
        ));
    }
}
