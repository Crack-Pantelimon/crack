pub extern crate dioxus_logger;
pub extern crate wasm_bindgen;
use anyhow::Context;
pub use wasm_bindgen_futures::spawn_local;

use std::sync::{Arc, OnceLock};

use api_asscrack::crack_worker::WorkerMessage;
use api_asscrack::crack_worker::api_worker::ApiImplMapping;

use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(js_name = "initDedicatedWorker")]
pub fn _js_init_dedicated_worker() -> Result<(), JsValue> {
    tracing::info!("init_dedicated_worker");
    Ok(())
}

#[wasm_bindgen(js_name = "computePayloadReply")]
pub async fn _js_compute_payload_reply(msg: JsValue) -> Result<JsValue, JsValue> {
    tracing::info!("compute_payload_reply: msg={msg:?}");

    _compute_payload_2(msg)
        .await
        .map_err(|e| format!("{e:?}").into())
}

async fn _compute_payload_2(msg: JsValue) -> anyhow::Result<JsValue> {
    let Some(mapping) = IMPL.get().cloned() else {
        anyhow::bail!("no API mapping");
    };

    let data = match serde_wasm_bindgen::from_value::<WorkerMessage>(msg) {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("deserialization error on message: {e:?}");
            anyhow::bail!("deserialization error on message: {e:?}");
        }
    };

    tracing::info!("on_message data: {:#?}", data);

    if &data.msg_type == "ping" {
        let client_version = data.msg_content;

        tracing::info!("Create message type=pong");
        let data2 = WorkerMessage {
            msg_id: 0,
            msg_type: "pong".to_string(),
            msg_content: client_version,
        };
        let data2 = serde_wasm_bindgen::to_value(&data2).context("serialize")?;
        return Ok(data2);
    } else {
        tracing::info!("Got App Message, type = {}({})", data.msg_type, data.msg_id);
        let mapping = mapping.clone();

        let response =
            api_asscrack::crack_worker::api_worker::compute_response_message(data, mapping).await;
        let response = serde_wasm_bindgen::to_value(&response).context("serialize")?;
        return Ok(response);
    }
}

static IMPL: OnceLock<Arc<ApiImplMapping>> = OnceLock::new();

pub async fn web_worker_registration(
    mapping: Arc<ApiImplMapping>,
) -> std::result::Result<(), JsValue> {
    tracing::info!("web_worker_registration() ... ");
    IMPL.set(mapping)
        .map_err(|_e| JsValue::from_str("cannot set impl mapping twice"))?;

    let global = js_sys::global();
    tracing::info!("global!! {:#?}", &global);

    Ok(())
}
