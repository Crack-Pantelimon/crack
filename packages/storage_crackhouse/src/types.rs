use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SQLAndParams {
    pub sql: String,
    pub params: Vec<DbValue>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum DbValueType {
    Null,
    Integer,
    Real,
    Text,
    Blob,
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
impl DbValue {
    pub fn fold_option(value: Option<DbValue>) -> DbValue {
        match value {
            None => Self::Null,
            Some(v) => v,
        }
    }
}

impl From<i64> for DbValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}
impl From<String> for DbValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<f64> for DbValue {
    fn from(value: f64) -> Self {
        Self::Real(value)
    }
}
impl From<Vec<u8>> for DbValue {
    fn from(value: Vec<u8>) -> Self {
        Self::Blob(value)
    }
}
impl<T> From<Option<T>> for DbValue
where
    DbValue: From<T>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            None => Self::Null,
            Some(value) => DbValue::from(value),
        }
    }
}

impl TryFrom<DbValue> for i64 {
    type Error = anyhow::Error;
    fn try_from(value: DbValue) -> Result<i64, Self::Error> {
        match value {
            DbValue::Integer(value) => Ok(value),
            _ => anyhow::bail!("cannot convert"),
        }
    }
}

impl TryFrom<DbValue> for f64 {
    type Error = anyhow::Error;
    fn try_from(value: DbValue) -> Result<f64, Self::Error> {
        match value {
            DbValue::Real(value) => Ok(value),
            _ => anyhow::bail!("cannot convert"),
        }
    }
}

impl TryFrom<DbValue> for String {
    type Error = anyhow::Error;
    fn try_from(value: DbValue) -> Result<String, Self::Error> {
        match value {
            DbValue::Text(value) => Ok(value),
            _ => anyhow::bail!("cannot convert"),
        }
    }
}

impl TryFrom<DbValue> for Vec<u8> {
    type Error = anyhow::Error;
    fn try_from(value: DbValue) -> Result<Vec<u8>, Self::Error> {
        match value {
            DbValue::Blob(value) => Ok(value),
            _ => anyhow::bail!("cannot convert"),
        }
    }
}

trait DbPrimitive {}
impl DbPrimitive for String {}
impl DbPrimitive for i64 {}
impl DbPrimitive for f64 {}
impl DbPrimitive for Vec<u8> {}

impl<T> TryFrom<DbValue> for Option<T>
where
    T: TryFrom<DbValue> + DbPrimitive,
{
    type Error = anyhow::Error;

    fn try_from(value: DbValue) -> Result<Option<T>, Self::Error> {
        match value {
            DbValue::Null => Ok(None),
            _z => DbValue::try_into(_z),
        }
    }
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
