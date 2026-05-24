
use std::sync::Arc;

use crack::api_asscrack::{api::api_worker_declarations::*, crack_worker::api_worker::make_api_mapping};
use crack::web_serviceworker_worker;

use web_serviceworker_worker::wasm_bindgen::prelude::*;
use web_serviceworker_worker::wasm_bindgen;
use web_serviceworker_worker::dioxus_logger;
use web_serviceworker_worker::do_worker_registration;
use dioxus_logger::tracing;
use tracing::Level;

#[wasm_bindgen(start)]
fn init_worker() -> std::result::Result<(), JsValue> {
    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

    do_worker_registration(make_api_mapping(vec![
        Arc::new(WorkerApiGroup2),
    ]))?;

    tracing::info!("init_worker() finished! WORKER IS RUNNING!!!");

    Ok(())
}
