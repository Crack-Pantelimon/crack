use api_asscrack::crack_worker::WorkerLoaderFactory;
use thread_crackworker::ThreadWorkerFactory;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use tracing::Level;

    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

    let _f = ThreadWorkerFactory.load_worker().await?;

    Ok(())
}

mod test {

    #[tokio::test]
    async fn test_load() -> anyhow::Result<()> {
        use api_asscrack::crack_worker::{WorkerLoaderFactory, WorkerMessage};
        use thread_crackworker::ThreadWorkerFactory;

        use tracing::Level;
        dioxus_logger::init(Level::INFO).expect("logger failed to init");
        tracing::info!("tracing...");

        let mut f = ThreadWorkerFactory.load_worker().await?;

        f.req_tx
            .send(WorkerMessage {
                msg_type: "ping".to_string(),
                msg_content: "abcd".to_string(),
            })
            .await?;
        let t = f.resp_rx.recv().await.unwrap();
        assert!(t.msg_type == "pong");
        assert!(t.msg_content == "abcd");
        Ok(())
    }
}
