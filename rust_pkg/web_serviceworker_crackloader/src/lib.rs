//! Browser-side adapter that turns the JavaScript dedicated-worker bridge into a Rust [`WorkerLoaderFactory`].
//!
//! The JavaScript host supplies `init_workers2`, which this crate uses to initialize a worker,
//! exchange typed messages, and expose request and response channels to Rust callers.

use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

use api_asscrack::{
    _crack_utils::n0_future,
    anyhow::{self, Context},
    crack_worker::{WorkerLoaderFactory, WorkerMessage, WorkerPipe},
};
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    /// Opaque JavaScript handle for the worker pair managed by the page host.
    pub type WorkerHandlesJs;

    /// Initializes the JavaScript worker bridge and returns its message handle.
    ///
    /// The host must make this function available before a [`WebWorkerFactory`] loads a worker.
    pub fn init_workers2() -> WorkerHandlesJs;

    /// Sends `message` to the worker represented by `this`.
    ///
    /// The message must use the bridge's `msg_id`, `msg_type`, and `msg_content` shape.
    #[wasm_bindgen(method)]
    pub fn send_message(this: &WorkerHandlesJs, message: &JsValue);

    /// Registers `callback` as the JavaScript `onmessage` handler for `this`.
    ///
    /// The callback receives each response emitted by the worker bridge.
    #[wasm_bindgen(method)]
    pub fn set_onmessage(this: &WorkerHandlesJs, callback: JsValue);
}

/// Factory that initializes and connects to the page-hosted JavaScript worker bridge.
///
/// Calling [`WorkerLoaderFactory::load_worker`] creates the bridge and returns channels for
/// submitting requests and receiving worker responses. It returns an error when the host bridge
/// cannot be found, initialized, or pinged successfully.
#[derive(Clone)]
pub struct WebWorkerFactory {}

#[api_asscrack::async_trait::async_trait(?Send)]
impl WorkerLoaderFactory for WebWorkerFactory {
    async fn load_worker(&self) -> anyhow::Result<WorkerPipe> {
        make_worker().await
    }
}

/// Creates a JavaScript promise that resolves after `ms` milliseconds and awaits it.
/// Returns an error if the promise is rejected.
async fn sleep(ms: i32) -> Result<(), JsValue> {
    let window = web_sys::window().expect("no window?");

    let promise = Promise::new(&mut |resolve, _reject| {
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    JsFuture::from(promise).await?;
    Ok(())
}

#[allow(deprecated)]
/// Waits for the page host to expose `init_workers2`, initializes the bridge, and
/// installs a temporary message logger.
/// Returns an error if the bridge function does not appear within ~3 seconds.
async fn get_js_worker() -> anyhow::Result<WorkerHandlesJs> {
    let window = web_sys::window().context("no window?")?;
    use api_asscrack::_crack_utils::sleep_ms;

    let mut found = false;
    for i in 0..20 {
        if window.has_own_property(&(&"init_workers2".to_string()).into()) {
            tracing::info!("found item!");
            found = true;
            break;
        }
        sleep_ms(150).await;
        tracing::info!("retry {i}... ");
    }

    if !found {
        tracing::error!("did not find startup fm.");
        anyhow::bail!("did not fidn startup fn.")
    }

    let js_handles = init_workers2();
    tracing::info!("set onmessage.");

    let closure = move |message| {
        tracing::debug!("GOT MESSAGE BCK! {message:?}");
    };
    let closure = Closure::new(Box::new(closure) as Box<dyn FnMut(JsValue)>);
    let closure = closure.into_js_value();

    js_handles.set_onmessage(closure);
    tracing::debug!("Sending message");
    // js_handles.send_message(&"penis 2".to_string().into());

    Ok(js_handles)
}

/// Initializes the JavaScript worker bridge, performs a ping-pong handshake,
/// and returns channels for sending requests and receiving responses.
/// Returns an error if the bridge cannot be found, initialized, or pinged.
async fn make_worker() -> anyhow::Result<WorkerPipe> {
    let js_handles = get_js_worker().await?;

    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<WorkerMessage>(1024);
    let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(1024);
    let (one_tx, mut one_rx) = tokio::sync::mpsc::channel(1);

    tracing::info!("set onmessage.");

    let closure = move |data: JsValue| {
        let one_tx = one_tx.clone();

        let msg_id = match js_sys::Reflect::get(&data, &"msg_id".into()) {
            Ok(v) => v.as_f64().unwrap_or(0.0) as u32,
            Err(_) => 0,
        };
        let msg_type = match js_sys::Reflect::get(&data, &"msg_type".into()) {
            Ok(v) => v.as_string().unwrap_or_default(),
            Err(_) => String::new(),
        };
        let js_content = match js_sys::Reflect::get(&data, &"msg_content".into()) {
            Ok(v) => v,
            Err(_) => JsValue::UNDEFINED,
        };

        let uint8_array = js_sys::Uint8Array::from(js_content);
        let msg_content = uint8_array.to_vec();

        let data = WorkerMessage {
            msg_id,
            msg_type,
            msg_content,
        };

        if &data.msg_type == "pong" {
            wasm_bindgen_futures::spawn_local(async move {
                let _r = one_tx.send(()).await;
                match _r {
                    Ok(_r) => {
                        tracing::debug!("reply ok.");
                    }
                    Err(e) => {
                        tracing::debug!("error sending pong! err => {e:#?}")
                    }
                }
            });
        } else {
            let resp_tx = (&resp_tx).clone();

            wasm_bindgen_futures::spawn_local(async move {
                match resp_tx.send(data.clone()).await {
                    Ok(_r) => {}
                    Err(e) => {
                        tracing::error!(
                            "FAILED to send message back to caller: {}({}): {e:#?}",
                            &data.msg_type,
                            &data.msg_id
                        )
                    }
                }
            });
        }
    };
    let closure = Closure::new(Box::new(closure) as Box<dyn FnMut(JsValue)>);
    let closure = closure.into_js_value();
    js_handles.set_onmessage(closure);

    // post message
    let ping = WorkerMessage {
        msg_id: 0,
        msg_type: "ping".to_string(),
        msg_content: "".as_bytes().to_vec(),
    };

    const N: i32 = 200;
    let mut _ok = false;
    for _i in 1..=N {
        let js_ping = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&js_ping, &"msg_id".into(), &JsValue::from(0));
        let _ = js_sys::Reflect::set(&js_ping, &"msg_type".into(), &JsValue::from("ping"));
        let view = unsafe { js_sys::Uint8Array::view(&ping.msg_content) };
        let uint8_array = view.slice(0, ping.msg_content.len() as u32);
        let _ = js_sys::Reflect::set(&js_ping, &"msg_content".into(), &uint8_array);
        js_handles.send_message(&js_ping.into());

        // wait for response
        tracing::debug!("waiting for response from worker...");
        let _o = n0_future::time::timeout(std::time::Duration::from_millis(333), one_rx.recv());
        let _o = _o.await;
        let _o_is_ok = _o.ok().flatten().is_some();

        if _o_is_ok {
            tracing::info!("Pingpong Ok after try {} / {}", _i, N);
            _ok = true;
            break;
        } else {
            tracing::warn!("pingpong fail {} / {}", _i, N);
            let _s = sleep(150).await;
            continue;
        }
    }
    if !_ok {
        tracing::error!("failed to  start dedicated worker!");
        anyhow::bail!("failed to start dedicated worker!");
    }

    wasm_bindgen_futures::spawn_local(async move {
        while let Some(req) = req_rx.recv().await {
            let js_req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&js_req, &"msg_id".into(), &JsValue::from(req.msg_id));
            let _ =
                js_sys::Reflect::set(&js_req, &"msg_type".into(), &JsValue::from(&req.msg_type));

            let view = unsafe { js_sys::Uint8Array::view(&req.msg_content) };
            let uint8_array = view.slice(0, req.msg_content.len() as u32);
            let _ = js_sys::Reflect::set(&js_req, &"msg_content".into(), &uint8_array);

            js_handles.send_message(&js_req.into());
        }
    });

    // c.forget();
    Ok(WorkerPipe { req_tx, resp_rx })
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use api_asscrack::crack_worker::WorkerLoaderFactory;
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn smoke_construct_factory() {
        // Constructing the API surface must not require a real service-worker
        // registration; only `load_worker()` touches the JS `init_workers2` hook.
        let factory = WebWorkerFactory {};
        let cloned = factory.clone();
        let _boxed: Box<dyn WorkerLoaderFactory> = Box::new(cloned);
    }
}
