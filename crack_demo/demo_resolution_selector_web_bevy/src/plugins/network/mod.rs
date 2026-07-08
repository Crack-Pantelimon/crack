use bevy::prelude::*;
use bevy::winit::{EventLoopProxy, EventLoopProxyWrapper, WinitUserEvent};
use std::sync::Arc;

use crate::plugins::states::NetworkConnectionState;
use net_crackpipe::{
    chat::chat_controller::{IChatController, IChatReceiver, IChatSender},
    chat::chat_ticket::ChatTicket,
    chat::global_chat::{GlobalChatMessageContent, GlobalChatPresence},
    global_matchmaker::GlobalMatchmaker,
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

        app.add_systems(Startup, start_network);
        app.add_systems(Update, drain_chat_events);
        app.add_plugins(multiplayer_plugin::MultiplayerPlugin);
    }
}

#[cfg(not(target_family = "wasm"))]
fn start_network(
    mut commands: Commands,
    rt: Res<NetworkRuntime>,
    proxy_wrapper: Res<EventLoopProxyWrapper>,
) {
    let secrets = UserIdentitySecrets::generate();
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
    rt.0.spawn(future);
}

#[cfg(target_family = "wasm")]
fn start_network(mut commands: Commands, proxy_wrapper: Res<EventLoopProxyWrapper>) {
    let secrets = UserIdentitySecrets::generate();
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
    _crack_utils::n0_future::task::spawn(future);
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
    let global_mm = match GlobalMatchmaker::new(Arc::new(secrets)).await {
        Ok(mm) => mm,
        Err(e) => {
            send_status(format!("Error: {:?}", e));
            return;
        }
    };

    send_status("Connecting to chat...".to_string());
    let controller = match global_mm.global_chat_controller().await {
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
    let _ = controller.wait_joined().await;

    // Join gameplay room
    let gameplay_ticket =
        ChatTicket::new_str_bs("global_gameplay", global_mm.bootstrap_nodes_set().await);
    let gameplay_controller = match global_mm
        .own_node()
        .await
        .unwrap()
        .join_chat::<GameplaySyncRoomType>(&gameplay_ticket)
        .await
    {
        Ok(c) => {
            let gameplay_c = c.clone();
            let incoming_tx_gp = incoming_tx.clone();
            let proxy_gp = proxy.clone();
            _crack_utils::n0_future::task::spawn(async move {
                let _ = gameplay_c.wait_joined().await;
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

    // Spawn tasks for gameplay sync if joined successfully
    if let Some(gameplay_c) = gameplay_controller {
        let gameplay_sender = gameplay_c.sender();
        let game_outgoing_rx_clone = game_outgoing_rx.clone();

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

        _crack_utils::n0_future::task::spawn(async move {
            while let Some(msg) = gameplay_recv.next_message().await {
                if *msg.from.node_id() == own_node_id {
                    continue; // Skip own loopback
                }

                let from_node_id = *msg.from.node_id();
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

    let _ = global_mm.shutdown().await;
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
