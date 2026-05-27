use std::{collections::HashMap, sync::Arc};

use _crack_utils::{n0_future, random_u32};

use crate::{
    api::api_method_macros::ApiMethodDecl,
    crack_worker::{WorkerMessage, WorkerPipe},
};

#[derive(Clone)]
pub struct ApiClient {
    tx: Arc<tokio::sync::mpsc::Sender<WorkerMessage>>,
    // rx: std::sync::Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<WorkerMessage>>>,
    _memory: Arc<tokio::sync::Mutex<ApiClientMemory>>,
    _thread: Arc<_crack_utils::n0_future::task::JoinHandle<anyhow::Result<()>>>,
}

struct ApiClientMemory {
    map: HashMap<u32, MessageLater>,
}

pub struct MessageLater {
    reply_to: tokio::sync::oneshot::Sender<WorkerMessage>,
}

async fn client_thread(
    _memory: Arc<tokio::sync::Mutex<ApiClientMemory>>,
    mut rx_pipe: tokio::sync::mpsc::Receiver<WorkerMessage>,
) -> anyhow::Result<()> {
    while let Some(ret) = rx_pipe.recv().await {
        let ret_id = ret.msg_id;

        let Some(connect) = ({ _memory.lock().await.map.remove(&ret_id) }) else {
            tracing::warn!("got a message back but no known ID, id={}", ret_id);
            continue;
        };
        let _r = connect.reply_to.send(ret);
        if let Err(_e) = _r {
            tracing::info!("Failed to send back worker message: id={}", _e.msg_id);
            continue;
        }
    }
    Ok(())
}

impl ApiClient {
    pub fn new(pipe: WorkerPipe) -> Self {
        let _memory = Arc::new(tokio::sync::Mutex::new(ApiClientMemory {
            map: HashMap::new(),
        }));
        let _memory2 = _memory.clone();
        Self {
            tx: Arc::new(pipe.req_tx),
            // rx: Arc::new(tokio::sync::Mutex::new(pipe.resp_rx)),
            _thread: Arc::new(n0_future::task::spawn(client_thread(
                _memory2,
                pipe.resp_rx,
            ))),
            _memory,
        }
    }

    pub async fn call<T: ApiMethodDecl>(&self, arg: T::Arg) -> anyhow::Result<T::Ret> {
        let arg = postcard::to_stdvec(&arg)?;
        let msg_type = T::fullname();
        let req_id = random_u32();

        let msg = WorkerMessage {
            msg_content: arg,
            msg_id: req_id,
            msg_type,
        };
        let (one_tx, one_rx) = tokio::sync::oneshot::channel::<WorkerMessage>();

        let _ins = {
            let _memory = &mut self._memory.lock().await.map;

            _memory.insert(req_id, MessageLater { reply_to: one_tx })
        };
        self.tx.send(msg).await?;

        let ret = one_rx.await?;

        let ret_type = ret.msg_type;
        if ret_type != "return" {
            let content_str = String::from_utf8_lossy(&ret.msg_content).to_string();
            tracing::info!("worker returned err: type={} str={}", ret_type, content_str);
            anyhow::bail!("worker returned err: type={} str={}", ret_type, content_str);
        }
        let ret = ret.msg_content;

        let ret: Result<<T as ApiMethodDecl>::Ret, String> = postcard::from_bytes(&ret)?;
        ret.map_err(|e| anyhow::anyhow!("{e}"))
    }
}
