// mod user {
//     use sea_orm::entity::prelude::*;

//     #[sea_orm::model]
//     #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
//     #[sea_orm(table_name = "user")]
//     pub struct Model {
//         #[sea_orm(primary_key)]
//         pub id: i32,
//         pub name: String,
//     }
// }
// mod post {
//     use sea_orm::entity::prelude::*;

//     #[sea_orm::model]
//     #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
//     #[sea_orm(table_name = "post")]
//     pub struct Model {
//         #[sea_orm(primary_key)]
//         pub id: i32,
//         pub user_id: i32,
//         pub title: String,

//     }
// }
