use crate::plugins::crack_plugin::{CrackClient, CrackTasks};
use crate::plugins::geojson::{GameLoadingStatus, GeoJsonDatabase};
use crate::plugins::states::OsmDatabaseLoadFinished;
use bevy::prelude::*;
use bevy::tasks::AsyncComputeTaskPool;
use bevy::tasks::futures_lite::future;
use game_logic::api::{FetchArgs, FetchOsmData};

pub fn spawn_osm_task(
    mut loading_status: ResMut<GameLoadingStatus>,
    current_state: Res<State<OsmDatabaseLoadFinished>>,
    mut tasks: ResMut<CrackTasks>,
    client: Res<CrackClient>,
) {
    if current_state.get() == &OsmDatabaseLoadFinished::MapFinished
        && !loading_status.geojson_loading_started
        && tasks.osm.is_none()
    {
        tracing::info!("Spawning OSM task...");
        let api_client = client.0.clone();
        let base_url = crate::config::DATA_BASE_URL.to_string();
        let task = AsyncComputeTaskPool::get().spawn(async move {
            api_client
                .call::<FetchOsmData>(FetchArgs { base_url })
                .await
        });
        tasks.osm = Some(task);
        loading_status.geojson_loading_started = true;
    }
}

pub fn poll_osm_task(
    mut tasks: ResMut<CrackTasks>,
    mut database: ResMut<GeoJsonDatabase>,
    mut loading_status: ResMut<GameLoadingStatus>,
) {
    if let Some(mut task) = tasks.osm.take() {
        if let Some(res) = future::block_on(future::poll_once(&mut task)) {
            match res {
                Ok(data) => {
                    tracing::info!("OSM loaded via RPC successfully!");
                    for (cat, features) in &data.categories {
                        tracing::info!("OSM Category {}: {} features", cat, features.len());
                    }
                    database.categories = data.categories;
                    database.parsed = true;
                }
                Err(e) => {
                    tracing::error!("OSM RPC error: {e:?}");
                    loading_status.geojson_loading_started = false;
                }
            }
        } else {
            // Re-insert if not ready
            tasks.osm = Some(task);
        }
    }
}
