use bevy::prelude::*;
use crate::plugins::mission_plugin::state::{MissionState, MissionStatus};
use crate::plugins::mission_plugin::config::MissionList;
use crate::plugins::gta_plugin::car::Car;

#[derive(Component, Debug, Default)]
pub struct CarMarker;

pub fn check_mission_triggers(
    car_query: Query<&Transform, Or<(With<Car>, With<CarMarker>)>>,
    mission_list: Res<MissionList>,
    mut mission_state: ResMut<MissionState>,
) {
    let car_transform = match car_query.single() {
        Ok(t) => t,
        Err(_) => return,
    };
    let car_pos = car_transform.translation;

    // Check if we are currently on an active mission
    if let Some(current_id) = mission_state.current_mission {
        if let Some(mission) = mission_list.missions.iter().find(|m| m.id == current_id) {
            let end_pos = Vec3::from(mission.end_coords);
            let dist = car_pos.distance(end_pos);
            if dist < mission.radius {
                info!("Successfully completed mission: {}", mission.title);
                mission_state.complete_current_mission();
            }
        }
    } else {
        // Find if we entered the starting zone of an unlocked mission
        for mission in &mission_list.missions {
            if mission_state.can_start_mission(mission.id, &mission_list) {
                let start_pos = Vec3::from(mission.start_coords);
                let dist = car_pos.distance(start_pos);
                if dist < mission.radius {
                    info!("Started mission: {}", mission.title);
                    mission_state.start_mission(mission.id);
                    break;
                }
            }
        }
    }
}
