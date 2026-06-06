use std::sync::Arc;

use crack::api_asscrack::anyhow;
use crack::native_thread_worker::ThreadWorkerFactory;
use crack::native_thread_worker::dioxus_logger;
use crack::native_thread_worker::tokio;
use crack::native_thread_worker::tracing;
use crack::storage_crackhouse::api::ExecuteSQL2;
use crack::{
    api_asscrack::{
        api::{
            api_client::ApiClient,
            api_worker_declarations::{WorkerApiGroup2, WorkerPing},
        },
        crack_worker::{WorkerLoaderFactory, api_worker::make_api_mapping},
    },
    storage_crackhouse::api::{ExecuteSQL, RusquliteTest, StorageCrackhouseApiGroup},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use tracing::Level;

    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

    let _f = ThreadWorkerFactory {
        impl_mapping: make_api_mapping(vec![
            Arc::new(WorkerApiGroup2),
            Arc::new(StorageCrackhouseApiGroup),
        ]),
    }
    .load_worker()
    .await?;

    let c = ApiClient::new(_f);
    c.call::<WorkerPing>(()).await?;

    // c.call::<RusquliteTest>(()).await?;
    // c.call::<ExecuteSQL>("SELECT 1 + 1 FROM PERSON".to_string())
    // .await?;

    tracing::info!("\n\n====================\n\n WRITE SQL: >>> ");
    while let Ok(sql) = std::io::read_to_string(std::io::stdin()) {
        let ret = c.call::<ExecuteSQL>(sql.clone()).await;
        let ret2 = c.call::<ExecuteSQL2>(sql.clone()).await;

        let ret = match ret {
            Ok(r) => r,
            Err(e) => format!("{e:#?}"),
        };

        let ret2 = match ret2 {
            Ok(r) => format!("{:#?}",r),
            Err(e) => format!("{e:#?}"),
        };
        tracing::info!("===========\n\n{ret}\n\n================\n\n{ret2}\n\n==============");
    }

    Ok(())
}
