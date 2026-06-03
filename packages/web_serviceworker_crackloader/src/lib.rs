use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen]
extern "C" {
    pub type WorkerHandlesJs;
    pub fn init_workers2() -> WorkerHandlesJs;
    #[wasm_bindgen(method)]
    fn send_message(this: &WorkerHandlesJs, message: &JsValue);
    #[wasm_bindgen(method)]
    fn set_onmessage(this: &WorkerHandlesJs, callback: JsValue);
}
