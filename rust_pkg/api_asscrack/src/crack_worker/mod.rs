/// API-method dispatch mapping and request-response execution helpers.
pub mod api_worker;

/// The two channel endpoints used by an API client and worker to communicate.
pub struct WorkerPipe {
    /// Sends client requests to the worker event loop.
    pub req_tx: tokio::sync::mpsc::Sender<WorkerMessage>,
    /// Receives worker responses for delivery to the API client.
    pub resp_rx: tokio::sync::mpsc::Receiver<WorkerMessage>,
}

/// A serialized request or response exchanged through a [`WorkerPipe`].
#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct WorkerMessage {
    /// Correlation ID shared by the request and its response.
    pub msg_id: u32,
    /// Fully qualified method name or protocol response status.
    pub msg_type: String,
    /// Postcard-serialized payload or error detail bytes.
    pub msg_content: Vec<u8>,
}

/// Creates a worker transport for an API client.
#[async_trait::async_trait(?Send)]
pub trait WorkerLoaderFactory {
    /// Asynchronously creates the request and response channels for one worker.
    ///
    /// Returns the client-facing [`WorkerPipe`], or an error if worker startup
    /// or transport initialization fails.
    async fn load_worker(&self) -> anyhow::Result<WorkerPipe>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_worker_message_serde_roundtrip() {
        let msg = WorkerMessage {
            msg_id: 7,
            msg_type: "ping".to_string(),
            msg_content: b"abcd".to_vec(),
        };
        let bytes = postcard::to_stdvec(&msg).unwrap();
        let back: WorkerMessage = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back.msg_id, 7);
        assert_eq!(back.msg_type, "ping");
        assert_eq!(back.msg_content, b"abcd".to_vec());
    }
}
