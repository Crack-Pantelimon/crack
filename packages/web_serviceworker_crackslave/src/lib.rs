use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(js_name="initDedicatedWorker")]
pub fn init_dedicated_worker() -> Result<(), JsValue> {
    tracing::info!("init_dedicated_worker");
    Ok(())
}


#[wasm_bindgen(js_name="computePayloadReply")]
pub async fn compute_payload_reply(msg: JsValue) -> Result<JsValue, JsValue> {
    tracing::info!("compute_payload_reply: msg={msg:?}");
    Ok(msg)
}

