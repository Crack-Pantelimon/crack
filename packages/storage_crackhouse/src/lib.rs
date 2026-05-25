pub mod api;


pub mod models;
pub mod schema;

mod demo;

use std::sync::Once;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_migrations::EmbeddedMigrations;
use diesel_migrations::MigrationHarness;
use diesel_migrations::embed_migrations;


const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");


pub fn establish_connection() -> SqliteConnection {
    // let (vfs, once) = &*VFS.lock().unwrap();
    // let url = match vfs {
    //     0 => "post.db",
    //     1 => "file:post.db?vfs=opfs-sahpool",
    //     2 => "file:post.db?vfs=relaxed-idb",
    //     _ => unreachable!(),
    // };
    // let url = "post.db";
    // let url = "file:post.db?vfs=relaxed-idb";
    tracing::info!("establish_connection() .... ");
    let url = "file:post.db?vfs=opfs-sahpool";

static CONN: Once = Once::new();

    let mut conn =
        SqliteConnection::establish(url).unwrap_or_else(|_| panic!("Error connecting to post.db"));
    CONN.call_once(|| {
        let pending = conn.pending_migrations(MIGRATIONS).unwrap();
        tracing::info!("Have to run some {} migrations: {:#?} ", pending.len(), pending.into_iter().map(|x| x.name().to_string()).collect::<Vec<_>>());

        tracing::info!("Running Migrations...");
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        tracing::info!("Migrations finished.");
    });


    tracing::info!("establish_connection() OK ");
// see https://fractaledmind.github.io/2023/09/07/enhancing-rails-sqlite-fine-tuning/
// sleep if the database is busy, this corresponds to up to 2 seconds sleeping time.
let _ = conn.batch_execute("PRAGMA busy_timeout = 2000;");
// better write-concurrency
let _ = conn.batch_execute("PRAGMA journal_mode = WAL;");
// fsync only in critical moments
let _ = conn.batch_execute("PRAGMA synchronous = NORMAL;");
// write WAL changes back every 1000 pages, for an in average 1MB WAL file.
// May affect readers if number is increased
let _ = conn.batch_execute("PRAGMA wal_autocheckpoint = 1000;");
// // free some space by truncating possibly massive WAL files from the last run
// let _ = conn.batch_execute("PRAGMA wal_checkpoint(TRUNCATE);");
    conn
}


#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub async fn install_opfs_sahpool() -> anyhow::Result<()>{
    tracing::info!("install_opfs_sahpool() ...");
    use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfg, install};
    install::<sqlite_wasm_rs::WasmOsCallback>(&OpfsSAHPoolCfg::default(), false)
        .await.map_err(|e| {
            tracing::error!("install_opfs_sahpool(): {e:?}");
            anyhow::anyhow!("install_opfs_sahpool(): {e:?}")})?;
    Ok(())
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub async fn install_relaxed_idb() -> anyhow::Result<()> {
    tracing::info!("install_relaxed_idb() ...");
    use sqlite_wasm_vfs::relaxed_idb::{RelaxedIdbCfg, install};
    install::<sqlite_wasm_rs::WasmOsCallback>(&RelaxedIdbCfg::default(), false)
        .await
        .map_err(|e| {
            tracing::error!(" install_relaxed_idb(): {e:?}");
            anyhow::anyhow!(" install_relaxed_idb(): {e:?}")})?;
    Ok(())
}


