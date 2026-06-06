use crate::pages::*;
use dioxus::prelude::*;
#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[route("/")]
    HomePage,

    #[route("/tables/:table")]
    TableViewPage { table: String },
    // #[route("/about")]
    // About,

    // #[route("/user/:id")]
    // User { id: u32 },
}
