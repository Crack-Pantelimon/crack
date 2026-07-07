use std::sync::Arc;

use crate::{
    impl_rusqulite::sql_query,
    types::{DbValue, DbValueType, SQLAndParams},
};

pub trait ModelGroup: Send + Sync {
    fn grp_name(&self) -> &'static str;
    fn model_infos(&self) -> &'static [&'static dyn ModelDef];
}
// impl ModelGroup for std::sync::Arc<dyn ModelGroup> {
//     fn grp_name(&self) -> &'static str {
//         ModelGroup::grp_name(self.as_ref())
//     }

//     fn model_infos(&self) -> &'static [ModelImpl] {
//         ModelGroup::model_infos(self.as_ref())
//     }
// }

pub trait ModelDef: Send + Sync {
    fn table_name(&self) -> &'static str;
    fn model_grp(&self) -> &'static str;
    fn user_columns(&self) -> &'static [ModelColumnImpl];
    fn pk_names(&self) -> &'static [&'static str];
}

pub trait ModelSerial: ModelDef {
    fn from_values(values: Vec<DbValue>) -> anyhow::Result<Self>
    where
        Self: Sized;
    fn to_values(&self) -> Vec<DbValue>;

    fn pk_values(&self) -> Vec<(String, DbValue)> {
        let mut ve = vec![];

        for (k, v) in self
            .user_columns()
            .into_iter()
            .zip(self.to_values().into_iter())
        {
            if self.pk_names().contains(&k.column_name) {
                ve.push((k.column_name.to_string(), v));
            }
        }
        ve
    }

    fn sql_for_delete_row(self) -> SQLAndParams
    where
        Self: Sized,
    {
        let table_name = format!("{}_{}", self.model_grp(), self.table_name());

        let pk_values = self.pk_values();
        let mut where_txt = vec![];
        for (name, _value) in pk_values.iter() {
            where_txt.push(format!("{} = ?", name));
        }
        let where_txt = where_txt.join(" AND ");

        let sql = format!(
            r#"
        DELETE FROM {table_name} WHERE {where_txt}
        "#
        );
        let params = pk_values.into_iter().map(|x| x.1).collect::<Vec<_>>();
        SQLAndParams { sql, params }
    }

    fn sql_for_insert_row_or_ignore(&self) -> SQLAndParams {
        let table_name = format!("{}_{}", self.model_grp(), self.table_name());

        let columns = self.user_columns();
        let mut column_names = vec![];
        for column in columns {
            column_names.push(column.column_name);
        }
        let column_names_str = column_names.join(", ");

        let mut placeholders = vec![];
        for _ in columns {
            placeholders.push("?");
        }
        let placeholders_str = placeholders.join(", ");

        let sql = format!(
            r#"
        INSERT OR IGNORE INTO {table_name} ({column_names_str}) VALUES ({placeholders_str})
        "#
        );
        let params = self.to_values();
        SQLAndParams { sql, params }
    }

    fn sql_for_upsert_row(&self) -> SQLAndParams {
        let table_name = format!("{}_{}", self.model_grp(), self.table_name());

        let columns = self.user_columns();
        let mut column_names = vec![];
        for column in columns {
            column_names.push(column.column_name);
        }
        let column_names_str = column_names.join(", ");

        let mut placeholders = vec![];
        for _ in columns {
            placeholders.push("?");
        }
        let placeholders_str = placeholders.join(", ");

        let sql = format!(
            r#"
        INSERT OR REPLACE INTO {table_name} ({column_names_str}) VALUES ({placeholders_str})
        "#
        );
        let params = self.to_values();
        SQLAndParams { sql, params }
    }
}

#[derive(Clone, Debug)]
pub struct ModelColumnImpl {
    pub column_name: &'static str,
    pub column_type: DbValueType,
    pub is_nullable: bool,
}

fn sql_for_create_table(model: &dyn ModelDef) -> SQLAndParams {
    let table_name = format!("{}_{}", model.model_grp(), model.table_name());
    let mut model_columns = vec![];
    for column in model.user_columns() {
        let i = format!(
            "{} {} {}",
            column.column_name,
            column.column_type.to_sql_str(),
            if column.is_nullable { "" } else { " NOT NULL " }
        );
        model_columns.push(i);
    }

    let pks = model.pk_names().join(", ");
    let pks = format!("PRIMARY KEY ({})", pks);
    model_columns.push(pks);

    let model_columns_str = model_columns.join(",\n        ");
    let sql = format!(
        r#"
    CREATE TABLE {table_name}
    (
        {model_columns_str}
    );
    "#
    );
    SQLAndParams {
        sql,
        params: vec![],
    }
}

fn sql_for_drop_table(model: &dyn ModelDef) -> SQLAndParams {
    let table_name = format!("{}_{}", model.model_grp(), model.table_name());
    let sql = format!(
        r#"
    DROP TABLE IF EXISTS {table_name};
    "#
    );
    SQLAndParams {
        sql,
        params: vec![],
    }
}

pub async fn run_migrate_tables(
    groups: impl Iterator<Item = Arc<dyn ModelGroup>>,
) -> anyhow::Result<()> {
    let mut v = vec![];

    for grp in groups.into_iter() {
        for model in grp.model_infos() {
            // TODO! Get existing model SQLs from the DB and only drop/create if changed

            let sql_drop = sql_for_drop_table(*model);
            v.push(sql_drop);

            let sql_create = sql_for_create_table(*model);
            v.push(sql_create);
        }
    }

    for s in v {
        tracing::info!("RUNNING MIGRATE SQL:     {:?}", &s);
        let _r = sql_query(s).await;
        tracing::info!("RUNNING MIGRATE RESULT = {:?}", &_r);
        _r?;
    }

    Ok(())
}

pub trait DbTypeMapping {
    const DB_TYPE: DbValueType;
    const IS_NULLABLE: bool;
}

impl DbTypeMapping for i64 {
    const DB_TYPE: DbValueType = DbValueType::Integer;
    const IS_NULLABLE: bool = false;
}
impl DbTypeMapping for String {
    const DB_TYPE: DbValueType = DbValueType::Text;
    const IS_NULLABLE: bool = false;
}
impl DbTypeMapping for f64 {
    const DB_TYPE: DbValueType = DbValueType::Real;
    const IS_NULLABLE: bool = false;
}
impl DbTypeMapping for Vec<u8> {
    const DB_TYPE: DbValueType = DbValueType::Blob;
    const IS_NULLABLE: bool = false;
}
impl<T: DbTypeMapping> DbTypeMapping for Option<T> {
    const DB_TYPE: DbValueType = T::DB_TYPE;
    const IS_NULLABLE: bool = true;
}

#[macro_export]
macro_rules! declare_model_group {
    (
        $grp_name:ident,

        $(
            #[db_table(pk($($pk:ident),*))]
            pub struct $struct_name:ident {
                $(
                    pub $col_name:ident : $col_type:ty
                ),* $(,)?
            }
        )*
    ) => {
        $crate::api_asscrack::paste::paste! {
            pub struct $grp_name;
            impl $crate::models::ModelGroup for $grp_name {
                fn grp_name(&self) -> &'static str {
                    stringify!($grp_name)
                }
                fn model_infos(&self) -> &'static [&'static dyn $crate::models::ModelDef] {
                    &[
                        $(
                            &[<$struct_name _Entity>],
                        )*
                    ]
                }
            }

            $(
                // Entity struct
                #[allow(nonstandard_style)]
                pub struct [<$struct_name _Entity>];
                impl $crate::models::ModelDef for [<$struct_name _Entity>] {
                    fn table_name(&self) -> &'static str {
                        stringify!($struct_name)
                    }
                    fn model_grp(&self) -> &'static str {
                        stringify!($grp_name)
                    }
                    fn user_columns(&self) -> &'static [$crate::models::ModelColumnImpl] {
                        &[
                            $(
                                $crate::models::ModelColumnImpl {
                                    column_name: stringify!($col_name),
                                    column_type: <$col_type as $crate::models::DbTypeMapping>::DB_TYPE,
                                    is_nullable: <$col_type as $crate::models::DbTypeMapping>::IS_NULLABLE,
                                },
                            )*
                        ]
                    }
                    fn pk_names(&self) -> &'static [&'static str] {
                        &[
                            $(
                                stringify!($pk),
                            )*
                        ]
                    }
                }

                // Actual struct
                pub struct $struct_name {
                    $(
                        pub $col_name: $col_type,
                    )*
                }

                impl $crate::models::ModelDef for $struct_name {
                    fn table_name(&self) -> &'static str {
                        [<$struct_name _Entity>].table_name()
                    }
                    fn model_grp(&self) -> &'static str {
                        [<$struct_name _Entity>].model_grp()
                    }
                    fn user_columns(&self) -> &'static [$crate::models::ModelColumnImpl] {
                        [<$struct_name _Entity>].user_columns()
                    }
                    fn pk_names(&self) -> &'static [&'static str] {
                        [<$struct_name _Entity>].pk_names()
                    }
                }
                impl $crate::models::ModelSerial for $struct_name {
                    fn from_values(values: Vec<$crate::types::DbValue>) -> anyhow::Result<Self> {
                        let mut i = values.into_iter();
                        let r = Self {
                            $(
                               $col_name: $crate::types::DbValue::fold_option(i.next()).try_into()?,
                            )*
                        };
                        Ok(r)
                    }
                    fn to_values(&self) -> Vec<$crate::types::DbValue> {
                        let mut v = vec![];
                        $(
                            v.push(self.$col_name.clone().into());
                        )*

                        v
                    }
                }
            )*
        }
    }
}

#[allow(unused_imports, unused, dead_code)]
mod test {

    use std::sync::Arc;

    use crate::{
        impl_rusqulite::sql_query,
        models::{ModelGroup, ModelSerial, run_migrate_tables},
    };

    declare_model_group! {
        ModelGroup1,


        #[db_table(pk(id1, id2))]
        pub struct Table1 {
            pub id1: i64,
            pub id2: String,
            pub val3: Option<String>,
            pub val4: Option<f64>,
            pub val5: Option<Vec<u8>>,
        }


        #[db_table(pk(a))]
        pub struct Table2 {
            pub a: i64,
        }
    }

    #[tokio::test]
    async fn test_migrate() -> anyhow::Result<()> {
        let _r = run_migrate_tables(vec![Arc::new(ModelGroup1) as Arc<dyn ModelGroup>].into_iter())
            .await?;

        let t1 = Table1 {
            id1: 1,
            id2: "2".into(),
            val3: Some("3".into()),
            val4: Some(3.14),
            val5: None,
        };

        let t1_create_1 = t1.sql_for_insert_row_or_ignore();
        let t1_create_2 = t1.sql_for_upsert_row();
        let t1_delete = t1.sql_for_delete_row();

        let _r = sql_query(t1_create_1).await?;
        let _r = sql_query(t1_create_2.clone()).await?;
        let _r = sql_query(t1_delete).await?;
        let _r = sql_query(t1_create_2).await?;

        Ok(())
    }
}

/*
// ====================

pub struct ModelGroup1;
impl ModelGroup for ModelGroup1 {
    fn grp_name(&self) -> &'static str {
        "ModelGroup1"
    }
    fn model_infos(&self) -> &'static [&'static dyn ModelImpl] {
        &[
            &Table1_Entity,
            &Table2_Entity,
        ]
    }
}

// ========================
#[allow(nonstandard_style)]
pub struct Table1_Entity;
impl ModelImpl for Table1_Entity {
    fn table_name(&self)-> &'static str {
        "Table1"
    }

    fn model_grp(&self)-> &'static str {
        "ModelGroup1"
    }

    fn user_columns(&self) -> &'static [ModelColumnImpl] {
        &[
            ModelColumnImpl {
                column_name: "id1",
                column_type: DbValueType::Integer,
                is_nullable: false,
            },
            ModelColumnImpl {
                column_name: "id2",
                column_type: DbValueType::Text,
                is_nullable: false,
            },
                        ModelColumnImpl {
                column_name: "val3",
                column_type: DbValueType::Text,
                is_nullable: true,
            },
                        ModelColumnImpl {
                column_name: "val4",
                column_type: DbValueType::Real,
                is_nullable: true,
            },
                        ModelColumnImpl {
                column_name: "val5",
                column_type: DbValueType::Blob,
                is_nullable: true,
            },
        ]
    }
    fn pk_names(&self) -> &'static [&'static str] {
        &["id1", "id2"]
    }
}
pub struct Table1 {
    pub id1: i64,
    pub id2: String,
    pub val3: Option<String>,
    pub val4: Option<f64>,
    pub val5: Option<Vec<u8>>,
}

impl ModelImpl for Table1 {
    fn table_name(&self)-> &'static str {
        Table1_Entity.table_name()
    }

    fn model_grp(&self)-> &'static str {
        Table1_Entity.model_grp()
    }

    fn user_columns(&self) -> &'static [ModelColumnImpl] {
        Table1_Entity.user_columns()
    }
    fn pk_names(&self) -> &'static [&'static str] {
        Table1_Entity.pk_names()
    }
}

// ========================

#[allow(nonstandard_style)]
pub struct Table2_Entity;
impl ModelImpl for Table2_Entity {
    fn table_name(&self)-> &'static str {
        "Table2"
    }

    fn model_grp(&self)-> &'static str {
        "ModelGroup1"
    }

    fn user_columns(&self) -> &'static [ModelColumnImpl] {
        &[
            ModelColumnImpl {
                column_name: "a",
                column_type: DbValueType::Integer,
                is_nullable: false,
            }
        ]
    }
    fn pk_names(&self) -> &'static [&'static str] {
        &["a"]
    }
}
pub struct Table2 {
    pub a: i64,
}

impl ModelImpl for Table2 {
    fn table_name(&self)-> &'static str {
        Table2_Entity.table_name()
    }

    fn model_grp(&self)-> &'static str {
        Table2_Entity.model_grp()
    }

    fn user_columns(&self) -> &'static [ModelColumnImpl] {
        Table2_Entity.user_columns()
    }

    fn pk_names(&self) -> &'static [&'static str] {
        Table2_Entity.pk_names()
    }

}
*/
