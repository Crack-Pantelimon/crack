use dioxus::{logger::tracing, prelude::*};

use crate::crack::ProvideCrack;

const FAVICON: Asset = asset!("/assets/favicon.ico");
#[used]
static FAVICON2: Asset = asset!(
    "/assets/crack-lighter.png",
    AssetOptions::image().with_hash_suffix(false)
);
const MAIN_CSS: Asset = asset!("/assets/main.css");

#[used]
static WORKER_JS: Asset = asset!(
    "/assets/pkg_web_serviceworker/web_worker.js",
    AssetOptions::js()
        .with_minify(false)
        .with_hash_suffix(false)
);

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

#[component]
pub fn App() -> Element {
    tracing::info!("App()");

    rsx! {
        Title {"Crack!"}
        document::Script { src: asset!("/assets/scripts/storage.js") }
        document::Script {
            src: asset!(
                "/assets/scripts/v2/crack2-client.js",
                    AssetOptions::js()
                        .with_minify(false)
                        .with_hash_suffix(false)
            ),
            "type": "module",
        }
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: asset!("/assets/pico.css") }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        div {
            id: "main-div",
            style: "
                width: calc(100dvw - 2vmin);
                height: calc(100dvh - 2vmin);
                margin: 1vmin;
                padding: 1vmin;
                border: 1vmin solid black;
                container-type: size;
                display: flex;
                flex-direction: row;
                gap: 1vmin;


            ",

            ProvideCrack {
                Router::<crate::route::Route> {}
            }
        }
    }
}
