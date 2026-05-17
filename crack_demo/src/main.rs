use dioxus::{logger::tracing, prelude::*};

// #[derive(Debug, Clone, Routable, PartialEq)]
// #[rustfmt::skip]
// enum Route {
//     #[layout(Navbar)]
//     #[route("/")]
//     Home {},
//     #[route("/blog/:id")]
//     Blog { id: i32 },
// }

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const HEADER_SVG: Asset = asset!("/assets/header.svg");
const WORKER_FOLDER: Asset = asset!(
    "/assets/pkg_web_serviceworker",
    AssetOptions::folder()
        .with_hash_suffix(false)
        .into_asset_options()
);

fn main() {
     dioxus::logger::init(tracing::Level::INFO).expect("failed to init logger");
     dioxus::logger::tracing::info!("INIT APP main()");
         tracing::trace!("trace");
    tracing::debug!("debug");
    tracing::info!("info");
    tracing::warn!("warn");
    tracing::error!("error");

    dioxus::launch(App);
}

#[component]
fn App() -> Element {
     dioxus::logger::tracing::info!("App()");

    let mut _loader_state = use_signal(|| None);
    use_effect(move || {
        tracing::info!("CRACK DEMO!");
        dioxus::logger::tracing::error!("EFFECT()");
        crack::storage_crackhouse::init();

        let _ = spawn(async move {
            tracing::info!("CRACKLOADER!");
            let _loader = web_serviceworker_crackloader::register_service_worker(
                format!("{WORKER_FOLDER}/web_serviceworker_crackslave.js"),
                "".to_string(),
                "".to_string(),
            )
            .await;
            _loader_state.set(Some(_loader));
        });
    });

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        // document::Script {"type": "module", src:format!("{WORKER_FOLDER}/web_serviceworker_crackslave.js")}
        
        // Router::<Route> {}
        h1 {"SUGE-O RAMONA"}
        pre {
            "LOADER: {_loader_state():#?}"
        }
    }
}

// #[component]
// pub fn Hero() -> Element {
//     rsx! {
//         div {
//             id: "hero",
//             img { src: HEADER_SVG, id: "header" }
//             div { id: "links",

//             }
//         }
//     }
// }

// /// Home page
// #[component]
// fn Home() -> Element {
//     rsx! {
//         Hero {}
//         Echo {}
//     }
// }

// /// Blog page
// #[component]
// pub fn Blog(id: i32) -> Element {
//     rsx! {
//         div {
//             id: "blog",

//             // Content
//             h1 { "This is blog #{id}!" }
//             p { "In blog #{id}, we show how the Dioxus router works and how URL parameters can be passed as props to our route components." }

//             // Navigation links
//             Link {
//                 to: Route::Blog { id: id - 1 },
//                 "Previous"
//             }
//             span { " <---> " }
//             Link {
//                 to: Route::Blog { id: id + 1 },
//                 "Next"
//             }
//         }
//     }
// }

// /// Shared navbar component.
// #[component]
// fn Navbar() -> Element {
//     rsx! {
//         div {
//             id: "navbar",
//             Link {
//                 to: Route::Home {},
//                 "Home"
//             }
//             Link {
//                 to: Route::Blog { id: 1 },
//                 "Blog"
//             }
//         }

//         Outlet::<Route> {}
//     }
// }

// /// Echo component that demonstrates fullstack server functions.
// #[component]
// fn Echo() -> Element {
//     let mut response = use_signal(|| String::new());

//     rsx! {
//         div {
//             id: "echo",
//             h4 { "ServerFn Echo" }
//             input {
//                 placeholder: "Type here to echo...",
//                 oninput:  move |event| async move {
//                     let data = echo_server(event.value()).await.unwrap();
//                     response.set(data);
//                 },
//             }

//             if !response().is_empty() {
//                 p {
//                     "Server echoed: "
//                     i { "{response}" }
//                 }
//             }
//         }
//     }
// }

// /// Echo the user input on the server.
// #[post("/api/echo")]
// async fn echo_server(input: String) -> Result<String, ServerFnError> {
//     Ok(input)
// }
