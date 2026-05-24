use api_asscrack::{
    anyhow,
    crack_worker::{WorkerLoaderFactory, WorkerMessage, WorkerPipe},
};
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Navigator, RegistrationOptions, ServiceWorkerRegistration, ServiceWorkerState, console,
};

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
            worker_scope,
            version,
        } = self.clone();

        let _e = wasm_bindgen_futures::spawn_local(async move {
            match register_service_worker(worker_url, worker_type, worker_scope).await {
                Ok(_) => {
                    tracing::info!("worker registration finished.")
                }
                Err(e) => {
                    tracing::error!("error running wasm service registration: {:#?}", e)
                }
            }
        });
        // let version = include_bytes!("../assets/pkg_web_serviceworker/md5.txt");
        // let version = String::from_utf8_lossy(version).trim().to_string();
        tracing::info!("ping to worker version = {}", version);
        let _active = ping(version).await;
        let _active = _active.map_err(|e| anyhow::anyhow!(format!("{e:#?}")));

        // tracing::info!("reply from ping: {:?}", _active);
        _active
    }
}

/// Retrieves the current service worker registration from the navigator
async fn get_service_reg(navigator: &Navigator) -> Result<ServiceWorkerRegistration, JsValue> {
    let fut = navigator.service_worker().ready()?;
    let res = JsFuture::from(fut).await?;
    Ok(ServiceWorkerRegistration::from(res))
}

async fn get_service_reg2(navigator: &Navigator) -> Result<ServiceWorkerRegistration, JsValue> {
    let list = navigator.service_worker().get_registrations().await?;
    tracing::info!("fetch reg list : {:#?}", list);
    if !(list.is_array()) {
        return Err("registrations is not array".into());
    }
    let list2 = js_sys::Array::from(&list);
    if !(list2.length() > 0) {
        return Err("registrations is empty array".into());
    }
    let item = list2.into_iter().next().unwrap();

    // let list: = list.from();

    Ok(ServiceWorkerRegistration::from(item))
    // return Err("X".into());
}

fn get_worker_from_reg(reg: &ServiceWorkerRegistration) -> Option<web_sys::ServiceWorker> {
    reg.active()
        .or_else(|| reg.waiting())
        .or_else(|| reg.installing())
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

/// This is the entry point of the service worker.
/// This function is responsible for loading a service worker script from the given URL.
/// The implementation largely follows the JavaScript code above, but is written using wasm_bindgen
// #[wasm_bindgen]
async fn register_service_worker(
    worker_url: String,
    worker_type: String,
    worker_scope: String,
    // _try_once: bool,
) -> Result<Promise, JsValue> {
    console::log_1(&"registering service worker via wasm_bindgen".into());

    let window = web_sys::window().expect("no global `window` exists");
    let location = window.location();
    let navigator = window.navigator();
    let service_worker = navigator.service_worker();

    let location_href = location.href().expect("no href found");
    let url = web_sys::Url::new_with_base(&worker_url, &location_href)?;
    let url = url.to_string().as_string().unwrap();

    console::log_2(&"Got URL: ".into(), &(url.clone().into()));

    let mut opts = RegistrationOptions::new();
    
    #[allow(deprecated)]
    opts.scope(&worker_scope);
    #[allow(deprecated)]
    opts.type_(worker_type.as_str());

    console::log_2(
        &"registering service worker with opts".into(),
        &opts.clone().into(),
    );

    let registration_fut = service_worker.register_with_options(&url, &opts);
    let registration_res = JsFuture::from(registration_fut).await?;
    let registration = ServiceWorkerRegistration::from(registration_res);

    let registered_worker = get_worker_from_reg(&registration)
        .ok_or_else(|| JsValue::from_str("Service worker registration is not valid"))?;

    console::log_2(
        &"registered service worker".into(),
        &registered_worker.clone().into(),
    );

    // Check to see if the registered worker is the same url
    if registered_worker.script_url() != url {
        console::log_1(&"registered worker is not the same url".into());
        tracing::info!(
            "registered script url: {:?}   old script url: {:?}",
            registered_worker.script_url(),
            url,
        );

        let update_fut = registration.update()?;
        JsFuture::from(update_fut).await?;

        console::log_1(&"service worker updated".into());
    }

    // Await service worker to be ready
    let service_reg = get_service_reg(&navigator).await?;

    if navigator.service_worker().controller().is_none() {
        // TODO: Check for errors such as when calling unregister, and reload the page
        console::log_1(&"service worker is not controlling".into());

        let reg = JsFuture::from(
            navigator
                .service_worker()
                .get_registration_with_document_url("/"),
        )
        .await?;
        let reg = ServiceWorkerRegistration::from(reg);
        console::log_1(&"unregistering service worker".into());

        JsFuture::from(reg.unregister()?).await?;
        console::log_1(&"service worker unregistered, trying to re-register".into());

        location.reload()?;
        return Ok(Promise::resolve(&JsValue::NULL));
    }

    // attempt to get the service worker from the registration, if it's not there, try to re-get the registration and try again
    let service_worker = match get_worker_from_reg(&service_reg) {
        Some(worker) => worker,
        None => {
            console::log_1(&"no worker on registration, trying to re-get registration".into());
            let service_reg = get_service_reg(&navigator).await?;
            match get_worker_from_reg(&service_reg) {
                Some(worker) => worker,
                None => {
                    console::log_1(
                        &"no worker on registration, waiting a bit and trying again".into(),
                    );
                    sleep(&window, 50).await?;

                    match get_worker_from_reg(&service_reg) {
                        Some(worker) => worker,
                        None => {
                            console::log_1(&"no worker on registration, giving up".into());
                            return Err(JsValue::from_str(
                                "Service worker registration is not valid",
                            ));
                        }
                    }
                }
            }
        }
    };

    match service_worker.state() {
        ServiceWorkerState::Redundant => {
            console::log_1(&"service worker is redundant".into());
            // reload
            location.reload()?;
        }
        ServiceWorkerState::Activated => {
            console::log_1(&"service worker is activated".into());
        }
        _ => {
            console::log_1(
                &"service worker controlling, but not activated. Waiting on event".into(),
            );
            // reload the page
            location.reload()?;
        }
    }

    tracing::info!("register service worker(): ALL OK.");
    Ok(Promise::resolve(&JsValue::from(service_worker)))
}

async fn ping(version: String) -> Result<WorkerPipe, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    sleep(&window, 100).await?;
    tracing::info!("starting ping!");
    const N: i32 = 10;
    for _i in 1..=N {
        tracing::info!("try ping {} / {}", _i, N);
        let _r = _try_ping(version.clone()).await;
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

    Err("failed to ping service worker!".into())
}

async fn _try_ping(version: String) -> Result<WorkerPipe, JsValue> {
    let window = web_sys::window().expect("no global `window` exists");
    // let location = window.location();
    let navigator = window.navigator();
    let container = navigator.service_worker();

    tracing::info!("get service reg...");
    let reg = get_service_reg2(&navigator).await?;
    // tracing::info!("service reg = {:?}", reg);

    tracing::info!("get worker...");
    let Some(worker) = get_worker_from_reg(&reg) else {
        tracing::error!("no service worker on registration...");
        return Err("no service worker on registration.".into());
    };
    // tracing::info!("worker = {:?}", worker);

    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<WorkerMessage>(1024);
    let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(1024);
    let (one_tx, mut one_rx) = tokio::sync::mpsc::channel(1);

    // set on_message just bevfore posting

    type T = Closure<dyn FnMut(web_sys::MessageEvent)>;
    let version2 = version.clone();
    let c: T = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let one_tx = one_tx.clone();
        // tracing::info!("backflop event: {:#?}", event);
        let data = event.data();
        let data = serde_wasm_bindgen::from_value::<WorkerMessage>(data);
        let data = match data {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("cannot deserialize message: {e:?}");
                return;
            }
        };
        // tracing::info!("Got back message: {:?}", data);

        if &data.msg_type == "pong" {
            let server_version = &data.msg_content;
            if server_version != &version2.as_bytes().to_vec() {
                tracing::warn!("==>> SERVER VERSION DIFFER! UPDATE! PLZ SEPPUKKU!");
                match reg.update() {
                    Ok(p) => {
                        wasm_bindgen_futures::spawn_local(async move {
                            tracing::info!("worker update() result: {:?}", p.await);

                            let window = web_sys::window().expect("no global `window` exists");
                            let _ = sleep(&window, 500).await;
                            let window = web_sys::window().expect("no global `window` exists");
                            tracing::info!("REFRESHING PAGE NOW!");
                            let location = window.location();
                            let _ = location.reload();
                        });
                    }
                    Err(e) => {
                        tracing::error!("error updating! {e:#?}");
                    }
                }
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
                // tracing::info!(
                //     "Passing message back to caller: {}({})",
                //     &data.msg_type,
                //     &data.msg_id
                // );

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

    container.set_onmessage(Some(c.as_ref().unchecked_ref()));

    // post message

    let ping = WorkerMessage {
        msg_id: 0,
        msg_type: "ping".to_string(),
        msg_content: version.clone().as_bytes().to_vec(),
    };
    // tracing::info!("_try_ping post message: {:?}", &ping);
    worker.post_message(&serde_wasm_bindgen::to_value(&ping)?)?;

    // wait for response
    tracing::info!("waiting for response from worker...");
    let _o = one_rx.recv().await;
    if _o.is_none() {
        tracing::error!("pingpong fail.");
        return Err("pingpong fail!".into());
    }
    tracing::info!("Ok. starting worker dispatching...");

    wasm_bindgen_futures::spawn_local(async move {
        while let Some(req) = req_rx.recv().await {
        // tracing::info!("Posting message to web worker: {}({})", &req.msg_type, &req.msg_id);
        match &serde_wasm_bindgen::to_value(&req) {
            Ok(o) => {
                match worker.post_message(o) {
                    Ok(_o) => {
                        // tracing::info!("Successfully posted service worker message: {}({})", &req.msg_type, &req.msg_id )
                    }
                    Err(e) => {
                        tracing::error!("worker.post_message() error: {e:#?}");
                    }
                }
            }
            Err(e) => {
                tracing::error!("to_value() error: {e:#?}");
            }
        }
    }
    // Ok(())
    ;
    });

    c.forget();
    Ok(WorkerPipe { req_tx, resp_rx })
}
