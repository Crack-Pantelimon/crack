use api_asscrack::declare_api_group2;
use api_asscrack::implement_api_group2;
use serde::Deserialize;
use serde::Serialize;

use crate::basic::DbValue;
use crate::basic::SqlResultSet;

declare_api_group2! {
    StorageCrackhouseApiGroup,
    [
        // (ExecuteSQL, String, String),
        (ExecuteSQLParams, SQLAndParams, SqlResultSet),
        (ExecuteSQL2, String, SqlResultSet),
        // (RusquliteTest, (), ()),
    ]
}

implement_api_group2! {
    StorageCrackhouseApiGroup,
    [

        // (ExecuteSQL, execute_sql),
        (ExecuteSQLParams, execute_sql_params),
        (ExecuteSQL2, execute_sql2),
        // (RusquliteTest, rusqulite_test),
    ]
}

// pub async fn rusqulite_test(_t: ()) -> anyhow::Result<()> {
//     let _x = crate::impl_rusqulite::run_test_person()?;

//     let _y = crate::basic::demo_main_seaorm()?;

//     Ok(())
// }

// pub async fn execute_sql(sql: String) -> anyhow::Result<String> {
//     crate::impl_rusqulite::sql_inject(sql)
// }
pub async fn execute_sql2(sql: String) -> anyhow::Result<SqlResultSet> {
    crate::basic::execute_sql2(sql, vec![])
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SQLAndParams {
    pub sql: String,
    pub params: Vec<DbValue>,
}

pub async fn execute_sql_params(req: SQLAndParams) -> anyhow::Result<SqlResultSet> {
    crate::basic::execute_sql2(req.sql, req.params)
}
