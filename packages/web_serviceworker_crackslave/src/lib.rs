mod register_worker;
mod compute_message;

use wasm_bindgen::prelude::*;

// Called when the wasm module is instantiated
#[wasm_bindgen(start)]
fn init_worker() -> std::result::Result<(), JsValue> {

    use tracing::Level;

    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

    register_worker::do_worker_registration()
}



#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct WorkerMessage {
    msg_type: String,
    msg_content: String,
}
