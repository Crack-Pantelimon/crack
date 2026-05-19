mod compute_message;

use api_asscrack::crack_worker::{
    WorkerComputeDyn, WorkerComputeImpl, WorkerLoaderFactory, WorkerMessage, WorkerPipe,
};

// Called when the wasm module is instantiated

pub struct ThreadWorkerFactory;

#[api_asscrack::async_trait::async_trait(?Send)]
impl WorkerLoaderFactory for ThreadWorkerFactory {
    async fn load_worker(&self) -> anyhow::Result<WorkerPipe> {
        let worker_compute = std::sync::Arc::new(ThreadWorkerCompute);
        let t = init_thread(worker_compute).await?;

        Ok(t)
    }
}

pub struct ThreadWorkerCompute;
#[api_asscrack::async_trait::async_trait]
impl WorkerComputeImpl for ThreadWorkerCompute {
    async fn compute_response_message(&self, req: WorkerMessage) -> WorkerMessage {
        crate::compute_message::compute_response_message(req).await
    }
}

async fn init_thread(worker_compute: WorkerComputeDyn) -> anyhow::Result<WorkerPipe> {
    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<WorkerMessage>(1024);
    let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(1024);

    let _t = tokio::task::spawn(async move {
        while let Some(req) = req_rx.recv().await {
            let resp_tx = resp_tx.clone();
            let worker_compute = worker_compute.clone();
            tokio::task::spawn(async move {
                let resp = if &req.msg_type == "ping" {
                    let mut new = req.clone();
                    new.msg_type = "pong".to_string();
                    new
                } else {
                    worker_compute.compute_response_message(req).await
                };
                let m = resp_tx.send(resp).await;
                match m {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("error sending msg: {e:#?}");
                    }
                }
            });
        }
    });

    Ok(WorkerPipe { req_tx, resp_rx })
}
