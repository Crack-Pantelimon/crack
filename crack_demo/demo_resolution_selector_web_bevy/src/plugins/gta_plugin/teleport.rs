use bevy::prelude::*;
use avian3d::prelude::*;
use crate::plugins::map_plugin::{MapTree, MapLODState};
use crate::plugins::gta_plugin::car::Car;
use crate::plugins::gta_plugin::GtaSpawnState;

pub fn teleport_car_system(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    data_res: Res<MapTree>,
    spatial_query: SpatialQuery,
    mut lod_state: ResMut<MapLODState>,
    mut spawn_state: ResMut<GtaSpawnState>,
    mut camera_state: ResMut<super::camera::GtaCameraState>,
    car_query: Query<Entity, With<Car>>,
) {
    if !data_res.parsed {
        return;
    }

    let force_teleport = keyboard.just_pressed(KeyCode::Space);
    let initial_trigger = !spawn_state.initialized;

    if initial_trigger || force_teleport {
        let bbox = data_res.bbox;
        let min_x = bbox.min.x;
        let max_x = bbox.max.x;
        let min_z = bbox.min.z;
        let max_z = bbox.max.z;

        // Cap coordinates to 95% of map bbox
        let center_x = (min_x + max_x) / 2.0;
        let center_z = (min_z + max_z) / 2.0;
        let half_x = ((max_x - min_x) / 2.0) * 0.95;
        let half_z = ((max_z - min_z) / 2.0) * 0.95;
        
        let mut chosen_point = None;
        
        // Try up to 20 times to find a valid raycast hit
        for _ in 0..20 {
            let rx = center_x + (rand::random::<f32>() * 2.0 - 1.0) * half_x;
            let rz = center_z + (rand::random::<f32>() * 2.0 - 1.0) * half_z;
            
            let start_y = bbox.max.y + 1.0;
            let end_y = bbox.min.y - 1.0;
            let origin = Vec3::new(rx, start_y, rz);
            let direction = Dir3::NEG_Y;
            let max_distance = start_y - end_y;
            
            if let Some(hit) = spatial_query.cast_ray(
                origin,
                direction,
                max_distance,
                true,
                &SpatialQueryFilter::default(),
            ) {
                let hit_point = origin + Vec3::NEG_Y * hit.distance;
                chosen_point = Some(hit_point);
                break;
            }
        }
        
        let spawn_point = match chosen_point {
            Some(pt) => pt,
            None => {
                // Safe street fallback: Cora Pantelimon main starting street (translated)
                Vec3::new(-1060.515, -20120.376, -4723.09)
            }
        };

        // 1. Replace the reference point list with ONLY the new spawn point
        lod_state.reference_points = vec![spawn_point];
        info!("Set new map reference point to: {:?}", spawn_point);

        // 2. Despawn any existing car immediately
        for entity in &car_query {
            commands.entity(entity).despawn();
        }

        // 3. Set the 3-second timer and target spawn point
        spawn_state.spawn_point = Some(spawn_point);
        spawn_state.timer = Some(Timer::from_seconds(3.0, TimerMode::Once));
        spawn_state.initialized = true;

        // Reset camera state so it snaps directly to the back of the car on spawn
        camera_state.smoothed_target = None;
        camera_state.yaw = 0.0;

        info!("Spawn timer initiated. Waiting 3.0 seconds for map LOD to load at {:?}", spawn_point);
    }
}
