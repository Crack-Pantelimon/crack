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
    pub type WorkerHandlesJs;
    pub fn init_workers2() -> WorkerHandlesJs;
    #[wasm_bindgen(method)]
    pub fn send_message(this: &WorkerHandlesJs, message: &JsValue);
    #[wasm_bindgen(method)]
    pub fn set_onmessage(this: &WorkerHandlesJs, callback: JsValue);
}

#[derive(Clone)]
pub struct WebWorkerFactory {}

#[api_asscrack::async_trait::async_trait(?Send)]
impl WorkerLoaderFactory for WebWorkerFactory {
    async fn load_worker(&self) -> anyhow::Result<WorkerPipe> {
        make_worker().await
    }
}

/// Creates a JS promise that resolves after the given number of milliseconds and awaits it
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
        tracing::info!("GOT MESSAGE BCK! {message:?}");
    };
    let closure = Closure::new(Box::new(closure) as Box<dyn FnMut(JsValue)>);
    let closure = closure.into_js_value();

    js_handles.set_onmessage(closure);
    tracing::info!("Sending message");
    // js_handles.send_message(&"penis 2".to_string().into());

    Ok(js_handles)
}

async fn make_worker() -> anyhow::Result<WorkerPipe> {
    let js_handles = get_js_worker().await?;

    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<WorkerMessage>(1024);
    let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(1024);
    let (one_tx, mut one_rx) = tokio::sync::mpsc::channel(1);

    tracing::info!("set onmessage.");

    let closure = move |data| {
        let one_tx = one_tx.clone();

        let data = serde_wasm_bindgen::from_value::<WorkerMessage>(data);
        let data = match data {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("cannot deserialize message: {e:?}");
                return;
            }
        };

        if &data.msg_type == "pong" {
            wasm_bindgen_futures::spawn_local(async move {
                let _r = one_tx.send(()).await;
                match _r {
                    Ok(_r) => {
                        tracing::info!("reply ok.");
                    }
                    Err(e) => {
                        tracing::error!("error sending pong! err => {e:#?}")
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

    const N: i32 = 10;
    let mut _ok = false;
    for _i in 1..=N {
        js_handles.send_message(&serde_wasm_bindgen::to_value(&ping)?);

        // wait for response
        tracing::info!("waiting for response from worker...");
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
            match &serde_wasm_bindgen::to_value(&req) {
                Ok(o) => {
                    js_handles.send_message(o);
                }
                Err(e) => {
                    tracing::error!("to_value() error: {e:#?}");
                }
            }
        }
    });

    // c.forget();
    Ok(WorkerPipe { req_tx, resp_rx })
}
