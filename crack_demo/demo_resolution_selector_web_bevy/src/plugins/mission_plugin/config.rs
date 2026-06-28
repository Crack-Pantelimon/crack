use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Mission {
    pub id: u32,
    pub title: String,
    pub client: String,
    pub prerequisites: Vec<u32>,
    pub start_coords: [f32; 3],
    pub end_coords: [f32; 3],
    pub radius: f32,
    pub objectives: Vec<String>,
    pub dialogues: Vec<String>,
    pub reward_cash: u32,
    pub reward_respect: u32,
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissionList {
    pub missions: Vec<Mission>,
}

pub fn load_missions_config(mut commands: Commands) {
    let config_str = include_str!("../../../public/missions_config.json");
    match serde_json::from_str::<Vec<Mission>>(config_str) {
        Ok(mut missions) => {
            // Apply coordinate transformation to all missions to match the 3d_data_v2 map offset
            for m in &mut missions {
                // X (East) shift: -797.55
                // Y (Height) shift: -20637.90
                // Z (North) shift: negated and shifted by -3310.80 to map to Bevy -Z coordinate
                m.start_coords[0] -= 797.55;
                m.start_coords[1] -= 20637.90;
                m.start_coords[2] = -m.start_coords[2] - 3310.80;

                m.end_coords[0] -= 797.55;
                m.end_coords[1] -= 20637.90;
                m.end_coords[2] = -m.end_coords[2] - 3310.80;
            }
            info!("Loaded {} missions into game engine.", missions.len());
            commands.insert_resource(MissionList { missions });
        }
        Err(e) => {
            error!("Failed to parse missions_config.json: {:?}", e);
            commands.insert_resource(MissionList::default());
        }
    }
}
