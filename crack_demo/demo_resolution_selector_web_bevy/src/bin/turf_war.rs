//! Turf War: a test arena where 4 AI factions fight each other and the player.
//!
//! Drops ~5 peds per faction into a debug arena full of obstacles, sets them all at war,
//! and lets them fight. A player-controlled pedestrian (faction Red) is spawned into the fray;
//! the other three factions hunt it, its HP shows as green `HP: xx` in the bottom-right corner,
//! and it respawns a few seconds after it dies. Dying peds (AI or player) play a death clip and
//! then despawn. AI state transitions, shots, hits, and deaths are logged to the console.
//!
//! Run: `cargo run --bin turf_war`

use avian3d::prelude::*;
use bevy::ecs::query::Has;
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::{
        audio::GameAudioPlugin,
        cars_driving::{
            car_info::{get_car_asset, get_random_car_type},
            driving_plugin::GamePhysicsLayer,
        },
        pedestrian_ai::{
            DEFAULT_HP, Faction, Health, PedestrianAiPlugin, SpawnAiPedestrianEvent, WarMatrix,
        },
        pedestrians::{
            PedestrianManifest, PedestriansPlugin,
            pedestrian_controller_plugin::{
                CharacterController, ControlledCharacter, PedestrianControllerPlugin,
                SpawnControlledPedestrianEvent,
            },
        },
        states::GameControlState,
        weapons::{WeaponManifest, WeaponsPlugin},
    },
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

/// Approximate car body extents for mass calculation.
const CAR_SIZE: Vec3 = Vec3::new(1.8, 1.0, 3.04);
const CAR_MASS: f32 = 1200.0;

/// Number of peds per faction.
const PEDS_PER_FACTION: usize = 5;

/// The player pedestrian fights for this faction; the other three combatant factions hunt it.
const PLAYER_FACTION: Faction = Faction::Red;
/// Where the player (re)spawns each life.
const PLAYER_SPAWN: Vec3 = Vec3::new(0.0, 2.0, 20.0);

fn main() {
    make_basic_app("Turf War")
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .init_state::<GameControlState>()
        .add_plugins(PedestriansPlugin)
        .add_plugins(PedestrianControllerPlugin)
        .add_plugins(WeaponsPlugin)
        .add_plugins(GameAudioPlugin)
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(PedestrianAiPlugin)
        .insert_resource(WarMatrix::all_out_war())
        .init_resource::<PlayerSpawner>()
        .add_systems(Startup, spawn_obstacles)
        .add_systems(PostStartup, setup_overhead_camera)
        .add_systems(Update, (spawn_factions_once, manage_player_pedestrian))
        .add_systems(EguiPrimaryContextPass, player_hp_ui)
        .run();
}

/// Debounce state for respawning the player pedestrian.
#[derive(Resource, Default)]
struct PlayerSpawner {
    /// Seconds to wait after triggering a spawn before checking liveness again (the spawn observer
    /// and model adopt are deferred, so the controller does not exist the same frame).
    respawn_timer: f32,
}

/// Keeps exactly one player-controlled pedestrian alive in the arena: spawns it, tags it as a
/// combatant (so the AI fights and can kill it), and respawns a fresh one after it dies.
fn manage_player_pedestrian(
    mut commands: Commands,
    time: Res<Time>,
    ped_manifest: Res<PedestrianManifest>,
    weapon_manifest: Res<WeaponManifest>,
    mut controlled: ResMut<ControlledCharacter>,
    // For the controlled controller entity: `Ok(true)` = alive & already a combatant,
    // `Ok(false)` = alive but not yet a combatant, `Err` = despawned / never spawned.
    q_controller: Query<Has<Health>, With<CharacterController>>,
    mut spawner: ResMut<PlayerSpawner>,
) {
    if !ped_manifest.loaded || !weapon_manifest.loaded {
        return;
    }
    if spawner.respawn_timer > 0.0 {
        spawner.respawn_timer -= time.delta_secs();
        return;
    }

    match controlled.controller.and_then(|c| q_controller.get(c).ok()) {
        // Gone (died and despawned) or never spawned — (re)spawn a player pedestrian.
        None => {
            // Clear the stale (despawned) reference so the spawn observer does not try to
            // despawn a corpse that `tick_dying` already removed.
            controlled.controller = None;
            controlled.ped = None;
            controlled.scale_node = None;
            controlled.awaiting = false;
            commands.trigger(SpawnControlledPedestrianEvent {
                position: PLAYER_SPAWN,
                url: None,
                scale: None,
                is_exiting_car: false,
                rotation: None,
                health: None,
            });
            spawner.respawn_timer = 1.0;
        }
        // Alive but not yet a combatant — give it health + a faction so the AI engages it.
        Some(false) => {
            if let Some(controller) = controlled.controller {
                commands
                    .entity(controller)
                    .insert((Health::full(DEFAULT_HP), PLAYER_FACTION));
            }
        }
        // Alive and already fighting — nothing to do.
        Some(true) => {}
    }
}

/// Draws the player's HP in the bottom-right corner as green `HP: xx` while controlling a ped.
fn player_hp_ui(
    mut contexts: EguiContexts,
    controlled: Res<ControlledCharacter>,
    healths: Query<&Health, With<CharacterController>>,
) {
    let Some(controller) = controlled.controller else {
        return;
    };
    let Ok(health) = healths.get(controller) else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Area::new(egui::Id::new("player_hp_area"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("HP: {:.0}", health.current.max(0.0)))
                    .color(egui::Color32::from_rgb(0, 255, 0))
                    .size(32.0)
                    .strong(),
            );
        });
}

/// Override the debug scene camera to look down on the arena.
fn setup_overhead_camera(mut camera_query: Query<&mut Transform, With<Camera3d>>) {
    for mut transform in &mut camera_query {
        *transform = Transform::from_xyz(0.0, 55.0, 40.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
}

/// Scatter cover obstacles across the arena.
fn spawn_obstacles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Static cubes as cover walls.
    let colors = [
        Color::srgb(0.5, 0.5, 0.6),
        Color::srgb(0.4, 0.45, 0.5),
        Color::srgb(0.55, 0.5, 0.45),
    ];

    let cube_configs: Vec<(Vec3, Vec3)> = vec![
        // (position, half-extents)
        (Vec3::new(5.0, 1.0, 0.0), Vec3::new(3.0, 2.0, 0.5)),
        (Vec3::new(-5.0, 1.0, 0.0), Vec3::new(3.0, 2.0, 0.5)),
        (Vec3::new(0.0, 1.0, 5.0), Vec3::new(0.5, 2.0, 3.0)),
        (Vec3::new(0.0, 1.0, -5.0), Vec3::new(0.5, 2.0, 3.0)),
        // Low walls for climbing
        (Vec3::new(8.0, 0.5, 8.0), Vec3::new(2.0, 1.0, 0.3)),
        (Vec3::new(-8.0, 0.5, -8.0), Vec3::new(2.0, 1.0, 0.3)),
        (Vec3::new(-8.0, 0.5, 8.0), Vec3::new(0.3, 1.0, 2.0)),
        (Vec3::new(8.0, 0.5, -8.0), Vec3::new(0.3, 1.0, 2.0)),
        // Central column
        (Vec3::new(0.0, 1.5, 0.0), Vec3::new(1.0, 3.0, 1.0)),
        // Scattered small cubes
        (Vec3::new(3.0, 0.75, 3.0), Vec3::new(0.8, 1.5, 0.8)),
        (Vec3::new(-3.0, 0.75, -3.0), Vec3::new(0.8, 1.5, 0.8)),
        (Vec3::new(-3.0, 0.75, 3.0), Vec3::new(0.8, 1.5, 0.8)),
        (Vec3::new(3.0, 0.75, -3.0), Vec3::new(0.8, 1.5, 0.8)),
    ];

    for (i, (pos, half)) in cube_configs.iter().enumerate() {
        let size = *half * 2.0;
        let mesh = meshes.add(Cuboid::new(size.x, size.y, size.z));
        let material = materials.add(colors[i % colors.len()]);

        commands.spawn((
            Name::new("CoverWall"),
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(*pos),
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            CollisionLayers::new(
                [GamePhysicsLayer::Map],
                [GamePhysicsLayer::Car, GamePhysicsLayer::Wheel],
            ),
        ));
    }

    // A few static prop cars.
    let volume = CAR_SIZE.x * CAR_SIZE.y * CAR_SIZE.z;
    let density = CAR_MASS / volume;
    let car_positions = [
        Vec3::new(12.0, 1.5, 0.0),
        Vec3::new(-12.0, 1.5, 0.0),
        Vec3::new(0.0, 1.5, 12.0),
    ];

    for pos in car_positions {
        let rot = Quat::from_rotation_y(rand::random::<f32>() * std::f32::consts::TAU);
        let car_asset = get_car_asset(get_random_car_type(), &asset_server);

        commands.spawn((
            Name::new("PropCar"),
            Transform::from_translation(pos).with_rotation(rot),
            RigidBody::Static,
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

/// Once both manifests are loaded, spawn the 4 faction clusters.
fn spawn_factions_once(
    mut commands: Commands,
    ped_manifest: Res<PedestrianManifest>,
    weapon_manifest: Res<WeaponManifest>,
    mut done: Local<bool>,
) {
    if *done || !ped_manifest.loaded || !weapon_manifest.loaded {
        return;
    }
    *done = true;

    // 4 corners of the arena.
    let corners = [
        Vec3::new(15.0, 2.0, 15.0),
        Vec3::new(-15.0, 2.0, 15.0),
        Vec3::new(15.0, 2.0, -15.0),
        Vec3::new(-15.0, 2.0, -15.0),
    ];

    let total = Faction::COMBATANTS.len() * PEDS_PER_FACTION;
    info!(
        "Turf war: spawning {} peds across {} factions",
        total,
        Faction::COMBATANTS.len()
    );

    for (i, &faction) in Faction::COMBATANTS.iter().enumerate() {
        let base = corners[i];
        for j in 0..PEDS_PER_FACTION {
            // Spread within the corner.
            let offset = Vec3::new(
                (j as f32 % 3.0) * 1.5 - 1.5,
                0.0,
                (j as f32 / 3.0).floor() * 1.5 - 0.75,
            );
            commands.trigger(SpawnAiPedestrianEvent {
                position: base + offset,
                faction,
                url: None,
                weapon: None,
            });
        }
    }
}
