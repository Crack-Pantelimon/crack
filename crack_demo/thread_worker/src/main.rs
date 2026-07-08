use crack::api_asscrack::anyhow;
use crack::api_asscrack::api::{api_client::ApiClient, api_worker_declarations::WorkerPing};
use crack::native_thread_worker::dioxus_logger;
use crack::native_thread_worker::tokio;
use crack::native_thread_worker::tracing;
use crack::storage_crackhouse::api::ExecuteSQL2;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use tracing::Level;

    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

    let _f = thread_worker::spawn_in_process_worker().await?;

    let c = ApiClient::new(_f);
    c.call::<WorkerPing>(()).await?;

    // c.call::<RusquliteTest>(()).await?;
    // c.call::<ExecuteSQL>("SELECT 1 + 1 FROM PERSON".to_string())
    // .await?;

    tracing::info!("\n\n====================\n\n WRITE SQL: >>> ");
    while let Ok(sql) = std::io::read_to_string(std::io::stdin()) {
        let ret2 = c.call::<ExecuteSQL2>(sql.clone()).await;

        let ret2 = match ret2 {
            Ok(r) => format!("{:#?}", r),
            Err(e) => format!("{e:#?}"),
        };
        tracing::info!("===========\n\n{ret2}\n\n================\n\n{ret2}\n\n==============");
    }

    Ok(())
}
