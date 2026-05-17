use wasm_bindgen::JsValue;

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

use crate::WorkerMessage;
use crate::compute_message::compute_response_message;



pub(crate) fn do_worker_registration() -> std::result::Result<(), JsValue> {
    
    let global = js_sys::global();
    tracing::info!("global!! {:#?}", &global);


    if let Ok(true) = js_sys::Reflect::has(&global, &JsValue::from_str("ServiceWorkerGlobalScope"))
    {
        console::log_1(&JsValue::from_str("in service worker V3"));
        // we're in a service worker, so we can cast the global to a ServiceWorkerGlobalScope
        let global: ServiceWorkerGlobalScope = global.unchecked_into::<ServiceWorkerGlobalScope>();


    let version = get_version(global.clone()).unwrap_or_default();

        // Force immediate activation
        let on_install = on_install(&global)?;
        let on_activate = on_activate(&global)?;
        global.set_oninstall(Some(on_install.as_ref().unchecked_ref()));
        global.set_onactivate(Some(on_activate.as_ref().unchecked_ref()));

        // register all the other callbacks
        let on_message = on_message(&global, version)?;
        global.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Ensure that the closures are not dropped before the service worker is terminated
        // This is technically a memory leak, but I'm not sure that it matters in this case
        on_install.forget();
        on_activate.forget();
        on_message.forget();

        wasm_bindgen_futures::spawn_local(async move {
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
) -> std::result::Result<Closure<dyn FnMut(web_sys::ExtendableMessageEvent)>, JsValue> {


    let reg = _global.registration();

    // let clients = _global.clients();
    console::log_1(&JsValue::from_str("serviceworker on_message()"));
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
                if &client_version == &version.clone() {
                    tracing::info!("PING: SAME VERSION. WELCOME NEW TAB.");
                } else {
                    tracing::info!("PING: DIFFERENT VERSION. MUST SEPPUKKU NOW.");
                    seppukku(reg);
                }

                let data2 = WorkerMessage {
                    msg_type: "pong".to_string(),
                    msg_content: version,
                };
                let data2 = serde_wasm_bindgen::to_value(&data2).expect("serialize");
                

                match client.post_message(&data2) {
                    Ok(_i)=>{}
                    Err(e) => {
                        tracing::warn!("CANNOT POST MESSAGE TO CLIENT! {:#?}", e);
                    }
                }
            } else {
                tracing::info!("got other tpye of message? TODO!");
                wasm_bindgen_futures::spawn_local(async move {
                    let request = data.clone();
                    let response = compute_response_message(request).await;
                    let response = serde_wasm_bindgen::to_value(&response).expect("serialize");
                    match client.post_message(&response) {
                        Ok(_i)=>{}
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

fn seppukku(
    _global: web_sys::ServiceWorkerRegistration) {
        let _w = _global.update();
        match _w {
            Ok(p) => {wasm_bindgen_futures::spawn_local(async move {
                tracing::info!("SEPPUKKU SENDING UPDATE RESULT: {:?}", p.await);
            })}
            Err(e) => {
                tracing::error!("FAILED TO SEPPUKKU! {e:#?}");
            }
        }
    }

#[tracing::instrument()]
async fn worker_loop() -> anyhow::Result<()> {
    use gloo_timers::future::TimeoutFuture;
    let mut i = 3000;
    loop {
        TimeoutFuture::new(i).await; i *=2;
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
    tracing::info!("worker_iteration crack smoker init. timestamp = {}", get_timestamp_now_ms());


    Ok(())
}

pub fn get_timestamp_now_ms() -> i64 {
    chrono::offset::Utc::now().timestamp_millis()
}



fn get_version(_global: ServiceWorkerGlobalScope) -> Option<String> {
    // let global = js_sys::global();

    const KEY : &str = "__wasm_worker_md5";
    let key = js_sys::eval(&KEY.to_string()).unwrap_or_default().as_string();
key
}


// =======================

// use std::cell::RefCell;
// use std::rc::Rc;
// use wasm_bindgen::prelude::*;
// use web_sys::{console, HtmlElement, HtmlInputElement, MessageEvent, Worker};

// /// A number evaluation struct
// ///
// /// This struct will be the main object which responds to messages passed to the
// /// worker. It stores the last number which it was passed to have a state. The
// /// statefulness is not required in this example but should show how
// /// larger, more complex scenarios with statefulness can be set up.
// #[wasm_bindgen]
// pub struct NumberEval {
//     number: i32,
// }

// #[wasm_bindgen]
// impl NumberEval {
//     /// Create new instance.
//     pub fn new() -> NumberEval {
//         NumberEval { number: 0 }
//     }

//     /// Check if a number is even and store it as last processed number.
//     ///
//     /// # Arguments
//     ///
//     /// * `number` - The number to be checked for being even/odd.
//     pub fn is_even(&mut self, number: i32) -> bool {
//         self.number = number;
//         self.number % 2 == 0
//     }

//     /// Get last number that was checked - this method is added to work with
//     /// statefulness.
//     pub fn get_last_number(&self) -> i32 {
//         self.number
//     }
// }

// /// Run entry point for the main thread.
// #[wasm_bindgen]
// pub fn startup() {
//     // Here, we create our worker. In a larger app, multiple callbacks should be
//     // able to interact with the code in the worker. Therefore, we wrap it in
//     // `Rc<RefCell>` following the interior mutability pattern. Here, it would
//     // not be needed but we include the wrapping anyway as example.
//     let worker_handle = Rc::new(RefCell::new(Worker::new("./assets/scripts/worker.js").unwrap()));
//     console::log_1(&"Created a new worker from within Wasm".into());

//     // Pass the worker to the function which sets up the `oninput` callback.
//     setup_input_oninput_callback(worker_handle);
// }

// fn setup_input_oninput_callback(worker: Rc<RefCell<web_sys::Worker>>) {
//     let document = web_sys::window().unwrap().document().unwrap();

//     // If our `onmessage` callback should stay valid after exiting from the
//     // `oninput` closure scope, we need to either forget it (so it is not
//     // destroyed) or store it somewhere. To avoid leaking memory every time we
//     // want to receive a response from the worker, we move a handle into the
//     // `oninput` closure to which we will always attach the last `onmessage`
//     // callback. The initial value will not be used and we silence the warning.
//     #[allow(unused_assignments)]
//     let mut persistent_callback_handle = get_on_msg_callback();

//     #[allow(unused_assignments)]
//     let callback = Closure::new(move || {
//         console::log_1(&"oninput callback triggered".into());
//         let document = web_sys::window().unwrap().document().unwrap();

//         let input_field = document
//             .get_element_by_id("inputNumber")
//             .expect("#inputNumber should exist");
//         let input_field = input_field
//             .dyn_ref::<HtmlInputElement>()
//             .expect("#inputNumber should be a HtmlInputElement");

//         // If the value in the field can be parsed to a `i32`, send it to the
//         // worker. Otherwise clear the result field.
//         match input_field.value().parse::<i32>() {
//             Ok(number) => {
//                 // Access worker behind shared handle, following the interior
//                 // mutability pattern.
//                 let worker_handle = &*worker.borrow();
//                 let _ = worker_handle.post_message(&number.into());
//                 persistent_callback_handle = get_on_msg_callback();

//                 // Since the worker returns the message asynchronously, we
//                 // attach a callback to be triggered when the worker returns.
//                 worker_handle
//                     .set_onmessage(Some(persistent_callback_handle.as_ref().unchecked_ref()));
//             }
//             Err(_) => {
//                 document
//                     .get_element_by_id("resultField")
//                     .expect("#resultField should exist")
//                     .dyn_ref::<HtmlElement>()
//                     .expect("#resultField should be a HtmlInputElement")
//                     .set_inner_text("");
//             }
//         }
//     });

//     // Attach the closure as `oninput` callback to the input field.
//     document
//         .get_element_by_id("inputNumber")
//         .expect("#inputNumber should exist")
//         .dyn_ref::<HtmlInputElement>()
//         .expect("#inputNumber should be a HtmlInputElement")
//         .set_oninput(Some(callback.as_ref().unchecked_ref()));

//     // Leaks memory.
//     callback.forget();
// }

// /// Create a closure to act on the message returned by the worker
// fn get_on_msg_callback() -> Closure<dyn FnMut(MessageEvent)> {
//     Closure::new(move |event: MessageEvent| {
//         console::log_2(&"Received response: ".into(), &event.data());

//         let result = match event.data().as_bool().unwrap() {
//             true => "even",
//             false => "odd",
//         };

//         let document = web_sys::window().unwrap().document().unwrap();
//         document
//             .get_element_by_id("resultField")
//             .expect("#resultField should exist")
//             .dyn_ref::<HtmlElement>()
//             .expect("#resultField should be a HtmlInputElement")
//             .set_inner_text(result);
//     })
// }