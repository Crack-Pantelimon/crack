use _crack_utils::sleep_ms;

use api_asscrack::declare_api_group2;
use api_asscrack::implement_api_group2;

declare_api_group2! {
    StorageCrackhouseApiGroup,
    [
        (ExecuteSQL, String, String),
        (ExecuteSQL2, String, String),
        (RusquliteTest, (), ()),
    ]
}

implement_api_group2! {
    StorageCrackhouseApiGroup,
    [

        (ExecuteSQL, execute_sql),
        (ExecuteSQL2, execute_sql2),
        (RusquliteTest, rusqulite_test),
    ]
}

pub async fn rusqulite_test(_t: ()) -> anyhow::Result<()> {
    let _x = crate::impl_rusqulite::run_test_person()?;


    let _y = crate::basic::demo_main_seaorm()?;

    Ok(())
}

pub async fn execute_sql(sql: String) -> anyhow::Result<String> {
    crate::impl_rusqulite::sql_inject(sql)
}
pub async fn execute_sql2(sql: String) -> anyhow::Result<String> {
    crate::basic::execute_sql2(sql)
}
