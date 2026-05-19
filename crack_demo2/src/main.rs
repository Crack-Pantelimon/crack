use dioxus::{logger::tracing, prelude::*};
use web_serviceworker_crackloader::WebWorkerFactory;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const HEADER_SVG: Asset = asset!("/assets/header.svg");

#[used]
static WORKER_JS: Asset = asset!(
    "/assets/pkg_web_serviceworker/web_serviceworker_crackslave.js",
    AssetOptions::js()
        .with_minify(false)
        .with_hash_suffix(false)
);
// #[used]
// static INDEX_JS : Asset = asset!(
//     "/assets/scripts/index.js",
//     AssetOptions::js().with_minify(false).with_hash_suffix(false)
// );

#[used]
static SCRIPT_FOLDER: Asset = asset!(
    "/assets/scripts",
    AssetOptions::folder().with_hash_suffix(false)
);

#[used]
static WORKER_FOLDER: Asset = asset!(
    "/assets/pkg_web_serviceworker",
    AssetOptions::folder().with_hash_suffix(false)
);

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    tracing::info!("App()");
    // let script_wasm = String::from_utf8_lossy( include_bytes!("../assets/pkg_web_serviceworker/web_serviceworker_crackslave.js")).to_string();
    // let script_launch = String::from_utf8_lossy( include_bytes!("../assets/scripts/index.js")).to_string();

    let web_worker = use_resource(move || async move {
        let opt = WebWorkerFactory {
            worker_url: "/assets/scripts/worker.js".to_string(),
            worker_type: "classic".to_string(),
            worker_scope: "/assets/scripts/".to_string(),
            version: String::from_utf8_lossy(include_bytes!(
                "../assets/pkg_web_serviceworker/md5.txt"
            ))
            .trim()
            .to_string(),
        };
        use crack::api_asscrack::crack_worker::WorkerLoaderFactory;
        let _active = opt.load_worker().await;

        if let Err(e) = &_active {
            tracing::error!("Error with web worker: {e:?}");
        } else {
            tracing::info!("Web worker init OK.");
        }

        _active
    });

    let web_worker_status = match web_worker.read().as_ref() {
        None => rsx! {h1{"Loading..."}},
        Some(Err(e)) => rsx! {pre{"Error: {e:#?}"}},
        Some(Ok(_v)) => rsx! {"OK"},
    };

    rsx! {
            document::Link { rel: "icon", href: FAVICON }
            document::Link { rel: "stylesheet", href: MAIN_CSS }

            // document::Script {"type": "module", src:format!("{WORKER_FOLDER}/web_serviceworker_crackslave.js")}
            // document::Script {"type": "module", src:WORKER_JS}
            // document::Script {"type": "module", src:INDEX_JS}
            // document::Script {"type": "module", src:"/public/pkg_web_serviceworker/web_serviceworker_crackslave.js"}
            // document::Script {"type": "module", src:"/public/scripts/index.js"}

            // document::Script {"type": "module", src:format!("{SCRIPT_FOLDER}/index.js")}
    //      {script_wasm}
            // document::Script {
            //     "


            //     {script_launch}
            //     "
            // }

            Hero {}
            {web_worker_status}
            // pre {
            //     "{worker():#?}"
            // }

        }
}

#[component]
pub fn Hero() -> Element {
    tracing::info!("Hero()");

    let mut i = use_signal(|| 0);
    // // let mut _loader_state = use_signal(|| None);
    // use_effect(move || {
    //     tracing::info!("CRACK DEMO!");
    //     dioxus::logger::tracing::error!("EFFECT()");
    //     crack::storage_crackhouse::init();

    //     let _ = spawn(async move {
    //         tracing::info!("CRACKLOADER!");
    //         let _loader = web_serviceworker_crackloader::register_service_worker(
    //             format!("{WORKER_FOLDER}/web_serviceworker_crackslave.js"),
    //             "classic".to_string(),
    //             "/".to_string(),
    //         )
    //         .await;
    //     dioxus::logger::tracing::error!("RESULT: {:#?}", _loader);
    //         // _loader_state.set(Some(_loader));
    //     });
    // });

    rsx! {
        div {
            id: "hero",
            img { src: HEADER_SVG, id: "header" }
            div { id: "links",
                button {
                    onclick: move |_| {
                        *i.write() += 1;

                    },
                    "CLICK ME {i()}",
                }
            }

        }
    }
}
