//! WASM dedicated-worker backend for the async worker RPC framework.
//!
//! Runs inside a Web Worker, receives typed RPC messages from JavaScript,
//! dispatches them via [`ApiImplMapping`], and returns serialized replies.
//!
//! [`ApiImplMapping`]: api_asscrack::crack_worker::api_worker::ApiImplMapping

/// Re-export of the Dioxus logger for wasm worker initialization.
pub extern crate dioxus_logger;
/// Re-export of wasm-bindgen for JavaScript interop in worker code.
pub extern crate wasm_bindgen;
/// Schedules a future on the wasm local task queue from worker code.
pub use wasm_bindgen_futures::spawn_local;

use std::sync::{Arc, OnceLock};

use api_asscrack::crack_worker::WorkerMessage;
use api_asscrack::crack_worker::api_worker::ApiImplMapping;

use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

/// Initializes the dedicated worker: logging and optional OPFS storage.
#[wasm_bindgen(js_name = "initDedicatedWorker")]
pub async fn _js_init_dedicated_worker() -> Result<(), JsValue> {
    tracing::info!("init_dedicated_worker");

    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        tracing::info!("install_opfs_sahpool() ... ");
        storage_crackhouse::install_opfs_sahpool()
            .await
            .map_err(|e| {
                tracing::error!("install_opfs_sahpool() error: {e:#?}");
                JsValue::from_str(&format!("install_opfs_sahpool() erorr: {e:#?}"))
            })?;
        tracing::warn!("install_opfs_sahpool() success!");
    }
    Ok(())
}

/// Handles one inbound worker message and returns the serialized reply.
#[wasm_bindgen(js_name = "computePayloadReply")]
pub async fn _js_compute_payload_reply(msg: JsValue) -> Result<JsValue, JsValue> {
    // tracing::info!("compute_payload_reply: msg={msg:?}");

    _compute_payload_2(msg)
        .await
        .map_err(|e| format!("{e:?}").into())
}

async fn _compute_payload_2(msg: JsValue) -> anyhow::Result<JsValue> {
    let Some(mapping) = IMPL.get().cloned() else {
        anyhow::bail!("no API mapping");
    };

    let t0 = _crack_utils::get_timestamp_now_ms();
    let msg_id = js_sys::Reflect::get(&msg, &"msg_id".into())
        .map_err(|e| anyhow::anyhow!("Reflect error on msg_id: {e:?}"))?
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("msg_id is not a number"))? as u32;

    let msg_type = js_sys::Reflect::get(&msg, &"msg_type".into())
        .map_err(|e| anyhow::anyhow!("Reflect error on msg_type: {e:?}"))?
        .as_string()
        .ok_or_else(|| anyhow::anyhow!("msg_type is not a string"))?;

    let js_content = js_sys::Reflect::get(&msg, &"msg_content".into())
        .map_err(|e| anyhow::anyhow!("Reflect error on msg_content: {e:?}"))?;

    let uint8_array = js_sys::Uint8Array::from(js_content);
    let msg_content = uint8_array.to_vec();
    let t_extract = _crack_utils::get_timestamp_now_ms();

    let data = WorkerMessage {
        msg_id,
        msg_type: msg_type.clone(),
        msg_content,
    };

    if &data.msg_type == "ping" {
        let client_version = data.msg_content;

        tracing::debug!("Create message type=pong");
        let js_response = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&js_response, &"msg_id".into(), &JsValue::from(0));
        let _ = js_sys::Reflect::set(&js_response, &"msg_type".into(), &JsValue::from("pong"));

        let view = unsafe { js_sys::Uint8Array::view(&client_version) };
        let uint8_array = view.slice(0, client_version.len() as u32);
        let _ = js_sys::Reflect::set(&js_response, &"msg_content".into(), &uint8_array);

        return Ok(js_response.into());
    } else {
        tracing::debug!("Got App Message, type = {}({})", data.msg_type, data.msg_id);
        let mapping = mapping.clone();

        let t_start_func = _crack_utils::get_timestamp_now_ms();
        let response =
            api_asscrack::crack_worker::api_worker::compute_response_message(data, mapping).await;
        let t_end_func = _crack_utils::get_timestamp_now_ms();

        let js_response = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &js_response,
            &"msg_id".into(),
            &JsValue::from(response.msg_id),
        );
        let _ = js_sys::Reflect::set(
            &js_response,
            &"msg_type".into(),
            &JsValue::from(&response.msg_type),
        );

        let view = unsafe { js_sys::Uint8Array::view(&response.msg_content) };
        let uint8_array = view.slice(0, response.msg_content.len() as u32);
        let _ = js_sys::Reflect::set(&js_response, &"msg_content".into(), &uint8_array);
        let t_serialize = _crack_utils::get_timestamp_now_ms();

        tracing::debug!(
            "Worker handler loops: extract={} ms, func={} ms, serialize={} ms (size={} bytes)",
            t_extract - t0,
            t_end_func - t_start_func,
            t_serialize - t_end_func,
            response.msg_content.len()
        );

        return Ok(js_response.into());
    }
}

static IMPL: OnceLock<Arc<ApiImplMapping>> = OnceLock::new();

/// Registers the API implementation mapping before handling messages.
///
/// Must be called once; a second call returns an error.
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

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    // Build/link smoke only: this crate needs a real dedicated-worker scope at
    // runtime, which the wasm-pack browser test harness does not provide. So we
    // reference the public entry points (pulling them through the linker) and
    // construct the payload type WITHOUT invoking any worker-scope APIs.
    #[wasm_bindgen_test]
    fn links() {
        let _init = _js_init_dedicated_worker;
        let _compute = _js_compute_payload_reply;
        let _register = web_worker_registration;
        let _spawn = spawn_local::<std::future::Ready<()>>;

        let msg = WorkerMessage {
            msg_id: 1,
            msg_type: "ping".to_string(),
            msg_content: vec![1, 2, 3],
        };
        assert_eq!(msg.msg_id, 1);
        assert_eq!(msg.msg_type, "ping");
        assert_eq!(msg.msg_content, vec![1, 2, 3]);
    }
}
