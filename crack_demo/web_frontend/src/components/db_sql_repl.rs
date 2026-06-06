use crack::storage_crackhouse::api::ExecuteSQL2;
use dioxus::prelude::*;

use crate::{components::display_table::{DefaultTableRenderer, DisplayTable}, crack::use_crack};

#[component]
pub fn SqlRepl() -> Element {
    let mut sql = use_signal(String::new);
    let mut result = use_signal(|| None);
    let mut error = use_signal(String::new);

    let c = use_coroutine(move |mut r| {
        async move {
            while let Ok(sql) = r.recv().await {
                let api = use_crack();
                result.set(None);
                error.set(String::new());
                
                let r = api.call::<ExecuteSQL2>(sql).await;

                match r {
                    Ok(r) => {
                        result.set(Some(r));
                    }
                    Err(e) => {
                        error.set(format!("{e:?}"));
                    }
                }
            }
        }
    });
    rsx! {
        div {
            style: "
                flex-grow: 1;
                height: 100%;
                border: 1px solid red;
                display:flex;
                flex-direction:column;
            ",


            div {
                style:"display:flex;flex-direction:row;width:100%;height:fit-content;",
                textarea {
                    placeholder:"Write SQL Here ....",
                    style: "
                        padding: 1cqmin; margin: 1cqmin;
                        width: 80%; border: 1px solid black;
                    ", 
                    oninput: move |e| sql.set(e.value())
                }

                button {
                    onclick: move |_e| {
                        c.send(sql.read().clone());
                    },
                    "SEND SQL"
                }
            }

            if let Some(r) = result.read().clone() {
                div {
                    style: "
                    width: 100%;
                    flex-grow: 1;
                    overflow: scroll;
                    ",

                    DisplayTable::<DefaultTableRenderer> {
                        data: r,
                        renderer: DefaultTableRenderer,
                    }
                }
            }
            if !error.read().is_empty() {
                pre {
                    style:"color:darkred; width: 60%; text-wrap: wrap;",
                    "{error()}"
                }
            }
        }
    }
}
