use std::sync::{Once, OnceLock};

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};

mod demo_sql;
mod entity;
mod mutation;
mod query;

use entity::*;
use mutation::*;
use query::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

fn db_conn() -> anyhow::Result<DatabaseConnection> {
    // ON WASM
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    const FILE: &str = "sqlite:file:/assets/scripts/post2.db?vfs=opfs-sahpool";

    // ON NON-WASM
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    const FILE: &str = "sqlite:post2.db";

    // let rs = rusqlite::Connection::open(FILE)?;

    tracing::info!("opening db : {FILE}");
    let db = Database::connect(FILE)?;

    static MIGRATE_ONCE: Once = Once::new();
    MIGRATE_ONCE.call_once(|| {
        tracing::info!("RUN MIGRATIONS!");
        match db.get_schema_registry("storage_crackhouse::*").sync(&db) {
            Ok(_o) => {}
            Err(e) => {
                tracing::error!("FAILED TO RUN MIGRATIONS!!!");
                tracing::error!("MIGRATIONS ERROR: {e:#?}");
            }
        }
    });

    tracing::info!("DB OPENED OK: {db:?}");
    Ok(db)
}

// static DB_LOCK2: std::sync::OnceLock<anyhow::Result<rusqlite::Connection>> = OnceLock::new();
static DB_LOCK: std::sync::OnceLock<anyhow::Result<DatabaseConnection>> = OnceLock::new();

// static DB2: std::cell::OnceCell<anyhow::Result<DatabaseConnection>> = OnceCell::new();

// static MIGRATE_ONCE: tokio::sync::OnceCell<Arc<DatabaseConnection>> = tokio::sync::OnceCell::new();

pub fn connect_sqlite_db() -> anyhow::Result<&'static DatabaseConnection> {
    let t: &'static DatabaseConnection = DB_LOCK
        .get_or_init(db_conn)
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e:#?}"))?;
    // DB_LOCK.as_ref().map_err(|e| anyhow::anyhow!("{e:?}"))
    Ok(t)
}

pub fn demo_main_seaorm() -> anyhow::Result<()> {
    tracing::info!("Running: demo_main_seaorm()");

    let db = connect_sqlite_db()?;

    tracing::info!("DB SYNC CREATE TABLES ETC ALL OK.");

    tracing::info!("{db:?}\n");

    let _r = db.execute_unprepared(demo_sql::DEMO_SQL);
    tracing::info!("EXECUTE SQL: {:#?}", _r);

    tracing::info!("===== =====\n");

    all_about_query(&db)?;

    tracing::info!("===== =====\n");

    all_about_mutation(&db)?;

    Ok(())
}

pub fn execute_sql2(sql: String) -> anyhow::Result<SqlResultSet> {
    let db = connect_sqlite_db()?;
    tracing::info!("EXECUTE SQL CODE: {}", &sql);

    let _r = db.query_all_raw(Statement::from_string(DbBackend::Sqlite, &sql))?;
    tracing::info!("EXECUTE SQL RESULT: {:#?}", _r);

    let mut x = SqlResultSet {
        column_names: vec![],
        rows: vec![],
    };

    for (_i, r) in _r.iter().enumerate() {
        tracing::info!("deserialize row {_i}");
        if x.column_names.is_empty() {
            x.column_names = r.column_names();
            tracing::info!("column names = {:?}", &x.column_names);
        }
        let mut column_vals = vec![];
        for (j, _col) in x.column_names.iter().enumerate() {
            tracing::info!("column index {j} {_col}");
            let val = _get_query_value(r, j);
            tracing::info!("column index {j} {_col} = {val:?}");

            column_vals.push(val);
        }
        x.rows.push(SqlResultRow { cols: column_vals })
    }

    Ok(x)
}

fn _get_query_value(r: &sea_orm::QueryResult, j: usize) -> DbValue {
    if let Ok(r) = r.try_get_by_index::<i64>(j) {
        return DbValue::Integer(r);
    };

    if let Ok(r) = r.try_get_by_index::<f64>(j) {
        return DbValue::Real(r);
    };

    if let Ok(r) = r.try_get_by_index::<String>(j) {
        return DbValue::Text(r);
    };

    if let Ok(r) = r.try_get_by_index::<Vec<u8>>(j) {
        return DbValue::Blob(r);
    };

    DbValue::Null
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SqlResultSet {
    pub column_names: Vec<String>,
    pub rows: Vec<SqlResultRow>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SqlResultRow {
    pub cols: Vec<DbValue>,
}

// impl SqlResultSet {
//     pub fn deserialize<T: DeserializeOwned>(&self) -> anyhow::Result<Vec<T>> {
//         let mut objs = vec![];
//         for row in self.rows.iter() {
//             let mut obj = serde_json::map::Map::new();
//             for ((_j, col), value) in self.column_names.iter().enumerate().zip(row.cols.iter()) {
//                 obj.insert(col.to_string(), value.clone());
//             }
//             let val = serde_json::Value::Object(obj);
//             objs.push(val);
//         }
//         let objs = serde_json::Value::Array(objs);

//         let t = serde_json::from_value(objs)?;
//         Ok(t)
//     }
// }

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum DbValue {
    /// The value is a `NULL` value.
    Null,
    /// The value is a signed integer.
    Integer(i64),
    /// The value is a floating point number.
    Real(f64),
    /// The value is a text string.
    Text(String),
    /// The value is a blob of data
    Blob(Vec<u8>),
}
