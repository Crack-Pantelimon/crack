use crack::storage_crackhouse::{
    api::{ExecuteSQL2, ExecuteSQLParams, SQLAndParams},
    basic::DbValue,
};
use dioxus::{logger::tracing, prelude::*};

use crate::{
    components::display_table::{DefaultTableRenderer, DisplayTable},
    crack::use_crack,
};

#[component]
pub fn TableContentPane(table: ReadSignal<String>) -> Element {
    let sql = "SELECT * FROM pragma_table_info(?);";

    let result = use_resource(move || {
        let param = DbValue::Text(table.read().clone());
        tracing::info!("TableContentPane {param:?}");

        let arg = SQLAndParams {
            sql: sql.to_string(),
            params: vec![param],
        };

        let arg2 = arg.clone();
        async move {
            let api = use_crack();
            api.call::<ExecuteSQLParams>(arg2.clone()).await
        }
    });

    let result = result.read();
    let Some(result) = result.as_ref() else {
        return rsx! {"Loading"};
    };
    let result = match result {
        Ok(result) => result,
        Err(e) => return rsx! {pre{"Error {e:?}!"}},
    };

    rsx! {
        div {
            style: "
                flex-grow: 1;
                height: 100%;
                border: 1px solid red;
            ",
                    DisplayTable::<DefaultTableRenderer> {
                        data: result.clone(),
                        renderer: DefaultTableRenderer,
                    }

        }
    }
}
