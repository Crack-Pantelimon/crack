use super::display_table::*;
use crack::storage_crackhouse::{api::ExecuteSQL2, basic::DbValue};
use dioxus::prelude::*;

use crate::{crack::use_crack, route::Route};

#[component]
pub fn TableListPane(selected_table: ReadSignal<Option<String>>) -> Element {
    let sql = "SELECT  sqlite_master.tbl_name as name, SUM(dbstat.pgsize) / 1024 as size_kb
FROM sqlite_master INNER JOIN dbstat ON dbstat.name = sqlite_master.name 
GROUP BY sqlite_master.tbl_name ;";
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
                to: Route::SqlQuery,
                "SqlQuery"
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
        match value {
            DbValue::Text(value) => {
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

            DbValue::Integer(value) => {
                rsx! {"{value}"}
            }
            value => DefaultTableRenderer.render(_name, value),
        }
    }
}
