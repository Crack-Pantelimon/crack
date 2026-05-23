use std::sync::Arc;

use crate::{api::api_method_macros::{ApiGroupDecl, ApiMethodDecl}, crack_worker::{WorkerMessage, WorkerPipe}};

#[derive(Clone)]
pub struct ApiClient {
    tx: std::sync::Arc<tokio::sync::mpsc::Sender<WorkerMessage>>,
    rx: std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<WorkerMessage>>>,
}


impl ApiClient {

    pub async fn new(pipe: WorkerPipe) -> Self {
        Self {
            tx: Arc::new(pipe.req_tx),
            rx: Arc::new(tokio::sync::Mutex::new(pipe.resp_rx)),
        }
    }

    pub async fn call<T: ApiMethodDecl>(&self, arg: T::Arg) -> anyhow::Result<T::Ret> {
        let arg = postcard::to_stdvec(&arg)?;
        let msg_type = T::fullname();

        let msg = WorkerMessage {
            msg_content: arg,
            msg_id: random_u64(),
            msg_type,
        };

        fn random_u64() -> u64 { 0 }

        // TODO: concurrency based on random_arg
        self.tx.send(msg).await?;
        let ret = {

            let mut rx = self.rx.lock().await;
            rx.recv().await

        };
        let Some(ret) = ret else {
            anyhow::bail!("no msg coming back from api.");
        };

        // TODO: check msg type, msg_id
        let ret_type = ret.msg_type;
        // let ret_id = ret.msg_id;
        if ret_type != "return" {
            let content_str = String::from_utf8_lossy(&ret.msg_content).to_string();

            anyhow::bail!("worker returned err: type={} str={}", ret_type, content_str);

        }
        let ret = ret.msg_content;

        let ret: Result<<T as ApiMethodDecl>::Ret, String>  = postcard::from_bytes(&ret)?;
        ret.map_err(|e| anyhow::anyhow!("{e}"))
    } 
}