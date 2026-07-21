//! SQLite-backed storage for crackhouse with native and wasm backends.
//!
//! Exposes SQL execution, schema migration helpers, and value types.

/// Public API bindings for remote callers.
pub mod api;
/// Rusqlite connection pool and query execution.
pub mod impl_rusqulite;
/// Table model traits, migrations, and schema macros.
pub mod models;
/// Shared SQL parameter and result value types.
pub mod types;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
/// Install the OPFS SAH-pool SQLite VFS in the browser.
pub async fn install_opfs_sahpool() -> anyhow::Result<()> {
    tracing::info!("install_opfs_sahpool() ...");
    use sqlite_wasm_vfs::sahpool::{OpfsSAHPoolCfg, install};
    install::<sqlite_wasm_rs::WasmOsCallback>(&OpfsSAHPoolCfg::default(), false)
        .await
        .map_err(|e| {
            tracing::error!("install_opfs_sahpool(): {e:?}");
            anyhow::anyhow!("install_opfs_sahpool(): {e:?}")
        })?;
    Ok(())
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
/// Install the relaxed IndexedDB SQLite VFS in the browser.
pub async fn install_relaxed_idb() -> anyhow::Result<()> {
    tracing::info!("install_relaxed_idb() ...");
    use sqlite_wasm_vfs::relaxed_idb::{RelaxedIdbCfg, install};
    install::<sqlite_wasm_rs::WasmOsCallback>(&RelaxedIdbCfg::default(), false)
        .await
        .map_err(|e| {
            tracing::error!(" install_relaxed_idb(): {e:?}");
            anyhow::anyhow!(" install_relaxed_idb(): {e:?}")
        })?;
    Ok(())
}

pub use api_asscrack;

// Browser-only: installs a wasm SQLite VFS backed by browser storage, then
// runs a trivial query through it. `install_opfs_sahpool`'s SAH-pool VFS
// needs `createSyncAccessHandle`, which requires a cross-origin-isolated
// page (COOP/COEP) that the plain wasm-pack test harness does not serve, so
// this instead exercises `install_relaxed_idb` (IndexedDB-backed, no
// cross-origin isolation needed) directly against `rusqlite::Connection` —
// the production path (`impl_rusqulite::_new_connection`) always opens the
// SAH-pool VFS and isn't exercised here.
#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    async fn smoke_relaxed_idb_migrate_and_query() {
        crate::install_relaxed_idb()
            .await
            .expect("install_relaxed_idb");

        let conn = rusqlite::Connection::open("file:crackhouse_smoke.db?vfs=relaxed-idb")
            .expect("open rusqlite connection over relaxed-idb VFS");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS smoke (id INTEGER PRIMARY KEY)",
            [],
        )
        .expect("create table over relaxed-idb VFS");
        conn.execute("INSERT INTO smoke (id) VALUES (1)", [])
            .expect("insert over relaxed-idb VFS");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM smoke", [], |row| row.get(0))
            .expect("query over relaxed-idb VFS");
        assert_eq!(count, 1);
    }
}
