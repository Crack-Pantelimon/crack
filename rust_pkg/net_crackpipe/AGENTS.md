

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

### src/chat/chat_const.rs
```
pub fn get_relay_domain() → (String, String)  :11-16
```

### src/chat/chat_controller.rs
```
pub struct ChatController  :23-34
pub struct ChatSender  :275-281
pub struct ChatReceiver  :369-372
pub enum ChatMessage  :252-256
pub trait IChatController  :259-259
pub trait IChatSender  :357-357
pub trait IChatReceiver  :382-382
pub trait IChatRoomRaw  :387-387
impl ChatController  :36-40
impl ChatController  :83-183
impl IChatController  :186-249
impl IChatSender  :284-327
impl ChatSender  :329-354
impl IChatReceiver  :375-379
```

### src/chat/chat_presence.rs
```
pub struct ChatPresence  :14-17
pub struct PresenceList  :43-49
pub struct PresenceListItem  :52-58
pub enum PresenceFlag  :20-25
impl PresenceFlag  :27-40
  pub fn from_instant(instant: i64) → Self  :28-28
impl PresenceList  :45-49
impl ChatPresence  :60-145
  pub fn new() → Self  :61-61
  pub fn notified(&self) → tokio::sync::futures::Notif...  :67-67
  pub async fn add_presence(&self, identity: &NodeIdentity, payload: &Option<T::P>) → bool  :71-71
  pub async fn update_ping(&self, identity: &NodeIdentity, rtt: u16)  :105-105
  pub async fn get_presence_list(&self) → PresenceList<T::P>  :113-113
  pub async fn remove_presence(&self, identity: &NodeIdentity)  :138-138
impl ChatPresenceData  :152-158
```

### src/chat/chat_ticket.rs
```
pub struct ChatTicket  :8-11
impl ChatTicket  :13-25
  pub fn new_str_bs(topic_id: &str, bs: BTreeSet<NodeId>) → Self  :14-14
```

### src/chat/direct_message.rs
```
pub struct ChatDirectMessage  :20-20
pub struct DirectMessageProtocol  :23-29
impl DirectMessageProtocol  :31-96
  pub async fn shutdown(&self)  :32-32
  pub async fn new(received_message_broadcaster: async_broadcast::Sender<(PublicKey, T) → Self  :41-45
  pub async fn send_direct_message(&self, iroh_target: PublicKey, payload: T,) → anyhow::Result<()>  :88-92
impl DirectMessageProtocol  :98-102
impl MessageDispatchers  :110-141
  pub fn new(endpoint: Endpoint) → Self  :111-111
  pub async fn shutdown(&self)  :118-118
  pub async fn drop_dispatcher(&self, target: PublicKey)  :133-133
  pub async fn send_message(&self, target: PublicKey, payload: T) → anyhow::Result<()>  :137-137
impl MessageDispatcher  :149-195
  pub fn new(target: PublicKey, endpoint: Endpoint) → Self  :150-150
  pub async fn send_message(&self, payload: T) → anyhow::Result<()>  :191-191
```

### src/chat/global_chat.rs
```
pub struct GlobalChatRoomType  :6-6
pub struct GlobalChatPresence  :16-20
pub enum GlobalChatMessageContent  :24-36
pub enum GlobalChatBootstrapQuery  :40-43
pub enum MatchHandshakeType  :46-51
impl GlobalChatRoomType  :8-14
```

### src/echo.rs
```
pub struct Echo  :9-12
impl Echo  :14-22
  pub fn new(own_endpoint_node_id: NodeId, sleep_manager: SleepManager) → Self  :16-16
impl Echo  :24-32
impl Echo  :34-76
```

### src/global_matchmaker.rs
```
pub struct GlobalMatchmaker  :39-48
pub struct BootstrapNodeInfo  :98-104
impl GlobalMatchmakerInner  :65-89
  pub async fn shutdown(&mut self) → Result<()>  :66-66
impl GlobalMatchmaker  :91-95
impl GlobalMatchmaker  :106-247
  pub async fn sleep(&self, duration: Duration)  :107-107
  pub async fn shutdown(&self) → Result<()>  :110-110
  pub fn user_secrets(&self) → std::sync::Arc<UserIdentity...  :122-122
  pub fn own_node_identity(&self) → NodeIdentity  :125-125
  pub fn user(&self) → UserIdentity  :132-132
  pub async fn global_chat_controller(&self) → Option<ChatController<Globa...  :136-136
  pub async fn bs_global_chat_controller(&self) → Option<ChatController<Globa...  :139-139
  pub async fn display_debug_info(&self) → Result<String>  :142-142
```

### src/lib.rs
```
pub fn timestamp_micros() → u128  :15-20
pub fn datetime_now() → DateTime<Utc>  :22-25
```

### src/main_node.rs
```
pub struct MainNode  :28-37
impl MainNode  :73-175
  pub async fn spawn(node_identity: Arc<NodeIdentity>, node_secret_key: Arc<SecretKey>, own_endpoint_node_id: Option<NodeId>, user_secrets: Arc<UserIdentitySecrets>, sleep_manager: SleepManager,) → Result<Self>  :74-80
  pub fn user(&self) → &NodeIdentity  :123-123
  pub fn endpoint(&self) → &Endpoint  :126-126
  pub fn node_id(&self) → NodeId  :129-129
  pub fn remote_info(&self) → Vec<RemoteInfo>  :132-132
  pub fn node_identity(&self) → &NodeIdentity  :138-138
  pub async fn shutdown(&self) → Result<()>  :141-141
  pub async fn join_chat(&self, ticket: &ChatTicket) → Result<ChatController<T>> w...  :153-156
```

### src/network_manager.rs
```
pub struct NetworkManagerConfig  :44-52
pub struct NetworkManager  :59-62
impl NetworkManager  :64-133
pub async fn init  :70-73
pub fn matchmaker  :81-81
pub async fn global_chat_controller  :85-85
pub async fn join_room  :99-99
pub async fn shutdown  :129-129
pub async fn run_standalone_bootstrap_if_needed  :220-310
```

### src/signed_message.rs
```
pub struct SignedMessage  :37-43
pub struct MessageSigner  :72-76
pub struct WireMessage  :114-119
pub struct ReceivedMessage  :122-128
pub enum ChatMessage  :136-139
pub trait AcceptableType  :13-13
pub trait IChatRoomType  :130-130
impl SignedMessage  :45-69
  pub fn verify_and_decode(bytes: &[u8]) → Result<WireMessage<T>>  :46-46
impl MessageSigner  :78-111
  pub fn sign_and_encode(&self, message: T,) → Result<(Vec<u8>, WireMessag...  :79-82
```

### src/sleep.rs
```
pub struct SleepManager  :7-9
impl SleepManager  :11-23
  pub fn new() → Self  :12-12
  pub async fn sleep(&self, duration: Duration)  :17-17
  pub fn wake_up(&self)  :20-20
impl SleepManagerInner  :30-52
```

### src/user_identity.rs
```
pub struct UserIdentity  :6-8
pub struct UserIdentitySecrets  :36-39
pub struct NodeIdentity  :69-73
impl UserIdentity  :10-33
  pub fn nickname(&self) → String  :11-11
  pub fn user_id(&self) → &PublicKey  :17-17
  pub fn html_color(&self) → String  :20-20
  pub fn rgb_color(&self) → (u8, u8, u8)  :24-24
impl UserIdentitySecrets  :41-46
impl UserIdentitySecrets  :48-64
  pub fn user_identity(&self) → &UserIdentity  :49-49
  pub fn secret_key(&self) → &SecretKey  :52-52
  pub fn generate() → Self  :55-55
impl NodeIdentity  :75-117
  pub fn nickname(&self) → String  :76-76
  pub fn html_color(&self) → String  :87-87
  pub fn rgb_color(&self) → (u8, u8, u8)  :90-90
  pub fn user_id(&self) → &PublicKey  :93-93
  pub fn node_id(&self) → &PublicKey  :96-96
  pub fn user_identity(&self) → &UserIdentity  :99-99
  pub fn bootstrap_idx(&self) → Option<u32>  :102-102
  pub fn new(user_identity: UserIdentity, node_id: PublicKey, bootstrap_idx: Option<u32>,) → Self  :106-110
```
