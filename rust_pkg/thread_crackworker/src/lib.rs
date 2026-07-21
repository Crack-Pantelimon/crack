//! Threaded worker backend for the async worker RPC framework.
//!
//! This crate provides a Tokio-based [`WorkerLoaderFactory`] implementation
//! that runs the API message loop on a background Tokio task. It is the
//! default native backend when the application is not compiled for WebAssembly.

pub extern crate dioxus_logger;
pub extern crate tokio;
pub extern crate tracing;

use std::sync::Arc;

use api_asscrack::crack_worker::{
    WorkerLoaderFactory, WorkerMessage, WorkerPipe, api_worker::ApiImplMapping,
};

// Called when the wasm module is instantiated

/// Factory that produces a [`WorkerPipe`] backed by a Tokio task.
///
/// The factory stores an [`Arc<ApiImplMapping>`] containing all registered
/// API method implementations. When [`load_worker`] is called it spawns a
/// background task that drains the request channel, invokes the matching
/// method implementation via [`compute_response_message`], and sends the
/// response back through the reply channel.
///
/// This is the default native (non-WASM) worker backend.
///
/// [`load_worker`]: WorkerLoaderFactory::load_worker
/// [`compute_response_message`]: api_asscrack::crack_worker::api_worker::compute_response_message
pub struct ThreadWorkerFactory {
    /// Mapping from fully-qualified method names to their implementations.
    pub impl_mapping: Arc<ApiImplMapping>,
}

#[api_asscrack::async_trait::async_trait(?Send)]
impl WorkerLoaderFactory for ThreadWorkerFactory {
    /// Spawns the worker task and returns the request/response channel pair.
    ///
    /// The returned [`WorkerPipe`] connects the caller to a background Tokio
    /// task that serializes incoming requests, dispatches them to the API
    /// implementations stored in `self.impl_mapping`, and sends typed
    /// responses back.
    ///
    /// # Errors
    /// Returns an error if the channel setup or task spawn fails.
    async fn load_worker(&self) -> anyhow::Result<WorkerPipe> {
        // let worker_compute = std::sync::Arc::new(ThreadWorkerCompute);
        let t = init_thread(self.impl_mapping.clone()).await?;

        Ok(t)
    }
}

/// Spawns the background request-processing task and returns the channel ends.
///
/// Creates a bounded MPSC channel pair. The returned [`WorkerPipe`] holds the
/// sender for requests and the receiver for responses. A Tokio task is spawned
/// that continuously receives requests, handles the special `"ping"` message
/// by echoing a `"pong"` response, and delegates all other messages to
/// [`compute_response_message`] with the provided API implementation mapping.
///
/// # Errors
/// Returns an error if the channel creation fails (should not happen with
/// bounded channels).
///
/// [`compute_response_message`]: api_asscrack::crack_worker::api_worker::compute_response_message
async fn init_thread(mapping: Arc<ApiImplMapping>) -> anyhow::Result<WorkerPipe> {
    let (req_tx, mut req_rx) = tokio::sync::mpsc::channel::<WorkerMessage>(1024);
    let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(1024);

    let _t = tokio::task::spawn(async move {
        while let Some(req) = req_rx.recv().await {
            let resp_tx = resp_tx.clone();
            let mapping = mapping.clone();
            // let worker_compute = worker_compute.clone();
            tokio::task::spawn(async move {
                let resp = if &req.msg_type == "ping" {
                    let mut new = req.clone();
                    new.msg_type = "pong".to_string();
                    new
                } else {
                    api_asscrack::crack_worker::api_worker::compute_response_message(req, mapping)
                        .await
                };
                let m = resp_tx.send(resp).await;
                match m {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("error sending msg: {e:#?}");
                    }
                }
            });
        }
    });

    Ok(WorkerPipe { req_tx, resp_rx })
}

mod test {
    #![allow(unused)]
    use std::sync::Arc;

    use api_asscrack::{
        api::api_worker_declarations::WorkerApiGroup2, crack_worker::api_worker::make_api_mapping,
        declare_api_group2, implement_api_group2,
    };

    #[tokio::test]
    async fn test_api_rx_tx_ping() -> anyhow::Result<()> {
        use crate::ThreadWorkerFactory;
        use api_asscrack::crack_worker::{WorkerLoaderFactory, WorkerMessage};

        let mut f = ThreadWorkerFactory {
            impl_mapping: make_api_mapping(vec![]),
        }
        .load_worker()
        .await?;

        f.req_tx
            .send(WorkerMessage {
                msg_id: 0,
                msg_type: "ping".to_string(),
                msg_content: "abcd".to_string().as_bytes().to_vec(),
            })
            .await?;
        let t = f.resp_rx.recv().await.unwrap();
        assert!(t.msg_type == "pong");
        assert!(t.msg_content == "abcd".as_bytes().to_vec());
        Ok(())
    }

    #[tokio::test]
    async fn test_api_ping_fn() -> anyhow::Result<()> {
        use crate::ThreadWorkerFactory;
        use api_asscrack::crack_worker::WorkerLoaderFactory;
        let f = ThreadWorkerFactory {
            impl_mapping: make_api_mapping(vec![Arc::new(WorkerApiGroup2), Arc::new(TestApiGroup)]),
        }
        .load_worker()
        .await?;
        let c = api_asscrack::api::api_client::ApiClient::new(f);

        let _r = c
            .call::<api_asscrack::api::api_worker_declarations::WorkerPing>(())
            .await?;

        let _r2 = c.call::<TestPing>((1, 2)).await?;
        assert!(_r2 == 3);
        Ok(())
    }

    declare_api_group2! {
        TestApiGroup,
        [
            (TestPing, (i32, i32), i32),
        ]
    }

    mod __impl {
        use super::{TestApiGroup, TestPing};
        use api_asscrack::implement_api_group2;

        implement_api_group2! {
            TestApiGroup,
            [
                (TestPing, test_ping),
            ]
        }

        async fn test_ping((x, y): (i32, i32)) -> anyhow::Result<i32> {
            Ok(x + y)
        }
    }
}
