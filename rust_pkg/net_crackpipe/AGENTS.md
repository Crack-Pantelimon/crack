

## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## src

### src/_random_word.rs
```
pub fn get_nickname_from_pubkey(pubkey: PublicKey) → String
```

### src/chat/chat_const.rs
```
pub fn get_relay_domain() → (String, String)
```

### src/chat/chat_controller.rs
```
pub struct ChatController
pub struct ChatSender
pub struct ChatReceiver
pub enum ChatMessage
pub trait IChatController
pub trait IChatSender
pub trait IChatReceiver
pub trait IChatRoomRaw
impl ChatController
impl ChatController
impl IChatController
impl IChatSender
impl ChatSender
impl IChatReceiver
```

### src/chat/chat_presence.rs
```
pub struct ChatPresence
pub struct PresenceList
pub struct PresenceListItem
pub enum PresenceFlag
impl PresenceFlag
  pub fn from_instant(instant: i64) → Self
impl PresenceList
impl ChatPresence
  pub fn new() → Self
  pub fn notified(&self) → tokio::sync::futures::Notif...
  pub async fn add_presence(&self, identity: &NodeIdentity, payload: &Option<T::P>) → bool
  pub async fn update_ping(&self, identity: &NodeIdentity, rtt: u16)
  pub async fn get_presence_list(&self) → PresenceList<T::P>
  pub async fn remove_presence(&self, identity: &NodeIdentity)
impl ChatPresenceData
```

### src/chat/chat_ticket.rs
```
pub struct ChatTicket
impl ChatTicket
  pub fn new_str_bs(topic_id: &str, bs: BTreeSet<NodeId>) → Self
```

### src/chat/direct_message.rs
```
pub struct ChatDirectMessage
pub struct DirectMessageProtocol
impl DirectMessageProtocol
  pub async fn shutdown(&self)
  pub async fn new(received_message_broadcaster: async_broadcast::Sender<(PublicKey, T) → Self
  pub async fn send_direct_message(&self, iroh_target: PublicKey, payload: T,) → anyhow::Result<()>
impl DirectMessageProtocol
impl MessageDispatchers
  pub fn new(endpoint: Endpoint) → Self
  pub async fn shutdown(&self)
  pub async fn drop_dispatcher(&self, target: PublicKey)
  pub async fn send_message(&self, target: PublicKey, payload: T) → anyhow::Result<()>
impl MessageDispatcher
  pub fn new(target: PublicKey, endpoint: Endpoint) → Self
  pub async fn send_message(&self, payload: T) → anyhow::Result<()>
```

### src/chat/global_chat.rs
```
pub struct GlobalChatRoomType
pub struct GlobalChatPresence
pub enum GlobalChatMessageContent
pub enum GlobalChatBootstrapQuery
pub enum MatchHandshakeType
impl GlobalChatRoomType
```

### src/chat/room_raw.rs
```
pub struct GossipChatRoom
impl GossipChatRoom
  pub async fn new(node: &MainNode, ticket: &ChatTicket) → Result<Self>
impl GossipChatRoom
```

### src/echo.rs
```
pub struct Echo
impl Echo
  pub fn new(own_endpoint_node_id: NodeId, sleep_manager: SleepManager) → Self
impl Echo
impl Echo
```

### src/global_matchmaker.rs
```
pub struct GlobalMatchmaker
pub struct BootstrapNodeInfo
impl GlobalMatchmakerInner
  pub async fn shutdown(&mut self) → Result<()>
impl GlobalMatchmaker
impl GlobalMatchmaker
  pub async fn sleep(&self, duration: Duration)
  pub async fn shutdown(&self) → Result<()>
  pub fn user_secrets(&self) → std::sync::Arc<UserIdentity...
  pub fn own_node_identity(&self) → NodeIdentity
  pub fn user(&self) → UserIdentity
  pub async fn global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn bs_global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn display_debug_info(&self) → Result<String>
```

### src/lib.rs
```
pub fn timestamp_micros() → u128
pub fn datetime_now() → DateTime<Utc>
```

### src/main_node.rs
```
pub struct MainNode
impl MainNode
  pub async fn spawn(node_identity: Arc<NodeIdentity>, node_secret_key: Arc<SecretKey>, own_endpoint_node_id: Option<NodeId>, user_secrets: Arc<UserIdentitySecrets>, sleep_manager: SleepManager,) → Result<Self>
  pub fn user(&self) → &NodeIdentity
  pub fn endpoint(&self) → &Endpoint
  pub fn node_id(&self) → NodeId
  pub fn remote_info(&self) → Vec<RemoteInfo>
  pub fn node_identity(&self) → &NodeIdentity
  pub async fn shutdown(&self) → Result<()>
  pub async fn join_chat(&self, ticket: &ChatTicket) → Result<ChatController<T>> w...
```

### src/network_manager.rs
```
pub struct NetworkManagerConfig
pub struct NetworkManager
impl NetworkManager
  pub async fn init(secrets: Arc<UserIdentitySecrets>, config: NetworkManagerConfig,) → Result<Self>
  pub fn matchmaker(&self) → GlobalMatchmaker
  pub async fn global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn join_room(&self, topic_id: &str) → Result<ChatController<T>>
  pub async fn shutdown(&self) → Result<()>
pub async fn run_standalone_bootstrap_if_needed(extra_topics: Vec<String>) → Result<()>
```

### src/signed_message.rs
```
pub struct SignedMessage
pub struct MessageSigner
pub struct WireMessage
pub struct ReceivedMessage
pub enum ChatMessage
pub trait AcceptableType
pub trait IChatRoomType
impl SignedMessage
  pub fn verify_and_decode(bytes: &[u8]) → Result<WireMessage<T>>
impl MessageSigner
  pub fn sign_and_encode(&self, message: T,) → Result<(Vec<u8>, WireMessag...
```

### src/sleep.rs
```
pub struct SleepManager
impl SleepManager
  pub fn new() → Self
  pub async fn sleep(&self, duration: Duration)
  pub fn wake_up(&self)
impl SleepManagerInner
```

### src/user_identity.rs
```
pub struct UserIdentity
pub struct UserIdentitySecrets
pub struct NodeIdentity
impl UserIdentity
  pub fn nickname(&self) → String
  pub fn user_id(&self) → &PublicKey
  pub fn html_color(&self) → String
  pub fn rgb_color(&self) → (u8, u8, u8)
impl UserIdentitySecrets
impl UserIdentitySecrets
  pub fn user_identity(&self) → &UserIdentity
  pub fn secret_key(&self) → &SecretKey
  pub fn generate() → Self
impl NodeIdentity
  pub fn nickname(&self) → String
  pub fn html_color(&self) → String
  pub fn rgb_color(&self) → (u8, u8, u8)
  pub fn user_id(&self) → &PublicKey
  pub fn node_id(&self) → &PublicKey
  pub fn user_identity(&self) → &UserIdentity
  pub fn bootstrap_idx(&self) → Option<u32>
  pub fn new(user_identity: UserIdentity, node_id: PublicKey, bootstrap_idx: Option<u32>,) → Self
```
