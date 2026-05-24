use crack::{api_asscrack::api::{api_client::ApiClient, api_worker_declarations::WorkerPing}, storage_crackhouse::api::{CreatePost, ShowPosts}};
use dioxus::{logger::tracing, prelude::*};
use crack::web_serviceworker_loader::WebWorkerFactory;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const HEADER_SVG: Asset = asset!("/assets/header.svg");

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

fn main() {
    dioxus::launch(App);
}

async fn get_crack() -> anyhow::Result<ApiClient> {
    tracing::info!("Get Crack!");
    let opt = WebWorkerFactory {
        worker_url: "/assets/scripts/worker.js".to_string(),
        worker_type: "classic".to_string(),
        worker_scope: "/assets/scripts/".to_string(),
        version: String::from_utf8_lossy(include_bytes!("../assets/pkg_web_serviceworker/md5.txt"))
            .trim()
            .to_string(),
    };
    use crack::api_asscrack::crack_worker::WorkerLoaderFactory;
    let _active = opt.load_worker().await?;
    tracing::info!("Got Pipe. Getting client ....");

    let c = ApiClient::new(_active);
    tracing::info!("Client OK. Sending Api PING...");
    let _r = c.call::<WorkerPing>(()).await?;
    tracing::info!("Client OK. Crack Connected!");

    let _r = c.call::<CreatePost>(("Test".to_string(), "test".to_string())).await?;
    tracing::info!("Create Post: {_r}");

    let _r = c.call::<ShowPosts>(()).await?;
    tracing::info!("Show Posts: {_r:#?}");

    

    Ok(c)
}

#[component]
fn App() -> Element {
    tracing::info!("App()");
    // let script_launch = String::from_utf8_lossy( include_bytes!("../assets/scripts/index.js")).to_string();

    let web_worker = use_resource(move || async move { get_crack().await });

    let web_worker_status = match web_worker.read().as_ref() {
        None => rsx! {h1{"Loading..."}},
        Some(Err(e)) => rsx! {pre{"Error: {e:#?}"}},
        Some(Ok(_v)) => rsx! {"OK"},
    };

    rsx! {
            document::Link { rel: "icon", href: FAVICON }
            document::Link { rel: "stylesheet", href: MAIN_CSS }

            Hero {}
            {web_worker_status}

        }
}

#[component]
pub fn Hero() -> Element {
    tracing::info!("Hero()");

    let mut i = use_signal(|| 0);

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
