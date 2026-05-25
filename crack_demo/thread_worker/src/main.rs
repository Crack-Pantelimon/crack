use std::sync::Arc;

use crack::{api_asscrack::{api::{api_client::ApiClient, api_worker_declarations::{WorkerApiGroup2, WorkerPing}}, crack_worker::{WorkerLoaderFactory, api_worker::make_api_mapping}}, storage_crackhouse::api::{ExecuteSQL, RusquliteTest, StorageCrackhouseApiGroup}};
use crack::native_thread_worker::ThreadWorkerFactory;
use crack::api_asscrack::anyhow;
use crack::native_thread_worker::tokio;
use crack::native_thread_worker::tracing;
use crack::native_thread_worker::dioxus_logger;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use tracing::Level;

    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

        let _f = ThreadWorkerFactory{impl_mapping: make_api_mapping(vec![
            Arc::new(WorkerApiGroup2),
            Arc::new(StorageCrackhouseApiGroup),
        ])}.load_worker().await?;

    let c = ApiClient::new(_f);
    c.call::<WorkerPing>(()).await?;

    c.call::<RusquliteTest>(()).await?;
    c.call::<ExecuteSQL>("SELECT 1 + 1 FROM PERSON".to_string()).await?;
    

    Ok(())
}

