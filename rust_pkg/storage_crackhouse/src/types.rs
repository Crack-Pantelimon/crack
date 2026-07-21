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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_db_value_serde_roundtrip() {
        let values = vec![
            DbValue::Null,
            DbValue::Integer(42),
            DbValue::Real(3.14),
            DbValue::Text("hello".into()),
            DbValue::Blob(vec![1, 2, 3]),
        ];
        for v in values {
            let json = serde_json::to_string(&v).unwrap();
            let back: DbValue = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn smoke_result_set_serde_roundtrip() {
        let set = SqlResultSet {
            column_names: vec!["id".into(), "name".into()],
            rows: vec![SqlResultRow {
                cols: vec![DbValue::Integer(1), DbValue::Text("a".into())],
            }],
        };
        let json = serde_json::to_string(&set).unwrap();
        let back: SqlResultSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.column_names, set.column_names);
        assert_eq!(back.rows.len(), 1);
        assert_eq!(back.rows[0].cols[0], DbValue::Integer(1));
    }

    #[test]
    fn smoke_db_value_conversions() {
        assert_eq!(DbValueType::Integer.to_sql_str(), "Integer");
        assert_eq!(DbValueType::Text.to_sql_str(), "Text");

        let i: i64 = DbValue::Integer(7).try_into().unwrap();
        assert_eq!(i, 7);

        let none: Option<String> = DbValue::Null.try_into().unwrap();
        assert_eq!(none, None);

        assert!(i64::try_from(DbValue::Text("x".into())).is_err());
    }
}
