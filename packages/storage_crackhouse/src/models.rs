use std::sync::Arc;

use crate::{impl_rusqulite::sql_query, types::{DbValueType, SQLAndParams}};

pub trait ModelGroup {
    fn grp_name(&self) -> &'static str;
    fn model_infos(&self) -> &'static [&'static dyn ModelImpl];
}
// impl ModelGroup for std::sync::Arc<dyn ModelGroup> {
//     fn grp_name(&self) -> &'static str {
//         ModelGroup::grp_name(self.as_ref())
//     }

//     fn model_infos(&self) -> &'static [ModelImpl] {
//         ModelGroup::model_infos(self.as_ref())
//     }
// }

pub trait ModelImpl {
    fn table_name(&self)-> &'static str;
    fn model_grp(&self)-> &'static str;
    fn user_columns(&self) -> &'static [ModelColumnImpl];
    fn pk_names(&self) -> &'static [&'static str];
}

#[derive(Clone, Debug)]
pub struct ModelColumnImpl {
    pub column_name: &'static str,
    pub column_type: DbValueType,
    pub is_nullable: bool,
}

fn sql_for_create_table(grp: Arc<dyn ModelGroup>, model: &dyn ModelImpl) -> String {
    let table_name = format!("{}_{}", grp.grp_name(), model.table_name());
    let mut model_columns = vec![];
    let mut pks = vec![];
    for column in model.user_columns() {
        let i = format!("{} {} {}", column.column_name, column.column_type.to_sql_str(), if column.is_nullable {""} else {" NOT NULL "});
        model_columns.push(i);
        if column.is_pk {
            pks.push(column.column_name);
        }
    }

    let pks = pks.join(", ");
    let pks = format!("PRIMARY KEY ({})", pks);
    model_columns.push(pks);

    let model_columns_str = model_columns.join(",\n        ");
    format!(r#"
    CREATE TABLE {table_name}
    (
        {model_columns_str}
    );
    "#)
}

fn sql_for_drop_table(grp: Arc<dyn ModelGroup>,model :&dyn ModelImpl) -> String {
    let table_name = format!("{}_{}", grp.grp_name(), model.table_name());
    format!(r#"
    DROP TABLE IF EXISTS {table_name};
    "#)
}

pub async fn run_migrate_tables(groups: impl Iterator<Item = Arc<dyn ModelGroup>>) -> anyhow::Result<()> {

    let mut v = vec![];

    for grp in groups.into_iter() {
        for model in grp.model_infos() {
            // TODO! Get existing model SQLs from the DB and only drop/create if changed


            let sql_drop = sql_for_drop_table(grp.clone(), *model);
            v.push(sql_drop);

            let sql_create = sql_for_create_table(grp.clone(), *model);
            v.push(sql_create);
        }
    }

    for s in v {
        tracing::info!("RUNNING MIGRATE SQL:     {}", &s);
        let s = SQLAndParams {sql: s, params: vec![]};
        let _r = sql_query(s).await;
        tracing::info!("RUNNING MIGRATE RESULT = {:?}", &_r);
        _r?;
    }

    
    Ok(())
}


/* 

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


*/


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
