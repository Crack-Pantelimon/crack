use crack::storage_crackhouse::api::ExecuteSQL2;
use crack::web_serviceworker_loader::WebWorkerFactory;
use crack::{
    api_asscrack::api::{api_client::ApiClient, api_worker_declarations::WorkerPing},
    storage_crackhouse::api::{ExecuteSQL, RusquliteTest},
};
use dioxus::{logger::tracing, prelude::*};

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

    // c.call::<RusquliteTest>(()).await?;
    // let _r = c
    // .call::<ExecuteSQL>("SELECT 1 + 1 FROM PERSON".to_string())
    // .await?;
    // tracing::info!("{}", _r);

    Ok(c)
}

#[component]
fn App() -> Element {
    tracing::info!("App()");

    let web_worker = use_resource(move || async move { get_crack().await });

    let web_worker_status = match web_worker.read().as_ref() {
        None => rsx! {h1{"Loading..."}},
        Some(Err(e)) => rsx! {pre{"Error: {e:#?}"}},
        Some(Ok(_v)) => rsx! {"OK"},
    };

    let web_sql_editor = match web_worker.read().as_ref() {
        Some(Ok(client)) => {
            let _client: Signal<ApiClient> = use_signal(move || client.clone());
            rsx! {
                ShowSQLEditor{
                    _client: _client
                }
            }
        }
        _ => rsx! {},
    };

    rsx! {
        document::Script {
            src:asset!("/assets/scripts/v2/crack2-client.js",  AssetOptions::js()
        .with_minify(false)
        .with_hash_suffix(false)), "type": "module",
        }
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        br{}
        Hero {}
        br{}
        {web_worker_status}

        br{}

        {web_sql_editor}
    }
}

#[component]
pub fn ShowSQLEditor(_client: Signal<ApiClient>) -> Element {
    let mut result_txt = use_signal(|| "".to_string());
    let mut result_txt2 = use_signal(|| "".to_string());
    let mut sql_txt = use_signal(|| "".to_string());

    let coro = use_coroutine(move |mut rx: UnboundedReceiver<String>| async move {
        while let Ok(sql) = rx.recv().await {
            let _r = _client.read().clone().call::<ExecuteSQL>(sql.clone()).await;
            let _r = match _r {
                Ok(s) => s,
                Err(e) => format!("{e:?}"),
            };
            result_txt.set(_r);

            let _r = _client.read().clone().call::<ExecuteSQL2>(sql).await;
            let _r = match _r {
                Ok(s) => s,
                Err(e) => format!("{e:?}"),
            };
            result_txt2.set(_r);
        }
        tracing::error!("coro exit.");
    });

    rsx! {
        h4 {"QUERY"}
        textarea {
            onchange: move |_d: Event<FormData>| {
                let d = _d.data().value();
                sql_txt.set(d);
            }
        }
        h5 {"SEND"}
        button {
            onclick: move |_| {
                coro.send(sql_txt.read().clone());

            },
            "SEND SQL"

        }
        h4 {"RESULTS1"}
        pre {
            style: "text-wrap: wrap;",
            {result_txt}
        }
        h4 {"RESULTS2"}
        pre {
            style: "text-wrap: wrap;",
            {result_txt2}
        }

    }
}

#[component]
pub fn Hero() -> Element {
    tracing::info!("Hero()");

    let mut i = use_signal(|| 0);

    rsx! {
        div {
            id: "hero",
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
