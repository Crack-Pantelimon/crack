//! Click-to-inspect debug window for free-camera mode.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::plugins::cars_driving::driving_plugin::spawn_car::{
    ActivePlayerVehicle, Car, CarHealth, DisabledCar,
};
use crate::plugins::map_plugin::map_lod::TreeMapTile;
use crate::plugins::pedestrian_ai::AiPedestrian;
use crate::plugins::pedestrian_ai::faction::Faction;
use crate::plugins::pedestrians::pedestrian_controller_plugin::{
    CharacterController, DriverMesh, MainCamera, PlayerDriven,
};
use crate::plugins::pedestrians::skeleton::PedestrianSkeleton;
use crate::plugins::pedestrians::spawn_pedestrian::ModelRoot;
use crate::plugins::states::GameControlState;
use crate::plugins::traffic::TrafficCar;
use crate::plugins::weapons::weapon_attach::{EquippedWeapon, WeaponKind, WeaponModel};
use crate::plugins::weapons::weapon_manifest::WeaponId;

/// debug picker plugin.
pub struct DebugPickerPlugin;

impl Plugin for DebugPickerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugPickerState>()
            .add_systems(
                Update,
                debug_picker_pick_system.run_if(in_state(GameControlState::MapFreecam)),
            )
            .add_systems(EguiPrimaryContextPass, debug_picker_ui);
    }
}

/// debug picker state.
#[derive(Resource, Default)]
pub struct DebugPickerState {
/// show window field.
    pub show_window: bool,
/// last pick field.
    pub last_pick: Option<PickResult>,
}

/// pick result.
pub struct PickResult {
/// hit point field.
    pub hit_point: Vec3,
/// distance field.
    pub distance: f32,
/// entity field.
    pub entity: Entity,
/// kind field.
    pub kind: PickKind,
}

/// pick kind.
pub enum PickKind {
/// car variant.
    Car {
/// Documented public item.
        car_type: String,
/// Documented public item.
        player_driven: bool,
/// Documented public item.
        ai_driven: bool,
/// Documented public item.
        disabled: bool,
/// Documented public item.
        health: Option<CarHealth>,
/// Documented public item.
        has_driver: bool,
    },
/// pedestrian variant.
    Pedestrian {
/// Documented public item.
        player_controlled: bool,
/// Documented public item.
        ai_controlled: bool,
/// Documented public item.
        faction: Option<Faction>,
/// Documented public item.
        weapon: Option<WeaponId>,
    },
/// weapon variant.
    Weapon {
/// Documented public item.
        kind: Option<WeaponKind>,
/// Documented public item.
        weapon_id: Option<WeaponId>,
    },
/// ground variant.
    Ground {
/// Documented public item.
        octant_path: String,
/// Documented public item.
        depth: usize,
/// Documented public item.
        asset_id: String,
/// Documented public item.
        bbox: game_logic::map::BBox,
    },
/// unknown variant.
    Unknown,
}

#[derive(SystemParam)]
struct WorldClassifyQueries<'w, 's> {
    parents: Query<'w, 's, &'static ChildOf>,
    tile: Query<'w, 's, &'static TreeMapTile>,
    car: Query<'w, 's, &'static Car>,
    active_player: Query<'w, 's, (), With<ActivePlayerVehicle>>,
    traffic: Query<'w, 's, (), With<TrafficCar>>,
    disabled: Query<'w, 's, (), With<DisabledCar>>,
    car_health: Query<'w, 's, &'static CarHealth>,
    drivers: Query<'w, 's, &'static DriverMesh>,
}

#[derive(SystemParam)]
struct ActorClassifyQueries<'w, 's> {
    controller: Query<'w, 's, (), With<CharacterController>>,
    model: Query<'w, 's, (), With<ModelRoot>>,
    skel: Query<'w, 's, (), With<PedestrianSkeleton>>,
    player_driven: Query<'w, 's, (), With<PlayerDriven>>,
    ai_ped: Query<'w, 's, (), With<AiPedestrian>>,
    faction: Query<'w, 's, &'static Faction>,
    equipped: Query<'w, 's, &'static EquippedWeapon>,
    weapon_model: Query<'w, 's, (), With<WeaponModel>>,
    weapon_kind: Query<'w, 's, &'static WeaponKind>,
}

#[derive(SystemParam)]
struct ClassifyQueries<'w, 's> {
    world: WorldClassifyQueries<'w, 's>,
    actors: ActorClassifyQueries<'w, 's>,
}

fn debug_picker_pick_system(
    mut state: ResMut<DebugPickerState>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    window_query: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    spatial_query: SpatialQuery,
    mut contexts: EguiContexts,
    classify: ClassifyQueries,
) {
    if !state.show_window {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if ctx.egui_wants_pointer_input() || ctx.is_pointer_over_egui() {
        return;
    }

    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
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

    let Some(hit) = spatial_query.cast_ray(
        ray.origin,
        ray.direction,
        10000.0,
        true,
        &SpatialQueryFilter::default(),
    ) else {
        return;
    };

    let hit_point = ray.origin + *ray.direction * hit.distance;
    let (entity, kind) = classify_hit(hit.entity, &classify);

    state.last_pick = Some(PickResult {
        hit_point,
        distance: hit.distance,
        entity,
        kind,
    });
}

fn classify_hit(hit_entity: Entity, q: &ClassifyQueries) -> (Entity, PickKind) {
    let w = &q.world;
    let a = &q.actors;
    let mut cur = hit_entity;
    loop {
        if let Ok(tile) = w.tile.get(cur) {
            return (
                cur,
                PickKind::Ground {
                    octant_path: tile.node_path.0.clone(),
                    depth: tile.node_path.0.len(),
                    asset_id: tile.asset_id.0.clone(),
                    bbox: tile.bbox,
                },
            );
        }

        if let Ok(car) = w.car.get(cur) {
            let has_driver = w.drivers.iter().any(|d| d.car == cur);
            return (
                cur,
                PickKind::Car {
                    car_type: car._car_type.clone(),
                    player_driven: w.active_player.contains(cur),
                    ai_driven: w.traffic.contains(cur),
                    disabled: w.disabled.contains(cur),
                    health: w.car_health.get(cur).ok().copied(),
                    has_driver,
                },
            );
        }

        if a.controller.contains(cur)
            || a.model.contains(cur)
            || a.skel.contains(cur)
            || w.drivers.get(cur).is_ok()
        {
            let controller = find_controller(cur, &w.parents, &a.controller);
            let subject = controller.unwrap_or(cur);
            return (
                subject,
                PickKind::Pedestrian {
                    player_controlled: a.player_driven.contains(subject),
                    ai_controlled: a.ai_ped.contains(subject),
                    faction: a.faction.get(subject).ok().copied(),
                    weapon: a.equipped.get(subject).ok().map(|w| w.0.clone()),
                },
            );
        }

        if a.weapon_model.contains(cur) || a.weapon_kind.get(cur).is_ok() {
            let kind = a.weapon_kind.get(cur).ok().copied();
            let controller = find_controller(cur, &w.parents, &a.controller);
            let weapon_id = controller.and_then(|e| a.equipped.get(e).ok().map(|w| w.0.clone()));
            return (cur, PickKind::Weapon { kind, weapon_id });
        }

        match w.parents.get(cur) {
            Ok(child_of) => cur = child_of.parent(),
            Err(_) => break,
        }
    }

    (hit_entity, PickKind::Unknown)
}

fn find_controller(
    start: Entity,
    parents: &Query<&ChildOf>,
    q_controller: &Query<(), With<CharacterController>>,
) -> Option<Entity> {
    let mut cur = start;
    loop {
        if q_controller.contains(cur) {
            return Some(cur);
        }
        match parents.get(cur) {
            Ok(child_of) => cur = child_of.parent(),
            Err(_) => return None,
        }
    }
}

fn debug_picker_ui(mut contexts: EguiContexts, mut state: ResMut<DebugPickerState>) {
    if !state.show_window {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let mut open = state.show_window;
    egui::Window::new("Debug Picker")
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("Left-click an object to inspect");
            ui.separator();

            match &state.last_pick {
                None => {
                    ui.label("Nothing picked yet");
                }
                Some(pick) => {
                    ui.label(format!("Entity: {:?}", pick.entity));
                    ui.label(format!(
                        "Hit: ({:.2}, {:.2}, {:.2})  dist {:.1}m",
                        pick.hit_point.x, pick.hit_point.y, pick.hit_point.z, pick.distance
                    ));
                    ui.separator();
                    render_pick_kind(ui, &pick.kind);
                }
            }
        });

    state.show_window = open;
}

fn render_pick_kind(ui: &mut egui::Ui, kind: &PickKind) {
    match kind {
        PickKind::Car {
            car_type,
            player_driven,
            ai_driven,
            disabled,
            health,
            has_driver,
        } => {
            ui.label("Kind: Car");
            ui.label(format!("Type: {car_type}"));
            let drive = if *player_driven {
                "player-driven"
            } else if *ai_driven {
                "AI (traffic)"
            } else {
                "unassigned"
            };
            ui.label(format!("Drive: {drive}"));
            ui.label(format!("Has driver: {has_driver}"));
            ui.label(format!("Disabled: {disabled}"));
            if let Some(h) = health {
                ui.label(format!("Health: {:.0} / {:.0}", h.current, h.max));
            } else {
                ui.label("Health: (none)");
            }
        }
        PickKind::Pedestrian {
            player_controlled,
            ai_controlled,
            faction,
            weapon,
        } => {
            ui.label("Kind: Pedestrian");
            let control = if *player_controlled {
                "player-controlled"
            } else if *ai_controlled {
                "AI"
            } else {
                "unknown"
            };
            ui.label(format!("Control: {control}"));
            if let Some(f) = faction {
                ui.label(format!("Faction: {}", f.label()));
            } else {
                ui.label("Faction: (none)");
            }
            if let Some(w) = weapon {
                ui.label(format!("Weapon: {}", w.label()));
            } else {
                ui.label("Weapon: (none)");
            }
        }
        PickKind::Weapon { kind, weapon_id } => {
            ui.label("Kind: Weapon");
            if let Some(k) = kind {
                ui.label(format!("Class: {k:?}"));
            } else {
                ui.label("Class: (unknown)");
            }
            if let Some(w) = weapon_id {
                ui.label(format!("Type: {}", w.label()));
            } else {
                ui.label("Type: (unknown)");
            }
        }
        PickKind::Ground {
            octant_path,
            depth,
            asset_id,
            bbox,
        } => {
            ui.label("Kind: Ground (octree tile)");
            ui.label(format!("Octant path: {octant_path}"));
            ui.label(format!("Depth: {depth}"));
            ui.label(format!("Asset: {asset_id}"));
            ui.label(format!(
                "BBox: min ({:.1}, {:.1}, {:.1})  max ({:.1}, {:.1}, {:.1})",
                bbox.min.x, bbox.min.y, bbox.min.z, bbox.max.x, bbox.max.y, bbox.max.z
            ));
        }
        PickKind::Unknown => {
            ui.label("Kind: Unknown");
        }
    }
}
