use dioxus::prelude::*;

use crate::components::{db_sql_repl::SqlRepl, db_table_content::TableContentPane, db_table_list::TableListPane};

#[component]
pub fn HomePage() -> Element {
    rsx! {
        TableListPane{selected_table: None}
        SqlRepl{}
    }
}

#[component]
pub fn TableViewPage(table: ReadSignal<String>) -> Element {
    rsx! {
        TableListPane{selected_table: Some(table.read().clone())}
        TableContentPane{table: table.read().clone()}
    }
}
