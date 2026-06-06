use dioxus::prelude::*;


#[component]
pub fn TableContentPane(table: String) -> Element {
    rsx! {
        div {
            style: "
                flex-grow: 1;
                max-width: 1000px;
                height: 100%;
                border: 1px solid red;
            ",

        }
    }
}

