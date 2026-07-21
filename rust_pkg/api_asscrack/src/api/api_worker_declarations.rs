//! Built-in API declarations used to verify worker request dispatch.
//!

use _crack_utils::sleep_ms;

use crate::declare_api_group2;
use crate::implement_api_group2;

declare_api_group2! {
    WorkerApiGroup2,
    [
        (WorkerPing, (), ()),
    ]
}

implement_api_group2! {
    WorkerApiGroup2,
    [
        (WorkerPing, worker_ping),
    ]
}

/// Handles the built-in no-op worker ping request.
///
/// The unit argument carries no request data. Returns `Ok(())` after a brief
/// asynchronous delay; this implementation currently has no error path.
pub async fn worker_ping(_x: ()) -> anyhow::Result<()> {
    sleep_ms(1).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::api_method_macros::{ApiGroupMethods, ApiMethodDecl};

    async fn declarations_body() {
        // The macros above must declare + implement exactly one method.
        assert_eq!(
            <WorkerPing as ApiMethodDecl>::fullname(),
            "WorkerApiGroup2.WorkerPing"
        );
        let infos = WorkerApiGroup2.method_infos();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].fullname(), "WorkerApiGroup2.WorkerPing");
        worker_ping(()).await.expect("worker_ping should succeed");
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn smoke_worker_api_group2_declarations() {
        declarations_body().await;
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    async fn smoke_worker_api_group2_declarations() {
        declarations_body().await;
    }
}
