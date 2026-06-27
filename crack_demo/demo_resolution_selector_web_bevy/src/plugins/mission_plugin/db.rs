use bevy::prelude::*;
use crate::plugins::mission_plugin::state::MissionState;
use storage_crackhouse::api::execute_sql2;
use storage_crackhouse::types::DbValue;

pub fn load_mission_progress(mut mission_state: ResMut<MissionState>) {
    let init_sql = "CREATE TABLE IF NOT EXISTS storage_crackhouse_MissionProgress (
        mission_id INTEGER PRIMARY KEY,
        status TEXT,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    );";
    
    // Initialize the progress table
    let _ = futures::executor::block_on(execute_sql2(init_sql.to_string()));
    
    // Load completed missions
    let query_sql = "SELECT mission_id FROM storage_crackhouse_MissionProgress WHERE status = 'COMPLETED';";
    if let Ok(result) = futures::executor::block_on(execute_sql2(query_sql.to_string())) {
        for row in result.rows {
            if let Some(col_val) = row.cols.first() {
                if let DbValue::Integer(mission_id) = col_val {
                    mission_state.completed_missions.insert(*mission_id as u32);
                }
            }
        }
        info!("Loaded {} completed missions from database.", mission_state.completed_missions.len());
    }
}

pub fn save_mission_progress_system(
    mission_state: Res<MissionState>,
    mut last_completed: Local<std::collections::HashSet<u32>>,
) {
    if mission_state.completed_missions.len() != last_completed.len() {
        for &id in &mission_state.completed_missions {
            if !last_completed.contains(&id) {
                let sql = format!(
                    "INSERT OR REPLACE INTO storage_crackhouse_MissionProgress (mission_id, status) VALUES ({}, 'COMPLETED');",
                    id
                );
                let _ = futures::executor::block_on(execute_sql2(sql));
                info!("Saved completed mission {} to database.", id);
            }
        }
        *last_completed = mission_state.completed_missions.clone();
    }
}
