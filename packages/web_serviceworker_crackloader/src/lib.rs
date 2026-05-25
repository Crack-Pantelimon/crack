use api_asscrack::{
    _crack_utils::n0_future, anyhow, crack_worker::{WorkerLoaderFactory, WorkerMessage, WorkerPipe}
};
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::console;

#[derive(Clone)]
pub struct WebWorkerFactory {
    pub worker_url: String,
    pub worker_type: String,
    pub worker_scope: String,
    pub version: String,
}

#[api_asscrack::async_trait::async_trait(?Send)]
impl WorkerLoaderFactory for WebWorkerFactory {
    async fn load_worker(&self) -> anyhow::Result<WorkerPipe> {
        let Self {
            worker_url,
            worker_type,
            worker_scope: _,
            version,
        } = self.clone();

        tracing::info!("loading shared worker version = {}", version);
        let _active = ping(worker_url, worker_type, version).await;
        let _active = _active.map_err(|e| anyhow::anyhow!(format!("{e:#?}")));

        _active
    }
}

/// Creates a JS promise that resolves after the given number of milliseconds and awaits it
async fn sleep(window: &web_sys::Window, ms: i32) -> Result<(), JsValue> {
    let promise = Promise::new(&mut |resolve, _reject| {
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    JsFuture::from(promise).await?;
    Ok(())
}

async fn ping(worker_url: String, worker_type: String, version: String) -> Result<WorkerPipe, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    sleep(&window, 100).await?;
    tracing::info!("starting ping!");
    const N: i32 = 10;
    for _i in 1..=N {
        tracing::info!("try ping {} / {}", _i, N);
        let _r = _try_ping(worker_url.clone(), worker_type.clone(), version.clone()).await;
        if _i == N {
            return Ok(_r?);
        }
        match _r {
            Ok(p) => {
                return Ok(p);
            }
            Err(e) => {
                tracing::error!("failed to ping webserver: {:#?}", e);
            }
        }
        sleep(&window, 1000).await?;
    }

    // refresh the page
    let window = web_sys::window().expect("no global `window` exists");
    let location = window.location();
    tracing::error!("WILL REFRESH PAGE NOW.");
    location.reload()?;

    Err("failed to ping shared worker!".into())
}

async fn _try_ping(worker_url: String, worker_type: String, version: String) -> Result<WorkerPipe, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    let location = window.location();

    let location_href = location.href().expect("no href found");
    let url = web_sys::Url::new_with_base(&worker_url, &location_href)?;
    let url_str = url.to_string().as_string().unwrap();

    console::log_2(&"Got SharedWorker URL: ".into(), &(url_str.clone().into()));

    let options = web_sys::WorkerOptions::new();
    if worker_type == "module" {
        options.set_type(web_sys::WorkerType::Module);
    } else {
        options.set_type(web_sys::WorkerType::Classic);
    }
    let worker = web_sys::SharedWorker::new_with_worker_options(&url_str, &options)?;

    let port = worker.port();
    port.start();

    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<WorkerMessage>(1024);
    let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(1024);
    let (one_tx, mut one_rx) = tokio::sync::mpsc::channel(1);

    type T = Closure<dyn FnMut(web_sys::MessageEvent)>;
    let version2 = version.clone();
    let c: T = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let one_tx = one_tx.clone();
        let data = event.data();
        let data = serde_wasm_bindgen::from_value::<WorkerMessage>(data);
        let data = match data {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("cannot deserialize message: {e:?}");
                return;
            }
        };

        if &data.msg_type == "pong" {
            let server_version = &data.msg_content;
            if server_version != &version2.as_bytes().to_vec() {
                tracing::warn!("==>> SERVER VERSION DIFFER! UPDATE! PLZ REFRESH!");
                wasm_bindgen_futures::spawn_local(async move {
                    let window = web_sys::window().expect("no global `window` exists");
                    let _ = sleep(&window, 500).await;
                    let window = web_sys::window().expect("no global `window` exists");
                    tracing::info!("REFRESHING PAGE NOW!");
                    let location = window.location();
                    let _ = location.reload();
                });
            } else {
                tracing::info!("SERVER VERSION OK.");
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
            };
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
    }));

    port.set_onmessage(Some(c.as_ref().unchecked_ref()));

    // post message
    let ping = WorkerMessage {
        msg_id: 0,
        msg_type: "ping".to_string(),
        msg_content: version.clone().as_bytes().to_vec(),
    };
    port.post_message(&serde_wasm_bindgen::to_value(&ping)?)?;

    // wait for response
    tracing::info!("waiting for response from worker...");
    let _o = n0_future::time::timeout(std::time::Duration::from_millis(333), one_rx.recv());
    let _o = _o.await;
    let _o_is_ok = _o.ok().flatten().is_some();

    if !_o_is_ok {
        tracing::error!("pingpong fail.");
        return Err("pingpong fail!".into());
    }
    tracing::info!("Ok. starting worker dispatching...");

    let port_clone = port.clone();
    wasm_bindgen_futures::spawn_local(async move {
        while let Some(req) = req_rx.recv().await {
            match &serde_wasm_bindgen::to_value(&req) {
                Ok(o) => {
                    match port_clone.post_message(o) {
                        Ok(_o) => {}
                        Err(e) => {
                            tracing::error!("port.post_message() error: {e:#?}");
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("to_value() error: {e:#?}");
                }
            }
        }
    });

    c.forget();
    Ok(WorkerPipe { req_tx, resp_rx })
}
