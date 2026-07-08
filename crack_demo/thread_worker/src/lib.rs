use crack::api_asscrack::anyhow;
use crack::api_asscrack::api::api_worker_declarations::WorkerApiGroup2;
use crack::api_asscrack::crack_worker::api_worker::{ApiImplMapping, make_api_mapping};
use crack::api_asscrack::crack_worker::{WorkerLoaderFactory, WorkerPipe};
use crack::native_thread_worker::ThreadWorkerFactory;
use crack::storage_crackhouse::api::StorageCrackhouseApiGroup;
use game_logic::api::GameLogicApiGroup;
use std::sync::Arc;

pub fn make_registered_mapping() -> Arc<ApiImplMapping> {
    make_api_mapping(vec![
        Arc::new(WorkerApiGroup2),
        Arc::new(StorageCrackhouseApiGroup),
        Arc::new(GameLogicApiGroup),
    ])
}

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

pub async fn run_bootstrap_if_needed() -> anyhow::Result<()> {
    crack::net_crackpipe::network_manager::run_standalone_bootstrap_if_needed(
        game_logic::network::bootstrap_topics(),
    )
    .await
}
