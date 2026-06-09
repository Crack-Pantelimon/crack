pub mod api;
pub mod impl_rusqulite;
pub mod models;
pub mod types;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
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
