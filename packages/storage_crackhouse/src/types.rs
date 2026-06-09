use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SQLAndParams {
    pub sql: String,
    pub params: Vec<DbValue>,
}


#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum DbValueType {
Null, Integer, Real, Text, Blob
}

impl DbValueType {

pub fn to_sql_str(&self) -> &'static str {
    match self {
        Self::Null => "NULL",
        Self::Integer => "Integer",
        Self::Real => "Real",
        Self::Text => "Text",
        Self::Blob => "Blob",
    }
}
}

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

impl From<rusqlite::types::Value> for DbValue {
    fn from(value: rusqlite::types::Value) -> Self {
        match value {
            rusqlite::types::Value::Null => DbValue::Null,
            rusqlite::types::Value::Integer(a) => DbValue::Integer(a),
            rusqlite::types::Value::Real(a) => DbValue::Real(a),
            rusqlite::types::Value::Text(a) => DbValue::Text(a),
            rusqlite::types::Value::Blob(a) => DbValue::Blob(a),
        }
    }
}
impl From<DbValue> for rusqlite::types::Value {
    fn from(value: DbValue) -> Self {
        match value {
            DbValue::Null => rusqlite::types::Value::Null,
            DbValue::Integer(a) => rusqlite::types::Value::Integer(a),
            DbValue::Real(a) => rusqlite::types::Value::Real(a),
            DbValue::Text(a) => rusqlite::types::Value::Text(a),
            DbValue::Blob(a) => rusqlite::types::Value::Blob(a),
        }
    }
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
