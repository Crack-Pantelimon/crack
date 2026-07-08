use std::sync::Arc;
use storage_crackhouse::declare_model_group;

declare_model_group! { GameLogicModels,
    #[db_table(pk(id))]
    pub struct GameKvEntry {
        pub id: i64,
        pub val: Option<String>,
    }
}

pub async fn run_game_migrations(_: ()) -> anyhow::Result<()> {
    storage_crackhouse::models::run_migrate_tables(
        vec![Arc::new(GameLogicModels) as Arc<dyn storage_crackhouse::models::ModelGroup>]
            .into_iter(),
    )
    .await?;

    // Create user_secrets table if not exists
    storage_crackhouse::api::execute_sql2(
        "CREATE TABLE IF NOT EXISTS user_secrets (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            secret_key TEXT NOT NULL
        )".to_string()
    )
    .await?;

    Ok(())
}
