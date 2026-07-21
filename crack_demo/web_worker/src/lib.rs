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
        if let Err(e) = run_bootstrap_if_needed().await {
            tracing::error!("Failed to run bootstrap check: {:?}", e);
        }
    });

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

pub async fn run_bootstrap_if_needed() -> anyhow::Result<()> {
    crack::net_crackpipe::network_manager::run_standalone_bootstrap_if_needed(
        game_logic::network::bootstrap_topics(),
    )
    .await
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crack::api_asscrack::crack_worker::{WorkerMessage, api_worker::compute_response_message};
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    // Same group composition as `init_worker()`; `make_api_mapping` panics on
    // duplicate or unimplemented declarations, so building it is itself the
    // assertion. No real Worker scope is required.
    fn worker_mapping() -> Arc<crack::api_asscrack::crack_worker::api_worker::ApiImplMapping> {
        make_api_mapping(vec![
            Arc::new(StorageCrackhouseApiGroup),
            Arc::new(WorkerApiGroup2),
            Arc::new(game_logic::api::GameLogicApiGroup),
        ])
    }

    #[wasm_bindgen_test]
    fn smoke_make_api_mapping() {
        let _mapping = worker_mapping();
    }

    #[wasm_bindgen_test]
    async fn smoke_compute_response_unknown_method() {
        let mapping = worker_mapping();
        let resp = compute_response_message(
            WorkerMessage {
                msg_id: 7,
                msg_type: "no_such_method".to_string(),
                msg_content: vec![],
            },
            mapping,
        )
        .await;
        assert_eq!(resp.msg_id, 7);
        assert!(
            resp.msg_type.contains("no_such_method"),
            "expected missing-key error, got: {}",
            resp.msg_type
        );
    }
}
