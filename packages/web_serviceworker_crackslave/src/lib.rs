pub extern crate wasm_bindgen;
pub extern crate dioxus_logger;


use std::sync::Arc;

use api_asscrack::{_crack_utils::sleep_ms, crack_worker::WorkerMessage};
use api_asscrack::crack_worker::api_worker::ApiImplMapping;
use wasm_bindgen::JsValue;
pub use wasm_bindgen_futures::spawn_local;


use wasm_bindgen::prelude::*;
use web_sys::{DedicatedWorkerGlobalScope, console};

pub fn web_worker_registration(mapping: Arc<ApiImplMapping>) -> std::result::Result<(), JsValue> {
    let global = js_sys::global();
    tracing::info!("global!! {:#?}", &global);

    if let Ok(true) = js_sys::Reflect::has(&global, &JsValue::from_str("DedicatedWorkerGlobalScope"))
    {
        console::log_1(&JsValue::from_str("in dedicated worker V4"));
        // cast the global to a DedicatedWorkerGlobalScope
        let global: DedicatedWorkerGlobalScope = global.unchecked_into::<DedicatedWorkerGlobalScope>();

        let version = get_version(global.clone()).unwrap_or_default();
        tracing::info!("version  =  '{}'", &version);

        let on_error = on_error(&global, version.clone())?;
        global.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        // Register the message handler
        let on_message = on_message(&global, version.clone(), mapping)?;
        global.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Ensure that the closures are not dropped
        on_message.forget();
        on_error.forget();


        spawn_local(async move {
            match worker_loop().await {
                Ok(_) => {
                    tracing::error!("WORKER EXITED!1");
                }
                Err(e) => {
                    tracing::error!("WORKER ERRORED! {:#?}", e);
                }
            }
        });
    } else {
        console::log_1(&JsValue::from_str("not in dedicated worker"));
        return Err("not in dedicated worker".into());
    }

    Ok(())
}

fn on_error(
    _global: &DedicatedWorkerGlobalScope,
    _version: String,
) -> std::result::Result<Closure<dyn FnMut(web_sys::ErrorEvent)>, JsValue> {
    tracing::info!("dedicated worker on_error() registration");
     Ok(Closure::wrap(
        Box::new(move |event: web_sys::ErrorEvent| {
            tracing::error!("dedicated worker error: {:#?}", event);
    })))
}

fn on_message(
    global: &DedicatedWorkerGlobalScope,
    version: String,
    mapping: Arc<ApiImplMapping>,
) -> std::result::Result<Closure<dyn FnMut(web_sys::MessageEvent)>, JsValue> {
    console::log_1(&JsValue::from_str("dedicated worker on_message() registration"));
    let global_clone = global.clone();
    let mapping = mapping.clone();
    Ok(Closure::wrap(
        Box::new(move |event: web_sys::MessageEvent| {
            let global_clone = global_clone.clone();
            let version = version.clone();
            let data = event.data();
            let data = match serde_wasm_bindgen::from_value::<WorkerMessage>(data.clone()) {
                Ok(data) => data,
                Err(e) => {
                    tracing::error!("deserialization error on message: {e:?}");
                    return;
                }
            };

            tracing::info!("on_message data: {:#?}", data);

            if &data.msg_type == "ping" {
                let client_version = data.msg_content;
                if &client_version == &version.clone().as_bytes().to_vec() {
                    tracing::info!("PING: SAME VERSION. WELCOME.");
                } else {
                    tracing::info!("PING: DIFFERENT VERSION.");
                }

                tracing::info!("Create message type=pong and version={}", version);
                let data2 = WorkerMessage {
                    msg_id: 0,
                    msg_type: "pong".to_string(),
                    msg_content: version.as_bytes().to_vec(),
                };
                let data2 = serde_wasm_bindgen::to_value(&data2).expect("serialize");

                match global_clone.post_message(&data2) {
                    Ok(_i) => {}
                    Err(e) => {
                        tracing::warn!("CANNOT POST MESSAGE TO CLIENT! {:#?}", e);
                    }
                }
            } else {
                tracing::info!("Got App Message, type = {}({})", data.msg_type, data.msg_id);
                let mapping = mapping.clone();

                spawn_local(async move {
                    let request = data.clone();
                    let mapping = mapping.clone();
                    let response =
                        api_asscrack::crack_worker::api_worker::compute_response_message(request, mapping)
                            .await;
                    let response = serde_wasm_bindgen::to_value(&response).expect("serialize");
                    match global_clone.post_message(&response) {
                        Ok(_i) => {}
                        Err(e) => {
                            tracing::warn!("CANNOT POST MESSAGE TO CLIENT! {:#?}", e);
                        }
                    };
                });
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>,
    ))
}

#[tracing::instrument()]
async fn worker_loop() -> anyhow::Result<()> {
    let mut i = 3000;
    loop {
        sleep_ms(i).await;
        i *= 2;
        match worker_iteration().await {
            Ok(_v) => {
                // tracing::info!("worker iteration exited: {:#?}.", v);
            }
            Err(e) => {
                tracing::error!("worker iteration error: {:#?}", e);
            }
        }
    }
}

async fn worker_iteration() -> anyhow::Result<()> {
    tracing::info!(
        "worker_iteration crack smoker init 2. timestamp = {}",
        api_asscrack::_crack_utils::get_timestamp_now_ms()
    );

    Ok(())
}


fn get_version(_global: DedicatedWorkerGlobalScope) -> Option<String> {
    // let global = js_sys::global();

    const KEY: &str = "__wasm_worker_md5";
    let key = js_sys::eval(&KEY.to_string())
        .unwrap_or_default()
        .as_string();
    key
}
