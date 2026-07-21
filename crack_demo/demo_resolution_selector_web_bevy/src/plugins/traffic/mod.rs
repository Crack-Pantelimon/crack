use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

/// consts submodule.
pub mod consts;
pub use consts::*;

/// common submodule.
pub mod common;
/// debug ui submodule.
pub mod debug_ui;
/// despawn submodule.
pub mod despawn;
/// driver submodule.
pub mod driver;
/// pedestrian traffic submodule.
pub mod pedestrian_traffic;
/// road graph submodule.
pub mod road_graph;
/// spawn submodule.
pub mod spawn;

/// traffic drive mode.
#[derive(Default, PartialEq, Clone, Copy, Debug)]
pub enum TrafficDriveMode {
/// normal variant.
    #[default]
    Normal,
/// Documented public item.
    Reversing(f32), // f32 = remaining reverse secs
}

/// traffic config.
#[derive(Resource)]
pub struct TrafficConfig {
/// enabled field.
    pub enabled: bool,          // default false
/// spawn radius field.
    pub spawn_radius: f32,      // slider 50.0..=500.0, default 150.0
/// max cars field.
    pub max_cars: usize,        // slider 10..=100, default 30
/// speed kmh field.
    pub speed_kmh: f32,         // cruise speed target, default 30.0
/// draw road gizmos field.
    pub draw_road_gizmos: bool, // debug polyline rendering
/// ped enabled field.
    pub ped_enabled: bool,      // default false
/// max peds field.
    pub max_peds: usize,        // slider 0..=100, default 20
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            spawn_radius: 150.0,
            max_cars: 30,
            speed_kmh: 30.0,
            draw_road_gizmos: false,
            ped_enabled: false,
            max_peds: 20,
        }
    }
}

/// Marker + path state on the car root entity.
#[derive(Component)]
pub struct TrafficCar {
/// state field.
    pub state: common::TrafficAgentState,
/// half height field.
    pub half_height: f32,       // cached car half height
/// mode field.
    pub mode: TrafficDriveMode, // drive mode (Normal or Reversing)
}

/// Trigger: spawn one traffic car whose path starts at/near `position`.
#[derive(Event, Clone, Debug)]
pub struct SpawnTrafficCarEvent {
/// position field.
    pub position: Vec3,
}

/// traffic pedestrian.
#[derive(Component)]
pub struct TrafficPedestrian {
/// state field.
    pub state: common::TrafficAgentState,
/// offset sign field.
    pub offset_sign: f32, // +1 / -1: which side of the road centre
/// last pos field.
    pub last_pos: Vec3,   // for stuck check
}

/// spawn traffic pedestrian event.
#[derive(Event, Clone, Debug)]
pub struct SpawnTrafficPedestrianEvent {
/// position field.
    pub position: Vec3,
}

/// traffic plugin.
pub struct TrafficPlugin;

impl Plugin for TrafficPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrafficConfig>()
            .init_resource::<road_graph::TrafficRoadGraph>()
            .init_resource::<pedestrian_traffic::PendingTrafficPeds>()
            .add_observer(spawn::spawn_traffic_car_observer)
            .add_observer(pedestrian_traffic::spawn_traffic_pedestrian_observer)
            .add_systems(
                Update,
                (
                    road_graph::build_road_graph,
                    spawn::traffic_network_spawner,
                    driver::drive_traffic_cars,
                    despawn::despawn_traffic_cars,
                    pedestrian_traffic::traffic_pedestrian_spawner,
                    pedestrian_traffic::adopt_traffic_pedestrians,
                    pedestrian_traffic::despawn_traffic_pedestrians,
                    debug_ui::draw_traffic_gizmos,
                )
                    .chain()
                    .run_if(
                        in_state(crate::plugins::states::OsmDatabaseLoadFinished::OsmFinished)
                            .and_then(in_state(
                                crate::plugins::states::InitialMapLoadFinished::Finished,
                            )),
                    ),
            )
            .add_systems(
                Update,
                pedestrian_traffic::drive_traffic_pedestrians
                    .after(crate::plugins::pedestrian_ai::movement_ai::ai_movement)
                    .run_if(
                        in_state(crate::plugins::states::OsmDatabaseLoadFinished::OsmFinished)
                            .and_then(in_state(
                                crate::plugins::states::InitialMapLoadFinished::Finished,
                            )),
                    ),
            )
            .add_systems(EguiPrimaryContextPass, debug_ui::traffic_debug_ui);
    }
}
