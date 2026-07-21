use std::sync::Arc;

/// Storage model group for game-specific worker tables.
pub struct GameLogicModels;

impl storage_crackhouse::models::ModelGroup for GameLogicModels {
    fn grp_name(&self) -> &'static str {
        "GameLogicModels"
    }

    fn model_infos(&self) -> &'static [&'static dyn storage_crackhouse::models::ModelDef] {
        &[&GameKvEntry_Entity]
    }
}

/// Entity metadata for the `game_kv_entry` table schema.
#[allow(nonstandard_style)]
pub struct GameKvEntry_Entity;

impl storage_crackhouse::models::ModelDef for GameKvEntry_Entity {
    fn table_name(&self) -> &'static str {
        "GameKvEntry"
    }

    fn model_grp(&self) -> &'static str {
        "GameLogicModels"
    }

    fn user_columns(&self) -> &'static [storage_crackhouse::models::ModelColumnImpl] {
        &[
            storage_crackhouse::models::ModelColumnImpl {
                column_name: "id",
                column_type: <i64 as storage_crackhouse::models::DbTypeMapping>::DB_TYPE,
                is_nullable: <i64 as storage_crackhouse::models::DbTypeMapping>::IS_NULLABLE,
            },
            storage_crackhouse::models::ModelColumnImpl {
                column_name: "val",
                column_type:
                    <Option<String> as storage_crackhouse::models::DbTypeMapping>::DB_TYPE,
                is_nullable:
                    <Option<String> as storage_crackhouse::models::DbTypeMapping>::IS_NULLABLE,
            },
        ]
    }

    fn pk_names(&self) -> &'static [&'static str] {
        &["id"]
    }
}

/// Key-value row in the game worker database.
pub struct GameKvEntry {
    /// Row primary key.
    pub id: i64,
    /// Optional string payload.
    pub val: Option<String>,
}

impl storage_crackhouse::models::ModelDef for GameKvEntry {
    fn table_name(&self) -> &'static str {
        GameKvEntry_Entity.table_name()
    }

    fn model_grp(&self) -> &'static str {
        GameKvEntry_Entity.model_grp()
    }

    fn user_columns(&self) -> &'static [storage_crackhouse::models::ModelColumnImpl] {
        GameKvEntry_Entity.user_columns()
    }

    fn pk_names(&self) -> &'static [&'static str] {
        GameKvEntry_Entity.pk_names()
    }
}

impl storage_crackhouse::models::ModelSerial for GameKvEntry {
    fn from_values(
        values: Vec<storage_crackhouse::types::DbValue>,
    ) -> anyhow::Result<Self> {
        let mut i = values.into_iter();
        let r = Self {
            id: storage_crackhouse::types::DbValue::fold_option(i.next()).try_into()?,
            val: storage_crackhouse::types::DbValue::fold_option(i.next()).try_into()?,
        };
        Ok(r)
    }

    fn to_values(&self) -> Vec<storage_crackhouse::types::DbValue> {
        let mut v = vec![];
        v.push(self.id.clone().into());
        v.push(self.val.clone().into());
        v
    }
}

/// Runs schema migrations for game worker tables.
pub async fn run_game_migrations(_: ()) -> anyhow::Result<()> {
    storage_crackhouse::models::run_migrate_tables(
        vec![Arc::new(GameLogicModels) as Arc<dyn storage_crackhouse::models::ModelGroup>]
            .into_iter(),
    )
    .await?;

    storage_crackhouse::api::execute_sql2(
        "CREATE TABLE IF NOT EXISTS user_secrets (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            secret_key TEXT NOT NULL
        )"
        .to_string(),
    )
    .await?;

    Ok(())
}
