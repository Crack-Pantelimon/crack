use rusqlite::{Connection, Result, types::Value};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::types::{DbValue, SQLAndParams, SqlResultRow, SqlResultSet};

fn _new_connection() -> Result<Connection> {
    // ON WASM
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    const FILE: &str = "file:/assets/scripts/post3.db?vfs=opfs-sahpool";

    // ON NON-WASM
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    const FILE: &str = "post3.db";

    Connection::open(FILE)
}

lazy_static::lazy_static! {
pub static ref CONN: Arc<Mutex<Result<Connection>>> = Arc::new(Mutex::new(_new_connection()));
}

pub async fn sql_query(sql: SQLAndParams) -> anyhow::Result<SqlResultSet> {
    let conn = CONN.lock().await;
    let conn = conn
        .as_ref()
        .map_err(|e| anyhow::anyhow!("Error fetching SQL lock: {e:?}"))?;
    // let conn = conn.as_ref().map_err(|e| anyhow::anyhow!("Error obtaining SQL connection: {e:?}"))?;
    let mut _stmt = conn.prepare(&sql.sql)?;
    let column_count = _stmt.column_count();

    let mut r = SqlResultSet {
        column_names: vec![],
        rows: vec![],
    };

    r.column_names = _stmt
        .column_names()
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();

    for (i, param) in sql.params.into_iter().enumerate() {
        let param = Value::from(param);
        _stmt.raw_bind_parameter(1 + i, param)?;
    }
    let mut _resp = _stmt.raw_query();

    while let Some(_row) = _resp.next()? {
        let mut v = vec![];
        for j in 0..column_count {
            let col_val = _row.get_ref(j)?;
            let col_val = rusqlite::types::Value::from(col_val);
            let col_val = DbValue::from(col_val);
            v.push(col_val);
        }
        r.rows.push(SqlResultRow { cols: v });
    }

    Ok(r)
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
