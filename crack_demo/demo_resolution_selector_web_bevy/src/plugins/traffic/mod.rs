use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;

pub mod road_graph;
pub mod spawn;
pub mod driver;
pub mod despawn;
pub mod debug_ui;

#[derive(Resource)]
pub struct TrafficConfig {
    pub enabled: bool,          // default true
    pub spawn_radius: f32,      // slider 50.0..=500.0, default 150.0
    pub max_cars: usize,        // slider 10..=100, default 30
    pub speed_kmh: f32,         // cruise speed target, default 30.0
    pub draw_road_gizmos: bool, // debug polyline rendering
}

impl Default for TrafficConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            spawn_radius: 150.0,
            max_cars: 30,
            speed_kmh: 30.0,
            draw_road_gizmos: false,
        }
    }
}

/// Marker + path state on the car root entity.
#[derive(Component)]
pub struct TrafficCar {
    pub path: Vec<Vec3>,        // full resolved polyline
    pub next_idx: usize,        // next waypoint index
    pub stuck_timer: f32,       // secs below min speed
    pub out_of_view_timer: f32, // secs failing the visibility raycast
    pub half_height: f32,       // cached car half height
}

/// Trigger: spawn one traffic car whose path starts at/near `position`.
#[derive(Event, Clone, Debug)]
pub struct SpawnTrafficCarEvent {
    pub position: Vec3,
}

pub struct TrafficPlugin;

impl Plugin for TrafficPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrafficConfig>()
            .init_resource::<road_graph::TrafficRoadGraph>()
            .add_observer(spawn::spawn_traffic_car_observer)
            .add_systems(
                Update,
                (
                    road_graph::build_road_graph,
                    spawn::traffic_network_spawner,
                    driver::drive_traffic_cars,
                    despawn::despawn_traffic_cars,
                    debug_ui::draw_traffic_gizmos,
                )
                    .chain()
                    .run_if(
                        in_state(crate::plugins::states::OsmDatabaseLoadFinished::OsmFinished)
                            .and_then(in_state(crate::plugins::states::InitialMapLoadFinished::Finished)),
                    ),
            )
            .add_systems(EguiPrimaryContextPass, debug_ui::traffic_debug_ui);
    }
}
