use super::display_table::*;
use crack::storage_crackhouse::{
    api::ExecuteSQL2,
    basic::{DbValue, SqlResultSet},
};
use dioxus::{logger::tracing, prelude::*};

use crate::{crack::use_crack, route::Route};

#[component]
pub fn TableListPane(selected_table: ReadSignal<Option<String>>) -> Element {
    let sql = "SELECT name AS 'Table Name' FROM sqlite_master WHERE type='table';";
    let result = use_resource(move || async move {
        let api = use_crack();
        api.call::<ExecuteSQL2>(sql.to_string()).await
    })
    .suspend()?;
    let result = result.read();
    let result = result.as_ref();
    let result = result.map_err(|e| anyhow::anyhow!("SQL TABLE LIST PANE ERROR! {e:#?}"))?;


    rsx! {
        div {
            style: "
            width: 350px;
            border: 1px solid red;
            height: 100%;
            display: flex; flex-direction: column;
            ",
            Link {
                style: "padding: 1vmin",
                to: Route::HomePage,
                "HomePage"
            },

            DisplayTable::<LinkRenderer> {data: result.clone(), renderer: LinkRenderer{selected: selected_table.read().cloned().unwrap_or_default()}}
        }
    }
}



#[derive(Clone, PartialEq)]
struct LinkRenderer {
    pub selected: String,
}
impl TableCellRenderer for LinkRenderer {
    fn render(&self, _name: &str, value: DbValue) -> Element {

        let DbValue::Text(value) = value else {
            return Err(RenderError::from(anyhow::anyhow!(
                "link render: not a text"
            )));
        };

        let selected = self.selected == value;
        let color = if selected { "black" } else { "blue" };
        
        // tracing::info!("render {} ({} , {} ) {} {}", selected, self.selected, _name, _name, value);
        rsx! {
            Link {
                to: Route::TableViewPage { table: value.clone() }.to_string(),
                style: "color: {color};",
                "{value}"
            }
        }
    }
}
