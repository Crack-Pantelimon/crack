use std::sync::Arc;

use crack::api_asscrack::{
    api::api_worker_declarations::*, crack_worker::api_worker::make_api_mapping,
};
use crack::storage_crackhouse::api::StorageCrackhouseApiGroup;
use crack::web_serviceworker_worker::{self, spawn_local};
pub use crack::web_serviceworker_worker::{_js_compute_payload_reply, _js_init_dedicated_worker};

use dioxus_logger::tracing;
use tracing::Level;
use web_serviceworker_worker::dioxus_logger;
use web_serviceworker_worker::wasm_bindgen;
use web_serviceworker_worker::wasm_bindgen::prelude::*;
use web_serviceworker_worker::web_worker_registration;

#[wasm_bindgen(start)]
fn init_worker() -> std::result::Result<(), JsValue> {
    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("Web Worker : init_worker()...");

    spawn_local(async move {
        tracing::info!("Web Worker : spawned...");

        tracing::info!("Web Worker : web_worker_registration()...");
        let _r = web_worker_registration(make_api_mapping(vec![
            Arc::new(StorageCrackhouseApiGroup),
            Arc::new(WorkerApiGroup2),
            Arc::new(game_logic::api::GameLogicApiGroup),
        ]))
        .await;
        match _r {
            Err(e) => {
                tracing::error!("web_worker_registration ERROR! {:#?}. WORKER IS DEAD", e);
            }
            _ => {
                tracing::info!(
                    "init_worker / web_worker_registration() finished! WORKER IS RUNNING!!!"
                );
            }
        }
    });

    Ok(())
}
