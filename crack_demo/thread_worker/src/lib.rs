//! In-process thread worker wiring for the crack demo.
//!
//! Registers worker, storage, and game-logic APIs and spawns a local
//! `ThreadWorkerFactory` pipe for in-process RPC.
use crack::api_asscrack::anyhow;
use crack::api_asscrack::api::api_worker_declarations::WorkerApiGroup2;
use crack::api_asscrack::crack_worker::api_worker::{ApiImplMapping, make_api_mapping};
use crack::api_asscrack::crack_worker::{WorkerLoaderFactory, WorkerPipe};
use crack::native_thread_worker::ThreadWorkerFactory;
use crack::storage_crackhouse::api::StorageCrackhouseApiGroup;
use game_logic::api::GameLogicApiGroup;
use std::sync::Arc;

/// Builds the default API implementation map for this worker.
///
/// Includes worker ping, Crackhouse storage, and game-logic groups.
pub fn make_registered_mapping() -> Arc<ApiImplMapping> {
    make_api_mapping(vec![
        Arc::new(WorkerApiGroup2),
        Arc::new(StorageCrackhouseApiGroup),
        Arc::new(GameLogicApiGroup),
    ])
}

/// Spawns an in-process thread worker and returns its RPC pipe.
///
/// Starts bootstrap networking on a background task before loading.
pub async fn spawn_in_process_worker() -> anyhow::Result<WorkerPipe> {
    tokio::task::spawn(async {
        if let Err(e) = run_bootstrap_if_needed().await {
            tracing::error!("Failed to run bootstrap check: {:?}", e);
        }
    });

    ThreadWorkerFactory {
        impl_mapping: make_registered_mapping(),
    }
    .load_worker()
    .await
}

/// Runs standalone bootstrap networking when no peers are configured.
///
/// Uses game-logic bootstrap topics for the demo network mesh.
pub async fn run_bootstrap_if_needed() -> anyhow::Result<()> {
    crack::net_crackpipe::network_manager::run_standalone_bootstrap_if_needed(
        game_logic::network::bootstrap_topics(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use crack::api_asscrack::api::{api_client::ApiClient, api_worker_declarations::WorkerPing};

    #[tokio::test]
    async fn smoke_spawn_in_process_worker_ping() -> anyhow::Result<()> {
        let pipe = crate::spawn_in_process_worker().await?;
        let c = ApiClient::new(pipe);
        c.call::<WorkerPing>(()).await?;
        Ok(())
    }
}
