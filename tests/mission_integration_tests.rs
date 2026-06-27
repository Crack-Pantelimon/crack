use bevy::prelude::*;
use demo_resolution_selector_web_bevy::plugins::mission_plugin::{
    MissionPlugin,
    state::{MissionState, MissionStatus, HeadlessMode},
    config::{MissionList, Mission},
    trigger::CarMarker,
};

fn setup_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(HeadlessMode);
    app.add_plugins(MissionPlugin);
    app
}

async fn setup_db_sandbox() {
    let mut conn_guard = storage_crackhouse::impl_rusqulite::CONN.lock().await;
    let mock_conn = rusqlite::Connection::open_in_memory().expect("Failed to open mock in-memory DB");
    
    mock_conn.execute(
        "CREATE TABLE IF NOT EXISTS storage_crackhouse_MissionProgress (
            mission_id INTEGER PRIMARY KEY,
            status TEXT NOT NULL,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    ).expect("Failed to create mock schema");
    
    *conn_guard = Ok(mock_conn);
}

#[tokio::test]
async fn test_config_loading() {
    let mut app = setup_test_app();
    app.update(); // Run startup systems
    
    let list = app.world().resource::<MissionList>();
    assert_eq!(list.missions.len(), 42); // We have 42 missions
    
    let first = &list.missions[0];
    assert_eq!(first.id, 1);
    assert_eq!(first.title, "Taximetria pe GPL");
}

#[tokio::test]
async fn test_trigger_and_state_machine_flow() {
    let mut app = setup_test_app();
    app.update(); // Load config
    
    // Check initial state
    {
        let state = app.world().resource::<MissionState>();
        assert!(state.current_mission.is_none());
        assert_eq!(state.active_state, MissionStatus::Available);
    }
    
    // Spawn player at start of Mission 1: [-262.965, 517.524, 1412.29]
    let start_pos = Vec3::new(-262.965, 517.524, 1412.29);
    app.world_mut().spawn((
        CarMarker,
        Transform::from_translation(start_pos),
    ));
    
    app.update(); // Run trigger check system
    
    // Verify Mission 1 started
    {
        let state = app.world().resource::<MissionState>();
        assert_eq!(state.current_mission, Some(1));
        assert_eq!(state.active_state, MissionStatus::Active);
    }
    
    // Move player to destination of Mission 1: [-2411.763, 516.621, 2029.887]
    let end_pos = Vec3::new(-2411.763, 516.621, 2029.887);
    let mut player_query = app.world_mut().query::<&mut Transform>();
    for mut transform in player_query.iter_mut(app.world_mut()) {
        transform.translation = end_pos;
    }
    
    app.update(); // Run trigger check system
    
    // Verify Mission 1 completed and Mission 2 is unlocked
    {
        let state = app.world().resource::<MissionState>();
        assert!(state.current_mission.is_none());
        assert!(state.completed_missions.contains(&1));
    }
}

#[tokio::test]
async fn test_db_persistence() {
    setup_db_sandbox().await;
    
    let mut app = setup_test_app();
    app.update(); // Load config
    
    // Complete Mission 1
    {
        let mut state = app.world_mut().resource_mut::<MissionState>();
        state.completed_missions.insert(1);
    }
    
    app.update(); // Run save system
    
    // Verify DB entry exists
    let res = storage_crackhouse::impl_rusqulite::sql_query(storage_crackhouse::types::SQLAndParams {
        sql: "SELECT * FROM storage_crackhouse_MissionProgress WHERE mission_id = 1".to_string(),
        params: vec![],
    }).await.unwrap();
    assert_eq!(res.rows.len(), 1);
    
    // Restart app and verify progress loaded on startup
    let mut app2 = setup_test_app();
    app2.update(); // Startup triggers database load
    
    let state2 = app2.world().resource::<MissionState>();
    assert!(state2.completed_missions.contains(&1));
}
