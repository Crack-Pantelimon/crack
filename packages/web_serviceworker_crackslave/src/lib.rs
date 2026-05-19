mod compute_message;
mod register_web_worker;

use api_asscrack::crack_worker::{WorkerComputeImpl, WorkerMessage};
use wasm_bindgen::prelude::*;

// Called when the wasm module is instantiated
#[wasm_bindgen(start)]
fn init_worker() -> std::result::Result<(), JsValue> {
    use tracing::Level;

    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

    let worker_compute = std::sync::Arc::new(WebServiceWorkerCompute);
    register_web_worker::do_worker_registration(worker_compute)?;

    Ok(())
}

struct WebServiceWorkerCompute;
#[api_asscrack::async_trait::async_trait]
impl WorkerComputeImpl for WebServiceWorkerCompute {
    async fn compute_response_message(&self, req: WorkerMessage) -> WorkerMessage {
        crate::compute_message::compute_response_message(req).await
    }
}
