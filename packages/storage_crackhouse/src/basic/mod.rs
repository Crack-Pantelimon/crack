
use std::{cell::OnceCell, sync::{Arc, LazyLock, Mutex, Once, OnceLock}};

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement, sea_query::backend};

mod entity;
mod mutation;
mod query;

use entity::*;
use mutation::*;
use query::*;

const DEMO_SQL: &str = "

DROP TABLE IF EXISTS `cake`;

CREATE TABLE `cake` (
  `id` int NOT NULL AUTO_INCREMENT,
  `name` varchar(255) NOT NULL,
  PRIMARY KEY (`id`)
);

INSERT INTO `cake` (`id`, `name`) VALUES
	(1, 'New York Cheese'),
	(2, 'Chocolate Forest');

DROP TABLE IF EXISTS `fruit`;

CREATE TABLE `fruit` (
  `id` int NOT NULL AUTO_INCREMENT,
  `name` varchar(255) NOT NULL,
  `cake_id` int DEFAULT NULL,
  PRIMARY KEY (`id`),
  CONSTRAINT `fk-fruit-cake` FOREIGN KEY (`cake_id`) REFERENCES `cake` (`id`)
);

INSERT INTO `fruit` (`id`, `name`, `cake_id`) VALUES
  (1, 'Blueberry', 1),
  (2, 'Raspberry', 1),
  (3, 'Strawberry', 2);

INSERT INTO `fruit` (`name`, `cake_id`) VALUES
  ('Apple', NULL),
  ('Banana', NULL),
  ('Cherry', NULL),
  ('Lemon', NULL),
  ('Orange', NULL),
  ('Pineapple', NULL);

DROP TABLE IF EXISTS `filling`;

CREATE TABLE `filling` (
  `id` int NOT NULL AUTO_INCREMENT,
  `name` varchar(255) NOT NULL,
  PRIMARY KEY (`id`)
);

INSERT INTO `filling` (`id`, `name`) VALUES
  (1, 'Vanilla'),
  (2, 'Lemon'),
  (3, 'Mango');

DROP TABLE IF EXISTS `cake_filling`;

CREATE TABLE `cake_filling` (
  `cake_id` int NOT NULL,
  `filling_id` int NOT NULL,
  PRIMARY KEY (`cake_id`, `filling_id`),
  CONSTRAINT `fk-cake_filling-cake` FOREIGN KEY (`cake_id`) REFERENCES `cake` (`id`),
  CONSTRAINT `fk-cake_filling-filling` FOREIGN KEY (`filling_id`) REFERENCES `filling` (`id`)
);

INSERT INTO `cake_filling` (`cake_id`, `filling_id`) VALUES
  (1, 1),
  (1, 2),
  (2, 2),
  (2, 3);

";

fn db_conn() -> anyhow::Result<DatabaseConnection> {

    // ON WASM
    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    const FILE: &str = "sqlite:file:/assets/scripts/post2.db?vfs=opfs-sahpool"; 

    // ON NON-WASM
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    const FILE: &str = "sqlite:post2.db";

    // let rs = rusqlite::Connection::open(FILE)?;

    tracing::info!("opening db : {FILE}");
    let db = Database::connect(
        FILE,
    )?;

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
    let t: &'static DatabaseConnection = DB_LOCK.get_or_init(db_conn).as_ref().map_err(|e| anyhow::anyhow!("{e:#?}"))?;
    // DB_LOCK.as_ref().map_err(|e| anyhow::anyhow!("{e:?}"))
    Ok(t)
}

pub fn demo_main_seaorm() -> anyhow::Result<()> {

    tracing::info!("Running: demo_main_seaorm()");

    let db =  connect_sqlite_db()?;


    tracing::info!("DB SYNC CREATE TABLES ETC ALL OK.");

    tracing::info!("{db:?}\n");

    let _r = db.execute_unprepared(DEMO_SQL);
    tracing::info!("EXECUTE SQL: {:#?}", _r);
    

    tracing::info!("===== =====\n");

    all_about_query(&db)?;

    tracing::info!("===== =====\n");

    all_about_mutation(&db)?;

    Ok(())
}


pub fn execute_sql2(sql: String) -> anyhow::Result<String> {

    let db =  connect_sqlite_db()?;

    let _r = db.query_all_raw(
        Statement::from_string(DbBackend::Sqlite, &sql))?;
    tracing::info!("EXECUTE SQL: {:#?}", _r);

    let mut x = "".into();
    for (i, r) in _r.iter().enumerate() {
        if i == 0 {
            let cols = r.column_names();
            let cols = cols.join(" | ");
            x += cols.as_str();
            x += "\n===============================";

        }
        let txt = format!("{r:?}\n");


        x += txt.as_str();
    }

    Ok(x)
}