use dioxus::{logger::tracing, prelude::*};

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const HEADER_SVG: Asset = asset!("/assets/header.svg");


#[used]
static WORKER_JS : Asset = asset!(
    "/assets/pkg_web_serviceworker/web_serviceworker_crackslave.js",
    AssetOptions::js().with_minify(false).with_hash_suffix(false)
);
// #[used]
// static INDEX_JS : Asset = asset!(
//     "/assets/scripts/index.js",
//     AssetOptions::js().with_minify(false).with_hash_suffix(false)
// );

#[used]
static SCRIPT_FOLDER: Asset = asset!(
    "/assets/scripts",
    AssetOptions::folder()
        .with_hash_suffix(false)
);

#[used]
static WORKER_FOLDER: Asset = asset!(
    "/assets/pkg_web_serviceworker",
    AssetOptions::folder()
        .with_hash_suffix(false)
);

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    tracing::info!("App()");
    // let script_wasm = String::from_utf8_lossy( include_bytes!("../assets/pkg_web_serviceworker/web_serviceworker_crackslave.js")).to_string();
    // let script_launch = String::from_utf8_lossy( include_bytes!("../assets/scripts/index.js")).to_string();


    
    let worker = use_resource(move || async move {
        let _e = spawn(async move {
            match web_serviceworker_crackloader::register_service_worker(
                "/assets/scripts/worker.js".to_string(),
                "classic".to_string(),
                "/assets/scripts/".to_string(),
            ).await {
                Ok(_) => {
                    tracing::info!("worker registration finished.")
                },
                Err(e) => {
                    tracing::error!("error running wasm service registration: {:#?}", e)
                }
            }
        });
        let version = include_bytes!("../assets/pkg_web_serviceworker/md5.txt");
        let version = String::from_utf8_lossy(version).trim().to_string();
        tracing::info!("ping to worker version = {}", version);
        let _active = web_serviceworker_crackloader::ping(version).await;

        tracing::info!("reply from ping: {:?}", _active);
        _active



    });


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
        pre {
            "{worker():#?}"
        }

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
