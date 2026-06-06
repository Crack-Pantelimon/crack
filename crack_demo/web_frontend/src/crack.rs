use crack::api_asscrack::api::api_client::ApiClient;
use crack::api_asscrack::api::api_worker_declarations::WorkerPing;
use crack::web_serviceworker_loader::WebWorkerFactory;
use dioxus::logger::tracing;
use dioxus::prelude::*;

#[derive(Clone)]
pub struct CrackContext {
    pub api_client: ReadSignal<ApiClient>,
}

#[component]
pub fn ProvideCrack(children: Element) -> Element {
    let web_worker = use_resource(move || async move { init_crack().await });

    let api_client = match web_worker.read().as_ref() {
        None => {
            tracing::info!("ProvideCrack(): loading...");
            return rsx! {h1{"Loading..."}};
        }
        Some(Err(e)) => {
            tracing::error!("ProvideCrack(): error!");

            return rsx! {
                pre{
                    style: "color: darkred;",
                    "Error getting crack: {e:#?}"
                }
            };
        }
        Some(Ok(_v)) => {
            tracing::info!("ProvideCrack(): OK!");
            use_signal(move || _v.clone())
        }
    };

    use_context_provider(|| CrackContext {
        api_client: api_client.into(),
    });

    rsx! {
        {children}
    }
}

pub fn use_crack() -> ApiClient {
    use_context::<CrackContext>().api_client.peek().clone()
}

async fn init_crack() -> anyhow::Result<ApiClient> {
    tracing::info!("Get Crack!");
    let opt = WebWorkerFactory {
        // worker_url: "/assets/scripts/worker.js".to_string(),
        // worker_type: "classic".to_string(),
        // worker_scope: "/assets/scripts/".to_string(),
        // version: String::from_utf8_lossy(include_bytes!("../assets/pkg_web_serviceworker/md5.txt"))
        //     .trim()
        //     .to_string(),
    };
    use crack::api_asscrack::crack_worker::WorkerLoaderFactory;
    let _active = opt.load_worker().await?;
    tracing::info!("Got Pipe. Getting client ....");

    let c = ApiClient::new(_active);
    tracing::info!("Client OK. Sending Api PING...");
    let _r = c.call::<WorkerPing>(()).await?;
    tracing::info!("Client OK. Crack Connected!");

    Ok(c)
}
