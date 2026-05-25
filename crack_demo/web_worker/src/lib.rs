
use std::sync::Arc;

use crack::api_asscrack::{api::api_worker_declarations::*, crack_worker::api_worker::make_api_mapping};
use crack::storage_crackhouse::api::StorageCrackhouseApiGroup;
use crack::web_serviceworker_worker::{self, spawn_local};

use web_serviceworker_worker::wasm_bindgen::prelude::*;
use web_serviceworker_worker::wasm_bindgen;
use web_serviceworker_worker::dioxus_logger;
use web_serviceworker_worker::web_worker_registration;
use dioxus_logger::tracing;
use tracing::Level;


#[wasm_bindgen(start)]
fn init_worker() -> std::result::Result<(), JsValue> {
    dioxus_logger::init(Level::INFO).expect("logger failed to init");
    tracing::info!("tracing...");

    spawn_local(async move {

        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
        crack::storage_crackhouse::install_relaxed_idb().await;
        #[cfg(all(target_family = "wasm", target_os = "unknown"))]
        crack::storage_crackhouse::install_opfs_sahpool().await;
        

        let _r = web_worker_registration(make_api_mapping(vec![
            Arc::new(StorageCrackhouseApiGroup),
            Arc::new(WorkerApiGroup2),
        ]));
        match _r {
            Err(e) => {tracing::error!("worker registration ERROR! {:#?}. WORKER IS DEAD", e);}
            _=>{tracing::info!("init_worker() finished! WORKER IS RUNNING!!!");}
        }
    });


    Ok(())
}
