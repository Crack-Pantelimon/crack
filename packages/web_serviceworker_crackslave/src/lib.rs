pub extern crate wasm_bindgen;
pub extern crate dioxus_logger;


use std::sync::Arc;

use api_asscrack::{_crack_utils::sleep_ms, crack_worker::WorkerMessage};
use api_asscrack::crack_worker::api_worker::ApiImplMapping;
use wasm_bindgen::JsValue;
pub use wasm_bindgen_futures::spawn_local;


#[derive(thiserror::Error, Debug)]
pub enum ServiceWorkerError {
    #[error("not in a service worker")]
    NotInServiceWorker,
}

impl From<ServiceWorkerError> for JsValue {
    fn from(e: ServiceWorkerError) -> Self {
        JsValue::from_str(&e.to_string())
    }
}

use wasm_bindgen::prelude::*;
use web_sys::{ServiceWorkerGlobalScope, console};

pub fn do_worker_registration(mapping: Arc<ApiImplMapping>) -> std::result::Result<(), JsValue> {
    let global = js_sys::global();
    tracing::info!("global!! {:#?}", &global);

    if let Ok(true) = js_sys::Reflect::has(&global, &JsValue::from_str("ServiceWorkerGlobalScope"))
    {
        console::log_1(&JsValue::from_str("in service worker V3"));
        // we're in a service worker, so we can cast the global to a ServiceWorkerGlobalScope
        let global: ServiceWorkerGlobalScope = global.unchecked_into::<ServiceWorkerGlobalScope>();

        let version = get_version(global.clone()).unwrap_or_default();
        tracing::info!("version  =  '{}'", &version);

        // Force immediate activation
        let on_install = on_install(&global)?;
        let on_activate = on_activate(&global)?;
        global.set_oninstall(Some(on_install.as_ref().unchecked_ref()));
        global.set_onactivate(Some(on_activate.as_ref().unchecked_ref()));

        // register all the other callbacks
        let on_message = on_message(&global, version, mapping)?;
        global.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Ensure that the closures are not dropped before the service worker is terminated
        // This is technically a memory leak, but I'm not sure that it matters in this case
        on_install.forget();
        on_activate.forget();
        on_message.forget();

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
        console::log_1(&JsValue::from_str("not in service worker"));
        return Err(ServiceWorkerError::NotInServiceWorker.into());
    }

    Ok(())
}

fn on_install(
    global: &ServiceWorkerGlobalScope,
) -> std::result::Result<Closure<dyn FnMut(web_sys::ExtendableEvent)>, JsValue> {
    console::log_1(&JsValue::from_str("serviceworker on_install()"));

    let skip_waiting = global.skip_waiting()?;
    Ok(Closure::wrap(
        Box::new(move |event: web_sys::ExtendableEvent| {
            tracing::info!("on_install event: {event:?}");
            event.wait_until(&skip_waiting).unwrap();
        }) as Box<dyn FnMut(_)>,
    ))
}

fn on_activate(
    global: &ServiceWorkerGlobalScope,
) -> std::result::Result<Closure<dyn FnMut(web_sys::ExtendableEvent)>, JsValue> {
    console::log_1(&JsValue::from_str("serviceworker on_activate()"));

    let clients = global.clients();
    Ok(Closure::wrap(
        Box::new(move |event: web_sys::ExtendableEvent| {
            tracing::info!("on_activate event: {event:?}");

            event.wait_until(&clients.claim()).unwrap();
        }) as Box<dyn FnMut(_)>,
    ))
}

/// Displays a message in the console when a message is received from the client
fn on_message(
    _global: &ServiceWorkerGlobalScope,
    version: String,
    mapping:  Arc<ApiImplMapping>,
) -> std::result::Result<Closure<dyn FnMut(web_sys::ExtendableMessageEvent)>, JsValue> {
    let reg = _global.registration();

    // let clients = _global.clients();
    console::log_1(&JsValue::from_str("serviceworker on_message()"));
    let mapping = mapping.clone();
    Ok(Closure::wrap(
        Box::new(move |event: web_sys::ExtendableMessageEvent| {
            // let event_source = event.source();
            let version = version.clone();
            let reg = reg.clone();

            let Some(source) = event.source() else {
                tracing::info!("on_message event source: {:?}", event.source());
                tracing::warn!("cannot get event source. drop message!");
                return;
            };
            let value: &JsValue = source.as_ref();
            let client = web_sys::WindowClient::from(value.clone());
            tracing::info!("on_message event source: {:#?}", source);
            // let window_client = &web_sys::WindowClient::from(source)) else {
            // tracing::warn!("cannot fetch window client. drop message!");
            // return;
            // };
            let data = &event.data();
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
                    tracing::info!("PING: SAME VERSION. WELCOME NEW TAB.");
                } else {
                    tracing::info!("PING: DIFFERENT VERSION. MUST SEPPUKKU NOW.");
                    seppukku(reg);
                }

                tracing::info!("Create message type=pong and version={}", version);
                let data2 = WorkerMessage {
                    msg_id: 0,
                    msg_type: "pong".to_string(),
                    msg_content: version.as_bytes().to_vec(),
                };
                let data2 = serde_wasm_bindgen::to_value(&data2).expect("serialize");

                match client.post_message(&data2) {
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
                    match client.post_message(&response) {
                        Ok(_i) => {}
                        Err(e) => {
                            tracing::warn!("CANNOT POST MESSAGE TO CLIENT! {:#?}", e);
                        }
                    };
                });
            }

            // let client = clients.get(.)
        }) as Box<dyn FnMut(_)>,
    ))
}

fn seppukku(_global: web_sys::ServiceWorkerRegistration) {
    let _w = _global.update();
    match _w {
        Ok(p) => spawn_local(async move {
            tracing::info!("SEPPUKKU SENDING UPDATE RESULT: {:?}", p.await);
        }),
        Err(e) => {
            tracing::error!("FAILED TO SEPPUKKU! {e:#?}");
        }
    }
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


fn get_version(_global: ServiceWorkerGlobalScope) -> Option<String> {
    // let global = js_sys::global();

    const KEY: &str = "__wasm_worker_md5";
    let key = js_sys::eval(&KEY.to_string())
        .unwrap_or_default()
        .as_string();
    key
}
