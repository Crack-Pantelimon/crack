use bevy::prelude::*;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use std::sync::Arc;

use crate::plugins::states::NetworkConnectionState;
use game_logic::network::GLOBAL_GAMEPLAY_TOPIC_ID;
use net_crackpipe::{
    chat::chat_controller::{IChatController, IChatReceiver, IChatSender},
    chat::global_chat::{GlobalChatMessageContent, GlobalChatPresence},
    network_manager::NetworkManager,
    user_identity::UserIdentitySecrets,
};

pub mod global_chat_ui;
pub mod multiplayer_plugin;

use multiplayer_plugin::{
    GameSyncChannels, GameSyncInbound, GameplayChatMessageContent, GameplaySyncRoomType,
    MultiplayerStats,
};

#[cfg(not(target_family = "wasm"))]
#[derive(Resource)]
pub struct NetworkRuntime(pub Arc<tokio::runtime::Runtime>);

#[derive(Resource)]
pub struct ChatState {
    pub own_nickname: String,
    pub own_color: (u8, u8, u8),
    pub presence_list: Vec<(String, (u8, u8, u8))>,
    pub msg_history: Vec<(String, String, (u8, u8, u8))>, // (nickname, text, rgb_color)
    pub status_message: String,
    pub input_buffer: String,
    pub outgoing_tx: async_channel::Sender<String>,
    pub incoming_rx: async_channel::Receiver<ChatEvent>,
    /// Number of chat messages that have arrived since the user last had the
    /// chat window open. Reset to 0 by the chat UI while the window is visible.
    pub unread_count: u32,
}

pub enum ChatEvent {
    Connected,
    GameplayConnected,
    Message {
        nickname: String,
        text: String,
        color: (u8, u8, u8),
    },
    PresenceUpdate(Vec<(String, (u8, u8, u8))>),
    StatusUpdate(String),
}

use crate::plugins::crack_plugin::CrackClient;

#[derive(Resource, Default)]
pub struct NetworkSetupState {
    pub started: bool,
    pub slot: Arc<std::sync::Mutex<Option<anyhow::Result<(UserIdentitySecrets, CrackClient)>>>>,
}

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<NetworkConnectionState>();

        #[cfg(not(target_family = "wasm"))]
        {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            app.insert_resource(NetworkRuntime(Arc::new(rt)));
        }

        app.init_resource::<NetworkSetupState>();
        app.add_systems(
            Update,
            (
                trigger_network_setup,
                install_network_setup,
                drain_chat_events,
            ),
        );
        app.add_plugins(multiplayer_plugin::MultiplayerPlugin);
    }
}

fn trigger_network_setup(
    mut setup_state: ResMut<NetworkSetupState>,
    client: Option<Res<CrackClient>>,
    #[cfg(not(target_family = "wasm"))] rt: Option<Res<NetworkRuntime>>,
) {
    if setup_state.started {
        return;
    }
    let Some(client) = client else {
        return;
    };
    setup_state.started = true;

    let slot = setup_state.slot.clone();
    let client_clone = client.clone();

    let future = async move {
        let secrets_res = init_network_secrets(&client_clone).await;
        *slot.lock().unwrap() = Some(secrets_res.map(|secrets| (secrets, client_clone)));
    };

    #[cfg(target_family = "wasm")]
    {
        wasm_bindgen_futures::spawn_local(future);
    }
    #[cfg(not(target_family = "wasm"))]
    {
        if let Some(rt) = rt {
            rt.0.spawn(future);
        }
    }
}

async fn init_network_secrets(client: &CrackClient) -> anyhow::Result<UserIdentitySecrets> {
    let sql = "SELECT secret_key FROM user_secrets WHERE id = 1 LIMIT 1".to_string();
    let res = client.0.call::<storage_crackhouse::api::ExecuteSQL2>(sql).await;

    match res {
        Ok(set) if !set.rows.is_empty() => {
            let row = &set.rows[0];
            let val = &row.cols[0];
            let json_str = match val {
                storage_crackhouse::types::DbValue::Text(s) => s.clone(),
                _ => anyhow::bail!("Invalid secret_key type in DB"),
            };
            let secrets: UserIdentitySecrets = serde_json::from_str(&json_str)?;
            Ok(secrets)
        }
        _ => {
            let secrets = UserIdentitySecrets::generate();
            let json_str = serde_json::to_string(&secrets)?;
            let escaped_json = json_str.replace('\'', "''");
            let sql = format!(
                "INSERT OR REPLACE INTO user_secrets (id, secret_key) VALUES (1, '{}')",
                escaped_json
            );
            client.0.call::<storage_crackhouse::api::ExecuteSQL2>(sql).await?;
            Ok(secrets)
        }
    }
}

fn install_network_setup(
    mut commands: Commands,
    setup_state: Res<NetworkSetupState>,
    #[cfg(not(target_family = "wasm"))] rt: Option<Res<NetworkRuntime>>,
    proxy_wrapper: Res<EventLoopProxyWrapper>,
) {
    let mut guard = setup_state.slot.lock().unwrap();
    if let Some(res) = guard.take() {
        match res {
            Ok((secrets, _client)) => {
                let user_id = secrets.user_identity();
                let own_nickname = user_id.nickname().to_string();
                let own_color = user_id.rgb_color();

                let (incoming_tx, incoming_rx) = async_channel::unbounded::<ChatEvent>();
                let (outgoing_tx, outgoing_rx) = async_channel::bounded::<String>(100);

                let (game_outgoing_tx, game_outgoing_rx) = async_channel::bounded::<Vec<u8>>(256);
                let (game_incoming_tx, game_incoming_rx) = async_channel::bounded::<GameSyncInbound>(256);

                commands.insert_resource(GameSyncChannels {
                    outgoing_tx: game_outgoing_tx,
                    incoming_rx: game_incoming_rx,
                });

                commands.insert_resource(ChatState {
                    own_nickname,
                    own_color,
                    presence_list: Vec::new(),
                    msg_history: Vec::new(),
                    status_message: "Initializing...".to_string(),
                    input_buffer: String::new(),
                    outgoing_tx,
                    incoming_rx,
                    unread_count: 0,
                });

                let future = chat_main_task(
                    secrets,
                    incoming_tx,
                    outgoing_rx,
                    game_incoming_tx,
                    game_outgoing_rx,
                    proxy_wrapper.clone(),
                );

                #[cfg(target_family = "wasm")]
                {
                    _crack_utils::n0_future::task::spawn(future);
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    if let Some(rt) = rt {
                        rt.0.spawn(future);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to initialize network setup: {:?}", e);
            }
        }
    }
}

async fn chat_main_task(
    secrets: UserIdentitySecrets,
    incoming_tx: async_channel::Sender<ChatEvent>,
    outgoing_rx: async_channel::Receiver<String>,
    game_incoming_tx: async_channel::Sender<GameSyncInbound>,
    game_outgoing_rx: async_channel::Receiver<Vec<u8>>,
    proxy: EventLoopProxy<WinitUserEvent>,
) {
    let send_status = |status: String| {
        let _ = incoming_tx.try_send(ChatEvent::StatusUpdate(status));
        let _ = proxy.send_event(WinitUserEvent::WakeUp);
    };

    send_status("Connecting to server...".to_string());
    // Global network init: own node + bootstrap endpoint if a slot is free
    // (that bootstrap then also carries the gameplay topic) + global chat +
    // periodic maintenance. One object owns all running network code.
    let network = match NetworkManager::init(
        Arc::new(secrets),
        game_logic::network::network_manager_config(),
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            send_status(format!("Error: {:?}", e));
            return;
        }
    };
    let global_mm = network.matchmaker();

    send_status("Connecting to chat...".to_string());
    let controller = match network.global_chat_controller().await {
        Some(c) => c,
        None => {
            send_status("Failed to get chat controller".to_string());
            return;
        }
    };

    let presence = controller.chat_presence();
    let sender = controller.sender();

    sender
        .set_presence(&GlobalChatPresence {
            url: "".to_string(),
            platform: "Bevy Egui Chat".to_string(),
            is_server: None,
        })
        .await;

    send_status("Waiting to join chat room...".to_string());
    // 2 nodes = us + our bootstrap node.
    let _ = controller.wait_joined(2).await;

    // Join gameplay room. The NetworkManager builds the ticket from every
    // known bootstrap node (which all subscribe to the gameplay topic and
    // drop its traffic), so two players joining at the same time still meet;
    // it also runs the presence-driven join_peers refresher for the room.
    let gameplay_controller = match network
        .join_room::<GameplaySyncRoomType>(GLOBAL_GAMEPLAY_TOPIC_ID)
        .await
    {
        Ok(c) => {
            let gameplay_c = c.clone();
            let incoming_tx_gp = incoming_tx.clone();
            let proxy_gp = proxy.clone();
            _crack_utils::n0_future::task::spawn(async move {
                // Bootstrap nodes subscribe to the gameplay topic raw (no
                // typed presence), so presence-wise we can only ever count
                // ourselves until another player joins — wait for 1 node.
                let _ = gameplay_c.wait_joined(1).await;
                let _ = incoming_tx_gp.try_send(ChatEvent::GameplayConnected);
                let _ = proxy_gp.send_event(WinitUserEvent::WakeUp);
            });
            Some(c)
        }
        Err(e) => {
            tracing::error!("Failed to join gameplay chat: {:?}", e);
            None
        }
    };

    send_status("Connected to rooms!".to_string());
    let _ = incoming_tx.try_send(ChatEvent::Connected);
    let _ = proxy.send_event(WinitUserEvent::WakeUp);

    // Send initial presence update
    let presence_list = presence.get_presence_list().await;
    let mut list = Vec::new();
    for item in presence_list.0 {
        // Hide bootstrap nodes from the presence list; only show real users.
        if item.identity.bootstrap_idx().is_some() {
            continue;
        }
        list.push((
            item.identity.nickname().to_string(),
            item.identity.rgb_color(),
        ));
    }
    let _ = incoming_tx.try_send(ChatEvent::PresenceUpdate(list));
    let _ = proxy.send_event(WinitUserEvent::WakeUp);

    // Start message receiver
    let recv = controller.receiver().await;

    let presence_clone = presence.clone();
    let incoming_tx_presence = incoming_tx.clone();
    let proxy_presence = proxy.clone();

    // Spawn task to check presence updates
    _crack_utils::n0_future::task::spawn(async move {
        loop {
            presence_clone.notified().await;
            let presence_list = presence_clone.get_presence_list().await;
            let mut list = Vec::new();
            for item in presence_list.0 {
                // Hide bootstrap nodes from the presence list; only show real users.
                if item.identity.bootstrap_idx().is_some() {
                    continue;
                }
                list.push((
                    item.identity.nickname().to_string(),
                    item.identity.rgb_color(),
                ));
            }
            if incoming_tx_presence
                .try_send(ChatEvent::PresenceUpdate(list))
                .is_err()
            {
                break;
            }
            let _ = proxy_presence.send_event(WinitUserEvent::WakeUp);
        }
    });

    // Spawn task to handle outgoing messages from Bevy
    let sender_clone = sender.clone();
    let incoming_tx_msg = incoming_tx.clone();
    let proxy_outgoing = proxy.clone();
    _crack_utils::n0_future::task::spawn(async move {
        while let Ok(text) = outgoing_rx.recv().await {
            let msg = GlobalChatMessageContent::TextMessage { text: text.clone() };
            match sender_clone.broadcast_message(msg).await {
                Ok(sent_preview) => {
                    let nickname = sent_preview.from.nickname().to_string();
                    let color = sent_preview.from.rgb_color();
                    let _ = incoming_tx_msg.try_send(ChatEvent::Message {
                        nickname,
                        text,
                        color,
                    });
                    let _ = proxy_outgoing.send_event(WinitUserEvent::WakeUp);
                }
                Err(e) => {
                    eprintln!("Error sending message: {:?}", e);
                }
            }
        }
    });

    // Spawn tasks for gameplay sync if joined successfully.
    //
    // IMPORTANT: borrow, don't move. `gameplay_controller` must stay alive for
    // the whole lifetime of this task: the ChatController owns the room's
    // message-dispatch and presence-heartbeat tasks via AbortOnDropHandle, and
    // the ChatSender/ChatReceiver clones used below do NOT keep them alive.
    // (Previously the last clone lived in the wait_joined task above, so ~30s
    // after startup the controller dropped, dispatch/heartbeat aborted, and
    // every peer silently "disconnected".)
    if let Some(gameplay_c) = gameplay_controller.as_ref() {
        let gameplay_sender = gameplay_c.sender();
        let game_outgoing_rx_clone = game_outgoing_rx.clone();

        // Populate the gameplay room's presence roster. The controller's
        // presence task then heartbeats this every PRESENCE_INTERVAL, which is
        // what keeps peers visible to each other independently of the high-rate
        // GameSync stream.
        gameplay_c
            .sender()
            .set_presence(&multiplayer_plugin::GameplayPresence::default())
            .await;

        // The presence-driven direct-mesh join_peers refresher for this room
        // is spawned by NetworkManager::join_room — without it the 20 Hz
        // GameSync broadcast would be relayed through bootstrap nodes, whose
        // mesh drops the peer under load ("de-sync" bug).

        // Forward outgoing gameplay messages
        _crack_utils::n0_future::task::spawn(async move {
            while let Ok(payload) = game_outgoing_rx_clone.recv().await {
                let id = rand::random::<i64>();
                let msg = GameplayChatMessageContent::GameSync { id, payload };
                if let Err(e) = gameplay_sender.broadcast_message(msg).await {
                    tracing::error!("Error sending game sync message: {:?}", e);
                }
            }
        });

        // Loop to handle incoming gameplay messages
        let gameplay_recv = gameplay_c.receiver().await;
        let game_incoming_tx_clone = game_incoming_tx.clone();
        let proxy_gameplay_in = proxy.clone();
        let own_node_id = *global_mm.own_node_identity().node_id();
        let msg_join_sender = gameplay_c.sender();

        _crack_utils::n0_future::task::spawn(async move {
            let mut joined_peers: std::collections::HashSet<net_crackpipe::PublicKey> =
                std::collections::HashSet::new();
            while let Some(msg) = gameplay_recv.next_message().await {
                if *msg.from.node_id() == own_node_id {
                    continue; // Skip own loopback
                }

                let from_node_id = *msg.from.node_id();

                // First time we hear from a peer, force a direct gossip
                // connection immediately (don't wait for the ~7s presence
                // heartbeat). Idempotent and cheap; the presence loop refreshes
                // it afterwards.
                if joined_peers.insert(from_node_id) {
                    let js = msg_join_sender.clone();
                    _crack_utils::n0_future::task::spawn(async move {
                        if let Err(e) = js.join_peers(vec![from_node_id]).await {
                            tracing::warn!("gameplay msg join_peers failed: {:?}", e);
                        }
                    });
                }

                let nickname = msg.from.nickname().to_string();
                let color = msg.from.rgb_color();

                match msg.message {
                    GameplayChatMessageContent::GameSync { id, payload } => {
                        let inbound = GameSyncInbound {
                            from_node_id,
                            nickname,
                            color,
                            id,
                            payload,
                        };
                        let _ = game_incoming_tx_clone.try_send(inbound);
                        let _ = proxy_gameplay_in.send_event(WinitUserEvent::WakeUp);
                    }
                }
            }
        });
    }

    // Loop to handle incoming global chat messages
    tracing::info!("Starting bevy chat incoming loop...");
    loop {
        match recv.next_message().await {
            Some(msg) => {
                let nickname = msg.from.nickname().to_string();
                let color = msg.from.rgb_color();
                tracing::info!("Bevy received message from {}: {:?}", nickname, msg.message);
                match msg.message {
                    GlobalChatMessageContent::TextMessage { text } => {
                        if incoming_tx
                            .try_send(ChatEvent::Message {
                                nickname,
                                text,
                                color,
                            })
                            .is_err()
                        {
                            tracing::warn!("incoming_tx send error, exiting loop");
                            break;
                        }
                        let _ = proxy.send_event(WinitUserEvent::WakeUp);
                    }
                    _ => {
                        tracing::info!("Received non-text message: {:?}", msg.message);
                    }
                }
            }
            None => {
                tracing::warn!("recv.next_message() returned None, exiting loop");
                break;
            }
        }
    }

    // Keeps the gameplay room's dispatch/presence tasks alive until here.
    drop(gameplay_controller);
    let _ = network.shutdown().await;
}

fn drain_chat_events(
    state: Option<ResMut<ChatState>>,
    mut next_state: ResMut<NextState<NetworkConnectionState>>,
    mut commands: Commands,
    mut stats: Option<ResMut<MultiplayerStats>>,
) {
    let Some(mut state) = state else {
        return;
    };
    while let Ok(event) = state.incoming_rx.try_recv() {
        match event {
            ChatEvent::Connected => {
                info!("P2P network connected.");
                state.status_message = "Connected!".to_string();
                next_state.set(NetworkConnectionState::Connected);
                commands
                    .trigger(crate::plugins::notifications::NotificationEvent::NetworkConnected);
            }
            ChatEvent::GameplayConnected => {
                info!("Gameplay chat network connected.");
                commands.trigger(crate::plugins::notifications::NotificationEvent::GameNetworkOk);
                if let Some(ref mut stats) = stats {
                    stats.connected = true;
                }
            }
            ChatEvent::Message {
                nickname,
                text,
                color,
            } => {
                state.msg_history.push((nickname, text, color));
                // Badge counter; cleared by the chat UI while the window is open.
                state.unread_count = state.unread_count.saturating_add(1);
            }
            ChatEvent::PresenceUpdate(list) => {
                state.presence_list = list;
            }
            ChatEvent::StatusUpdate(status) => {
                state.status_message = status;
            }
        }
    }
}
