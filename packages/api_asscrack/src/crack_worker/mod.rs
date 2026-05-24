pub mod api_worker;

pub struct WorkerPipe {
    pub req_tx: tokio::sync::mpsc::Sender<WorkerMessage>,
    pub resp_rx: tokio::sync::mpsc::Receiver<WorkerMessage>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize, Debug)]
pub struct WorkerMessage {
    pub msg_id: u32,
    pub msg_type: String,
    pub msg_content: Vec<u8>,
}

#[async_trait::async_trait(?Send)]
pub trait WorkerLoaderFactory {
    async fn load_worker(&self) -> anyhow::Result<WorkerPipe>;
}
