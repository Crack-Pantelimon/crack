use bevy::prelude::*;
use std::collections::HashSet;
use serde::{Deserialize, Serialize};

use crate::plugins::mission_plugin::config::{Mission, MissionList};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MissionStatus {
    #[default]
    Available,
    Active,
    Completed,
    Failed,
}

#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize)]
pub struct MissionState {
    pub current_mission: Option<u32>,
    pub completed_missions: HashSet<u32>,
    pub current_objective_idx: usize,
    pub active_state: MissionStatus,
}

impl MissionState {
    pub fn is_unlocked(&self, mission: &Mission) -> bool {
        mission.prerequisites.iter().all(|prereq| self.completed_missions.contains(prereq))
    }

    pub fn can_start_mission(&self, id: u32, mission_list: &MissionList) -> bool {
        if self.current_mission.is_some() || self.completed_missions.contains(&id) {
            return false;
        }
        if let Some(mission) = mission_list.missions.iter().find(|m| m.id == id) {
            self.is_unlocked(mission)
        } else {
            false
        }
    }

    pub fn start_mission(&mut self, id: u32) {
        self.current_mission = Some(id);
        self.active_state = MissionStatus::Active;
        self.current_objective_idx = 0;
    }

    pub fn complete_current_mission(&mut self) {
        if let Some(id) = self.current_mission {
            self.completed_missions.insert(id);
            self.current_mission = None;
            self.active_state = MissionStatus::Available;
            self.current_objective_idx = 0;
        }
    }
}
