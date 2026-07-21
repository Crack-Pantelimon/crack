use std::{marker::PhantomData, sync::Arc};

use chrono::{DateTime, Utc};
use futures::FutureExt;
use iroh::NodeId;
use n0_future::task::{spawn, AbortOnDropHandle};
use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use tracing::{info, warn};

use crate::{
    chat::chat_const::{CONNECT_TIMEOUT, PRESENCE_INTERVAL},
    chat::chat_presence::ChatPresence,
    chat::chat_ticket::ChatTicket,
    datetime_now,
    signed_message::{IChatRoomType, MessageSigner, SignedMessage},
    sleep::SleepManager,
    user_identity::NodeIdentity,
    ReceivedMessage,
};

/// Chat controller for a chat room type.
/// Manages message dispatch, presence, sender/receiver, and background tasks.
#[derive(Clone, Debug)]
pub struct ChatController<T: IChatRoomType> {
    ticket: ChatTicket,
    inner: Arc<dyn IChatRoomRaw>,
    presence: ChatPresence<T>,
    _p: PhantomData<T>,
    _dispatch_task: Arc<AbortOnDropHandle<anyhow::Result<()>>>,
    _presence_task: Arc<AbortOnDropHandle<anyhow::Result<()>>>,
    sender: ChatSender<T>,
    receiver: ChatReceiver<T>,
    _controller_id: uuid::Uuid,
    node_identity: NodeIdentity,
}

impl<T: IChatRoomType> PartialEq for ChatController<T> {
    fn eq(&self, other: &Self) -> bool {
        self._controller_id == other._controller_id
    }
}

async fn _dispatch_inner_loop<T: IChatRoomType>(
    m: crate::WireMessage<ChatMessage<T>>,
    msg_sender: &mut async_broadcast::Sender<ReceivedMessage<T>>,
    _presence: ChatPresence<T>,
    _sender: ChatSender<T>,
) -> anyhow::Result<()> {
    match m.message {
        ChatMessage::Message(message) => {
            msg_sender
                .broadcast(ReceivedMessage {
                    _sender_timestamp: m._timestamp,
                    _received_timestamp: datetime_now(),
                    _message_id: m._message_id,
                    from: m.from,
                    message,
                })
                .await?;
        }
        ChatMessage::Presence(presence) => {
            let was_added = _presence.add_presence(&m.from, &presence).await;
            if was_added {
                _sender.direct_presence(m.from).await?;
            }
            let ping_sender_ts = m._timestamp;
            _sender.direct_pong(m.from, ping_sender_ts).await?;
        }
        ChatMessage::Pong { ping_sender_ts } => {
            let now = datetime_now();
            let dt = now.signed_duration_since(ping_sender_ts).to_std().ok();
            if let Some(dt) = dt {
                let rtt = dt.as_micros() as f64 / 1000.0;
                if rtt > 0.0 && rtt < 65000.0 {
                    let rtt = rtt as u16 + 1;
                    _presence.update_ping(&m.from, rtt).await;
                }
            }
        }
    }
    Ok(())
}

impl<T: IChatRoomType> ChatController<T> {
    pub(crate) fn new(
        ticket: ChatTicket,
        inner: Arc<dyn IChatRoomRaw>,
        message_signer: MessageSigner,
        sleep_manager: SleepManager,
        node_identity: NodeIdentity,
    ) -> Self {
        let presence = ChatPresence::new();
        let sender = ChatSender {
            inner: inner.clone(),
            message_signer: message_signer.clone(),
            current_presence: Arc::new(RwLock::new(None)),
            chatroom_presence: presence.clone(),
            _p: PhantomData,
        };
        let (mut msg_sender, mut msg_receiver) = async_broadcast::broadcast(16);
        msg_sender.set_overflow(true);
        msg_receiver.set_overflow(true);

        let msg_receiver = Arc::new(RwLock::new(msg_receiver));
        let receiver = ChatReceiver {
            msg_receiver,
            _p: PhantomData,
        };
        let inner2 = inner.clone();
        let _presence = presence.clone();
        let _sender = sender.clone();
        let _sleep_manager = sleep_manager.clone();
        let _dispatch_task = async move {
            let mut errors = 0;
            loop {
                let Ok(Some(message)) = inner2.next_message().await else {
                    errors += 1;
                    if errors > 10 {
                        warn!("_dispatch_task: Chat room closed");
                        anyhow::bail!("_dispatch_task: Chat room closed");
                    }
                    _sleep_manager
                        .sleep(std::time::Duration::from_millis(8))
                        .await;
                    continue;
                };
                errors = 0;
                let msg = SignedMessage::verify_and_decode::<ChatMessage<T>>(&message);
                match msg {
                    Ok(m) => {
                        if let Err(e) = _dispatch_inner_loop::<T>(
                            m,
                            &mut msg_sender,
                            _presence.clone(),
                            _sender.clone(),
                        )
                        .await
                        {
                            warn!("_dispatch_task: Error dispatching message: {:?}", e);
                        }
                    }
                    Err(e) => {
                        warn!("_dispatch_task: Error verifying message: {:?}", e);
                    }
                }
            }
        };
        let _dispatch_task = AbortOnDropHandle::new(spawn(async move {
            let r = _dispatch_task.await;
            warn!("_dispatch_task: Chat room closed: {r:#?}");
            r
        }));

        let _sleep_manager = sleep_manager.clone();
        let _sender = sender.clone();
        let _presence_task = async move {
            loop {
                let _ = _sleep_manager.sleep(PRESENCE_INTERVAL).await;
                if let Err(e) = _sender.broadcast_presence().await {
                    warn!("_presence_task: Error broadcasting presence: {:?}", e);
                    anyhow::bail!("_presence_task: Error broadcasting presence: {:?}", e);
                }
            }
        };
        let _presence_task = AbortOnDropHandle::new(spawn(async move {
            let r = _presence_task.await;
            warn!("_presence_task: Chat room closed: {r:#?}");
            r
        }));

        Self {
            _controller_id: uuid::Uuid::new_v4(),
            inner,
            presence,
            _p: PhantomData,
            _dispatch_task: Arc::new(_dispatch_task),
            _presence_task: Arc::new(_presence_task),
            sender,
            receiver,
            ticket,
            node_identity,
        }
    }
}

#[async_trait::async_trait]
impl<T: IChatRoomType> IChatController<T> for ChatController<T> {
    fn node_identity(&self) -> NodeIdentity {
        self.node_identity
    }
    fn sender(&self) -> ChatSender<T> {
        self.sender.clone()
    }
    async fn receiver(&self) -> ChatReceiver<T> {
        let new_receiver = {
            let m = self.receiver.msg_receiver.write().await;
            // let new_receiver = m.clone();
            // let m2 = &mut *m;
            // std::mem::swap(m2, &mut new_receiver);
            // new_receiver
            m.clone()
        };
        ChatReceiver {
            msg_receiver: Arc::new(RwLock::new(new_receiver)),
            _p: PhantomData,
        }
    }
    async fn shutdown(&self) -> anyhow::Result<()> {
        info!("shutting down chat controller");
        self.inner.shutdown().await
    }
    fn chat_presence(&self) -> ChatPresence<T> {
        self.presence.clone()
    }
    async fn wait_joined(&self, min_nodes: usize) -> anyhow::Result<()> {
        let bootstrap = self.ticket.bootstrap.clone();
        if bootstrap.is_empty() {
            return Ok(());
        }
        let own_node_id = *self.node_identity.node_id();
        let mut attempts = 0;
        let p = self.chat_presence();
        loop {
            // Count distinct nodes present in the room. We always count
            // ourselves: we are trivially "in" the room, but our own presence
            // entry only appears after the first presence broadcast.
            let mut nodes = p
                .get_presence_list()
                .await
                .0
                .into_iter()
                .map(|p| *p.identity.node_id())
                .collect::<std::collections::BTreeSet<_>>();
            nodes.insert(own_node_id);
            info!(
                "wait_until_joined: found {}/{} nodes (room bootstrap: {}), attempts: {}",
                nodes.len(),
                min_nodes,
                bootstrap.len(),
                attempts
            );
            if nodes.len() >= min_nodes || attempts >= 100 {
                break;
            }
            let _ = n0_future::time::timeout(CONNECT_TIMEOUT / 10, p.notified()).await;
            attempts += 1;
        }
        Ok(())
    }
}

/// Chat message types for a room type.
/// Variants: user message, presence update, or ping/pong response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ChatMessage<T: IChatRoomType> {
    /// Typed user message payload for the room.
    Message(T::M),
    /// Presence update broadcast to peers; `None` when presence has not been set.
    Presence(Option<T::P>),
    /// Round-trip latency probe response referencing the sender's ping timestamp.
    Pong {
        /// Timestamp from the original ping/presence message used to compute RTT.
        ping_sender_ts: DateTime<Utc>,
    },
}

#[async_trait::async_trait]
/// Interface for chat room control: identity, send/receive, presence, and lifecycle.
pub trait IChatController<T: IChatRoomType>: Send + Sync + 'static + std::fmt::Debug {
    /// Returns this controller's node identity.
    fn node_identity(&self) -> NodeIdentity;
    /// Returns a clone of the room's message sender.
    fn sender(&self) -> ChatSender<T>;
    /// Returns a fresh receiver subscribed to incoming messages.
    async fn receiver(&self) -> ChatReceiver<T>;
    /// Shuts down the underlying raw room and background tasks.
    async fn shutdown(&self) -> anyhow::Result<()>;
    /// Returns the room's presence tracker.
    fn chat_presence(&self) -> ChatPresence<T>;
    /// Wait until the room's presence list (self always included) holds at
    /// least `min_nodes` distinct nodes. Use 2 for rooms where another node's
    /// *typed presence* is guaranteed (global chat: us + a bootstrap node;
    /// server chat: us + the server). Use 1 for rooms bootstrap nodes only
    /// subscribe to raw, without typed presence (the gameplay room), where
    /// waiting for anyone else would just spin ~30s.
    async fn wait_joined(&self, min_nodes: usize) -> anyhow::Result<()>;
}

#[derive(Clone, Debug)]
/// Outbound handle for broadcasting and direct messaging in a chat room.
pub struct ChatSender<T: IChatRoomType> {
    inner: Arc<dyn IChatRoomRaw>,
    message_signer: MessageSigner,
    current_presence: Arc<RwLock<Option<T::P>>>,
    chatroom_presence: ChatPresence<T>,
    _p: PhantomData<T>,
}

#[async_trait::async_trait]
impl<T: IChatRoomType> IChatSender<T> for ChatSender<T> {
    async fn broadcast_message(&self, message: T::M) -> anyhow::Result<ReceivedMessage<T>> {
        let message2 = ChatMessage::<T>::Message(message.clone());
        let (bytes, sent_preview) = self.message_signer.sign_and_encode(message2)?;
        self.inner.broadcast_message(bytes).await?;
        let sent_preview = ReceivedMessage::<T> {
            _sender_timestamp: sent_preview._timestamp,
            _received_timestamp: datetime_now(),
            _message_id: sent_preview._message_id,
            from: sent_preview.from,
            message,
        };
        Ok(sent_preview)
    }
    async fn direct_message(
        &self,
        to: NodeIdentity,
        message: T::M,
    ) -> anyhow::Result<ReceivedMessage<T>> {
        let message2 = ChatMessage::<T>::Message(message.clone());
        let (bytes, sent_preview) = self.message_signer.sign_and_encode(message2)?;
        self.inner.direct_message(to, bytes).await?;
        let received_message = ReceivedMessage::<T> {
            _sender_timestamp: sent_preview._timestamp,
            _received_timestamp: datetime_now(),
            _message_id: sent_preview._message_id,
            from: sent_preview.from,
            message,
        };
        Ok(received_message)
    }
    async fn join_peers(&self, peers: Vec<NodeId>) -> anyhow::Result<()> {
        self.inner.join_peers(peers).await
    }
    async fn set_presence(&self, presence: &T::P) {
        self.current_presence
            .write()
            .await
            .replace(presence.clone());
        if let Err(e) = self.broadcast_presence().await {
            warn!("Error broadcasting presence: {:?}", e);
        }
    }
}

impl<T: IChatRoomType> ChatSender<T> {
    async fn broadcast_presence(&self) -> anyhow::Result<()> {
        let presence = { self.current_presence.read().await.clone() };
        self.chatroom_presence
            .add_presence(&self.message_signer.node_identity, &presence)
            .await;
        let presence = ChatMessage::<T>::Presence(presence);
        let (presence, _) = self.message_signer.sign_and_encode(presence)?;
        self.inner.broadcast_message(presence).await
    }
    async fn direct_presence(&self, to: NodeIdentity) -> anyhow::Result<()> {
        let presence = { self.current_presence.read().await.clone() };
        let presence = ChatMessage::<T>::Presence(presence);
        let (presence, _) = self.message_signer.sign_and_encode(presence)?;
        self.inner.direct_message(to, presence).await
    }
    async fn direct_pong(
        &self,
        to: NodeIdentity,
        ping_sender_ts: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let pong = ChatMessage::<T>::Pong { ping_sender_ts };
        let (pong, _) = self.message_signer.sign_and_encode(pong)?;
        self.inner.direct_message(to, pong).await
    }
}

#[async_trait::async_trait]
/// Interface for sending signed messages and managing presence in a chat room.
pub trait IChatSender<T: IChatRoomType>: Send + Sync + 'static + std::fmt::Debug {
    /// Signs and broadcasts a typed message to all peers in the room.
    async fn broadcast_message(&self, message: T::M) -> anyhow::Result<ReceivedMessage<T>>;
    /// Signs and sends a typed message directly to a single peer.
    async fn direct_message(
        &self,
        to: NodeIdentity,
        message: T::M,
    ) -> anyhow::Result<ReceivedMessage<T>>;
    /// Requests the raw room to join additional peers for gossip relay.
    async fn join_peers(&self, peers: Vec<NodeId>) -> anyhow::Result<()>;
    /// Updates local presence and broadcasts it to peers.
    async fn set_presence(&self, presence: &T::P);
}

#[derive(Clone, Debug)]
/// Inbound handle that streams decoded messages received from the room.
pub struct ChatReceiver<T: IChatRoomType> {
    msg_receiver: Arc<RwLock<async_broadcast::Receiver<ReceivedMessage<T>>>>,
    _p: PhantomData<T>,
}

#[async_trait::async_trait]
impl<T: IChatRoomType> IChatReceiver<T> for ChatReceiver<T> {
    async fn next_message(&self) -> Option<ReceivedMessage<T>> {
        Some(self.msg_receiver.write().await.next().fuse().await?)
    }
}

#[async_trait::async_trait]
/// Interface for receiving decoded messages from a chat room.
pub trait IChatReceiver<T: IChatRoomType>: Send + Sync + 'static + std::fmt::Debug {
    /// Waits for and returns the next received message, or `None` if the stream ended.
    async fn next_message(&self) -> Option<ReceivedMessage<T>>;
}

#[async_trait::async_trait]
/// Low-level transport for a gossip-backed chat room (unsigned bytes on the wire).
pub trait IChatRoomRaw: Send + Sync + 'static + std::fmt::Debug {
    /// Broadcasts opaque signed message bytes to all peers in the room.
    async fn broadcast_message(&self, message: Vec<u8>) -> anyhow::Result<()>;
    /// Sends opaque signed message bytes directly to one peer.
    async fn direct_message(&self, to: NodeIdentity, message: Vec<u8>) -> anyhow::Result<()>;
    /// Returns the next raw message bytes from the room, if any.
    async fn next_message(&self) -> anyhow::Result<Option<Arc<Vec<u8>>>>;
    /// Instructs the room to establish gossip relay to additional peers.
    async fn join_peers(&self, peers: Vec<NodeId>) -> anyhow::Result<()>;
    /// Tears down the room transport.
    async fn shutdown(&self) -> anyhow::Result<()>;
}

// Living-guard regression tests (native only: they need a tokio runtime and
// multi-task scheduling). Both target past production trouble:
//  1. chatcontroller-lifetime-footgun: a `ChatController` must be owned for the
//     room's whole life — cloning the sender/receiver does NOT keep the
//     dispatch/presence tasks alive; dropping the last controller aborts them
//     (`AbortOnDropHandle`) and the room silently dies.
//  2. multiplayer-gossip-join-peers: high-rate gossip rooms need `join_peers`,
//     not bootstrap-only relay, so the sender must expose it end-to-end.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::chat::chat_ticket::ChatTicket;
    use crate::user_identity::UserIdentitySecrets;
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    struct TestRoomType;
    impl IChatRoomType for TestRoomType {
        type M = String;
        type P = String;
        fn default_presence() -> String {
            "online".to_string()
        }
    }

    #[derive(Debug, Default)]
    struct FakeRoomRaw {
        broadcast_log: Mutex<Vec<Vec<u8>>>,
        joined_peers: Mutex<Vec<NodeId>>,
    }

    #[async_trait::async_trait]
    impl IChatRoomRaw for FakeRoomRaw {
        async fn broadcast_message(&self, message: Vec<u8>) -> anyhow::Result<()> {
            self.broadcast_log.lock().unwrap().push(message);
            Ok(())
        }
        async fn direct_message(&self, _to: NodeIdentity, _message: Vec<u8>) -> anyhow::Result<()> {
            Ok(())
        }
        async fn next_message(&self) -> anyhow::Result<Option<Arc<Vec<u8>>>> {
            // No inbound traffic: pend forever so the dispatch task stays alive.
            futures::future::pending::<()>().await;
            unreachable!()
        }
        async fn join_peers(&self, peers: Vec<NodeId>) -> anyhow::Result<()> {
            self.joined_peers.lock().unwrap().extend(peers);
            Ok(())
        }
        async fn shutdown(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn make_controller() -> (ChatController<TestRoomType>, Arc<FakeRoomRaw>) {
        let raw = Arc::new(FakeRoomRaw::default());
        let node_secret_key = iroh::SecretKey::generate(rand::thread_rng());
        let user_secrets = UserIdentitySecrets::generate();
        let node_identity = NodeIdentity::new(
            *user_secrets.user_identity(),
            node_secret_key.public(),
            None,
        );
        let message_signer = MessageSigner {
            node_secret_key: Arc::new(node_secret_key),
            user_secrets: Arc::new(user_secrets),
            node_identity: Arc::new(node_identity),
        };
        let ticket = ChatTicket::new_str_bs("test-room", BTreeSet::new());
        let controller = ChatController::new(
            ticket,
            raw.clone(),
            message_signer,
            SleepManager::new(),
            node_identity,
        );
        (controller, raw)
    }

    #[tokio::test]
    async fn regression_controller_lifetime_footgun() {
        let (controller, raw) = make_controller();
        // Cloning the sender is fine *while the controller is alive*...
        let sender = controller.sender();
        let sent = sender
            .broadcast_message("hello".to_string())
            .await
            .expect("send should work while the controller is alive");
        assert_eq!(sent.message, "hello");
        // ...and the bytes on the wire are a valid signed ChatMessage.
        let log = raw.broadcast_log.lock().unwrap();
        assert_eq!(log.len(), 1);
        let wire = SignedMessage::verify_and_decode::<ChatMessage<TestRoomType>>(&log[0]).unwrap();
        assert!(matches!(wire.message, ChatMessage::Message(ref m) if m == "hello"));
        drop(log);
        // The room is only alive because `controller` is still owned here:
        // its `_dispatch_task`/`_presence_task` are AbortOnDropHandle, so
        // dropping the last controller clone kills the room even if senders
        // or receivers are still around. Keep controllers for the room's life!
        drop(controller);
    }

    #[tokio::test]
    async fn regression_join_peers_reaches_room() {
        let (controller, raw) = make_controller();
        let sender = controller.sender();
        let peer = iroh::SecretKey::generate(rand::thread_rng()).public();
        sender
            .join_peers(vec![peer])
            .await
            .expect("join_peers should reach the raw room");
        assert_eq!(raw.joined_peers.lock().unwrap().as_slice(), &[peer]);
    }
}
