use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

pub mod consts;
pub use consts::*;

pub mod common;
pub mod debug_ui;
pub mod despawn;
pub mod driver;
pub mod pedestrian_traffic;
pub mod road_graph;
pub mod spawn;

#[derive(Default, PartialEq, Clone, Copy, Debug)]
pub enum TrafficDriveMode {
    #[default]
    Normal,
    Reversing(f32), // f32 = remaining reverse secs
}

#[derive(Resource)]
pub struct TrafficConfig {
    pub enabled: bool,          // default true
    pub spawn_radius: f32,      // slider 50.0..=500.0, default 150.0
    pub max_cars: usize,        // slider 10..=100, default 30
    pub speed_kmh: f32,         // cruise speed target, default 30.0
    pub draw_road_gizmos: bool, // debug polyline rendering
    pub ped_enabled: bool,      // default true
    pub max_peds: usize,        // slider 0..=100, default 20
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            spawn_radius: 150.0,
            max_cars: 30,
            speed_kmh: 30.0,
            draw_road_gizmos: false,
            ped_enabled: true,
            max_peds: 20,
        }
    }
}

/// Marker + path state on the car root entity.
#[derive(Component)]
pub struct TrafficCar {
    pub state: common::TrafficAgentState,
    pub half_height: f32,       // cached car half height
    pub mode: TrafficDriveMode, // drive mode (Normal or Reversing)
}

/// Trigger: spawn one traffic car whose path starts at/near `position`.
#[derive(Event, Clone, Debug)]
pub struct SpawnTrafficCarEvent {
    pub position: Vec3,
}

#[derive(Component)]
pub struct TrafficPedestrian {
    pub state: common::TrafficAgentState,
    pub offset_sign: f32, // +1 / -1: which side of the road centre
    pub last_pos: Vec3,   // for stuck check
}

#[derive(Event, Clone, Debug)]
pub struct SpawnTrafficPedestrianEvent {
    pub position: Vec3,
}

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
