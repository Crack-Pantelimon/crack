use api_asscrack::declare_api_group2;
use api_asscrack::implement_api_group2;

use crate::types::SQLAndParams;
use crate::types::SqlResultSet;

declare_api_group2! {
    StorageCrackhouseApiGroup,
    [
        (ExecuteSQLParams, SQLAndParams, SqlResultSet),
        (ExecuteSQL2, String, SqlResultSet),
    ]
}

implement_api_group2! {
    StorageCrackhouseApiGroup,
    [
        (ExecuteSQLParams, execute_sql_params),
        (ExecuteSQL2, execute_sql2),
    ]
}

/// Execute raw SQL without bound parameters.
pub async fn execute_sql2(sql: String) -> anyhow::Result<SqlResultSet> {
    crate::impl_rusqulite::sql_query(SQLAndParams {
        sql,
        params: vec![],
    })
    .await
}

/// Execute SQL with typed parameter bindings.
pub async fn execute_sql_params(req: SQLAndParams) -> anyhow::Result<SqlResultSet> {
    crate::impl_rusqulite::sql_query(req).await
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn smoke_execute_sql2_literal_select() {
        let result = execute_sql2("SELECT 41 + 1 AS answer".to_string())
            .await
            .expect("execute_sql2 on a literal SELECT should succeed");
        assert_eq!(result.column_names, vec!["answer".to_string()]);
        assert_eq!(result.rows[0].cols[0], crate::types::DbValue::Integer(42));
    }
}
