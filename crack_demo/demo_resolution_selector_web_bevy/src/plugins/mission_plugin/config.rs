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
        Ok(missions) => {
            info!("Loaded {} missions into game engine.", missions.len());
            commands.insert_resource(MissionList { missions });
        }
        Err(e) => {
            error!("Failed to parse missions_config.json: {:?}", e);
            commands.insert_resource(MissionList::default());
        }
    }
}
