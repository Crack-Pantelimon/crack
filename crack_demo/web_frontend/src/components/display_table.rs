use crack::storage_crackhouse::basic::{DbValue, SqlResultSet};
use dioxus::prelude::*;

pub trait TableCellRenderer: PartialEq + Clone + 'static {
    fn render(&self, _column_name: &str, value: DbValue) -> Element;
}

#[derive(Clone, PartialEq)]
pub struct DefaultTableRenderer;
impl TableCellRenderer for DefaultTableRenderer {
    fn render(&self, _name: &str, value: DbValue) -> Element {
        let max_len = 96;
        match value {
            DbValue::Integer(v) => {
                rsx! {"{v}"}
            }
            DbValue::Real(v) => {
                rsx! {"{v}"}
            }
            DbValue::Text(v) => {
                let n = v.len();
                let mut v = v.as_bytes().to_vec();
                v.truncate(max_len);
                let v = String::from_utf8_lossy(&v).to_string();

                let mut ext = String::new();
                if n > v.len() {
                    ext = format!("... [size={}]", n);
                }
                rsx! {"{v}", i{style:"color:grey;", {ext}}}
            }
            DbValue::Blob(mut v) => {
                let n = v.len();
                v.truncate(max_len / 3);
                let v = v
                    .iter()
                    .map(|x| format!("{:#x}", *x))
                    .collect::<Vec<_>>()
                    .join("");

                let mut ext = String::new();
                if n > v.len() {
                    ext = format!("... [size={}]", n);
                };
                rsx! {"{v}", i{style:"color:grey;", {ext}}}
            }
            DbValue::Null => {
                rsx! {i {
                    style: "color:grey;",
                    "NULL"
                }}
            }
        }
    }
}

#[component]
pub fn DisplayTable<R: TableCellRenderer>(
    data: ReadSignal<SqlResultSet>,
    renderer: ReadSignal<R>,
) -> Element {
    rsx! {
        table {
            thead {
                tr {
                    for col in data.read().column_names.iter() {
                        th {
                            "{col}"
                        }
                    }
                }
            }
            tbody {
                for row in data.read().rows.iter() {

                    tr {
                        for (value, col_name) in row.cols.iter().zip(data.read().column_names.iter()) {
                            td {
                                {renderer.read().render(col_name, value.clone())}
                            }
                        }
                    }
                }
            }
        }
    }
}
