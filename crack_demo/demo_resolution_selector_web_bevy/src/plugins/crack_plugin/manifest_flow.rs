use crate::plugins::crack_plugin::{CrackClient, CrackTasks};
use crate::plugins::map_plugin::{MapLODState, MapTree};
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::futures_lite::future;
use game_logic::api::{FetchArgs, FetchMapManifest};

pub fn spawn_manifest_task(
    map_tree: Res<MapTree>,
    mut tasks: ResMut<CrackTasks>,
    client: Res<CrackClient>,
) {
    if !map_tree.parsed && tasks.manifest.is_none() {
        tracing::info!("Spawning manifest task...");
        let api_client = client.0.clone();
        let base_url = crate::config::DATA_BASE_URL.to_string();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            api_client
                .call::<FetchMapManifest>(FetchArgs { base_url })
                .await
        });
        tasks.manifest = Some(task);
    }
}

pub fn poll_manifest_task(
    mut tasks: ResMut<CrackTasks>,
    mut map_tree: ResMut<MapTree>,
    mut lod_state: ResMut<MapLODState>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
) {
    if let Some(mut task) = tasks.manifest.take() {
        if let Some(res) = future::block_on(future::poll_once(&mut task)) {
            match res {
                Ok(manifest) => {
                    tracing::info!("Manifest loaded via RPC successfully!");

                    let middle = (manifest.bbox.min + manifest.bbox.max) / 2.0;
                    let camera_pos = Vec3::new(middle.x, middle.y + 100.0, middle.z);
                    let target = camera_pos + Vec3::new(1.0, -0.2, 1.0);

                    tracing::info!(
                        "Placing camera at center {:?} looking south-east at {:?}",
                        camera_pos,
                        target
                    );
                    for mut cam_transform in &mut camera_query {
                        *cam_transform =
                            Transform::from_translation(camera_pos).looking_at(target, Vec3::Y);
                    }

                    map_tree.bbox = manifest.bbox;
                    map_tree.roots = manifest.roots;
                    map_tree.parsed = true;

                    lod_state.selected_node = None;
                    lod_state.lod_budget = manifest.lod_budget;
                    let timeout = 0.1 + rand::random::<f32>() * 0.1;
                    lod_state.lod_timer = Some(Timer::from_seconds(timeout, TimerMode::Once));
                    lod_state.max_lod = 24;
                    lod_state.min_tiles_per_diagonal = 0.6;
                    lod_state.max_tiles_per_diagonal = 1.30;
                    lod_state.enable_visibility_cull = true;
                    if lod_state.sample_radius_freecam <= 0.0 {
                        lod_state.sample_radius_freecam = 0.1;
                    }
                    if lod_state.sample_radius_car <= 0.0 {
                        lod_state.sample_radius_car = 0.1;
                    }
                    if lod_state.sample_radius_pedestrian <= 0.0 {
                        lod_state.sample_radius_pedestrian = 0.1;
                    }
                }
                Err(e) => {
                    tracing::error!("Manifest RPC error: {e:?}");
                    // Auto-retry happens by leaving task as None
                }
            }
        } else {
            // Re-insert if not ready
            tasks.manifest = Some(task);
        }
    }
}
