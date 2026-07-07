use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

use net_crackpipe::{
    chat::chat_controller::{IChatController, IChatReceiver, IChatSender},
    chat::global_chat::{GlobalChatMessageContent, GlobalChatPresence},
    global_matchmaker::GlobalMatchmaker,
    user_identity::UserIdentitySecrets,
};

fn main() {
    make_basic_app("Bevy Chat Client")
        .add_plugins(EguiPlugin::default())
        .add_plugins(SetupDebugScenePlugin)
        .add_systems(Startup, startup_chat_system)
        .add_systems(Update, (update_chat_system))
        .add_systems(EguiPrimaryContextPass,draw_chat_ui_system)
        .run();
}

#[derive(Resource)]
struct ChatState {
    own_nickname: String,
    own_color: (u8, u8, u8),
    presence_list: Vec<(String, (u8, u8, u8))>,
    msg_history: Vec<(String, String, (u8, u8, u8))>, // (nickname, text, rgb_color)
    status_message: String,
    input_buffer: String,
    outgoing_tx: Sender<String>,
    incoming_rx: Mutex<Receiver<ChatEvent>>,
}

enum ChatEvent {
    Message {
        nickname: String,
        text: String,
        color: (u8, u8, u8),
    },
    PresenceUpdate(Vec<(String, (u8, u8, u8))>),
    StatusUpdate(String),
}

fn startup_chat_system(mut commands: Commands) {
    let (incoming_tx, incoming_rx) = channel();
    let (outgoing_tx, outgoing_rx) = channel();

    // Spawn the background chat thread and get the generated username and color info
    let (own_nickname, own_color) = spawn_chat_thread(incoming_tx, outgoing_rx);

    commands.insert_resource(ChatState {
        own_nickname,
        own_color,
        presence_list: Vec::new(),
        msg_history: Vec::new(),
        status_message: "Initializing...".to_string(),
        input_buffer: String::new(),
        outgoing_tx,
        incoming_rx: Mutex::new(incoming_rx),
    });
}

fn spawn_chat_thread(
    incoming_tx: Sender<ChatEvent>,
    outgoing_rx: Receiver<String>,
) -> (String, (u8, u8, u8)) {
    // Generate identity secrets locally using the German words nickname gimmick
    let secrets = UserIdentitySecrets::generate();
    let user_id = secrets.user_identity();
    let own_nickname = user_id.nickname();
    let own_color = user_id.rgb_color();

    let own_nickname_clone = own_nickname.clone();
    let incoming_tx_msg = incoming_tx.clone();

    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let _ = incoming_tx.send(ChatEvent::StatusUpdate("Connecting to server...".to_string()));
            let global_mm = match GlobalMatchmaker::new(Arc::new(secrets)).await {
                Ok(mm) => mm,
                Err(e) => {
                    let _ = incoming_tx.send(ChatEvent::StatusUpdate(format!("Error: {:?}", e)));
                    return;
                }
            };

            let _ = incoming_tx.send(ChatEvent::StatusUpdate("Connecting to chat...".to_string()));
            let controller = match global_mm.global_chat_controller().await {
                Some(c) => c,
                None => {
                    let _ = incoming_tx.send(ChatEvent::StatusUpdate("Failed to get chat controller".to_string()));
                    return;
                }
            };

            let presence = controller.chat_presence();
            let sender = controller.sender();

            sender.set_presence(&GlobalChatPresence {
                url: "".to_string(),
                platform: "Bevy Egui Chat".to_string(),
                is_server: None,
            }).await;

            let _ = incoming_tx.send(ChatEvent::StatusUpdate("Waiting to join chat room...".to_string()));
            let _ = controller.wait_joined().await;

            let _ = incoming_tx.send(ChatEvent::StatusUpdate("Connected!".to_string()));

            // Send initial presence update
            let presence_list = presence.get_presence_list().await;
            let mut list = Vec::new();
            for item in presence_list.0 {
                list.push((item.identity.nickname().to_string(), item.identity.rgb_color()));
            }
            let _ = incoming_tx.send(ChatEvent::PresenceUpdate(list));

            // Start message receiver
            let recv = controller.receiver().await;

            let presence_clone = presence.clone();
            let incoming_tx_presence = incoming_tx.clone();

            // Spawn task to check presence updates
            tokio::spawn(async move {
                loop {
                    presence_clone.notified().await;
                    let presence_list = presence_clone.get_presence_list().await;
                    let mut list = Vec::new();
                    for item in presence_list.0 {
                        list.push((item.identity.nickname().to_string(), item.identity.rgb_color()));
                    }
                    if incoming_tx_presence.send(ChatEvent::PresenceUpdate(list)).is_err() {
                        break;
                    }
                }
            });

            // Spawn task to handle outgoing messages from Bevy
            let sender_clone = sender.clone();
            tokio::spawn(async move {
                while let Ok(text) = outgoing_rx.recv() {
                    let msg = GlobalChatMessageContent::TextMessage { text: text.clone() };
                    match sender_clone.broadcast_message(msg).await {
                        Ok(sent_preview) => {
                            let nickname = sent_preview.from.nickname().to_string();
                            let color = sent_preview.from.rgb_color();
                            let _ = incoming_tx_msg.send(ChatEvent::Message { nickname, text, color });
                        }
                        Err(e) => {
                            eprintln!("Error sending message: {:?}", e);
                        }
                    }
                }
            });

            // Loop to handle incoming messages
            loop {
                if let Some(msg) = recv.next_message().await {
                    let nickname = msg.from.nickname().to_string();
                    let color = msg.from.rgb_color();
                    match msg.message {
                        GlobalChatMessageContent::TextMessage { text } => {
                            if incoming_tx.send(ChatEvent::Message { nickname, text, color }).is_err() {
                                break;
                            }
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }

            let _ = global_mm.shutdown().await;
        });
    });

    (own_nickname_clone, own_color)
}

fn update_chat_system(mut state: ResMut<ChatState>) {
    let mut events = Vec::new();
    if let Ok(rx) = state.incoming_rx.lock() {
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
    }
    for event in events {
        match event {
            ChatEvent::Message { nickname, text, color } => {
                state.msg_history.push((nickname, text, color));
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

fn draw_chat_ui_system(
    mut contexts: EguiContexts,
    mut state: ResMut<ChatState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(egui::Color32::from_black_alpha(160)))
        .show(ctx, |ui| {
            let available_height = ui.available_height();
            let bottom_height = 40.0;
            let top_height = available_height - bottom_height - 30.0;

            ui.horizontal(|ui| {
                // Presence list panel on the left (200px wide)
                ui.allocate_ui_with_layout(
                    egui::vec2(200.0, top_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.heading("Active Users");
                        ui.separator();
                        
                        egui::ScrollArea::vertical()
                            .id_salt("presence_scroll")
                            .show(ui, |ui| {
                                if state.presence_list.is_empty() {
                                    ui.label("Searching...");
                                } else {
                                    for (nick, color) in &state.presence_list {
                                        let c = egui::Color32::from_rgb(color.0, color.1, color.2);
                                        ui.horizontal(|ui| {
                                            let (rect, _response) = ui.allocate_exact_size(
                                                egui::vec2(8.0, 8.0),
                                                egui::Sense::hover(),
                                            );
                                            ui.painter().circle_filled(rect.center(), 4.0, c);
                                            ui.colored_label(c, nick);
                                        });
                                    }
                                }
                            });
                    },
                );

                ui.separator();

                // Chat history panel on the right (takes remaining width)
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), top_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.heading("Global Chat Room");
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .id_salt("chat_scroll")
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for (nick, text, color) in &state.msg_history {
                                    ui.horizontal(|ui| {
                                        let c = egui::Color32::from_rgb(color.0, color.1, color.2);
                                        ui.colored_label(c, format!("{}:", nick));
                                        ui.label(text);
                                    });
                                }
                            });
                    },
                );
            });

            ui.separator();

            // Bottom bar: Username in bottom left, Chatbox in bottom right
            ui.horizontal(|ui| {
                // Bottom left: Username
                ui.allocate_ui_with_layout(
                    egui::vec2(200.0, bottom_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let c = egui::Color32::from_rgb(state.own_color.0, state.own_color.1, state.own_color.2);
                        ui.label("You:");
                        ui.colored_label(c, &state.own_nickname);
                    },
                );

                ui.separator();

                // Bottom right: Chatbox & Status
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), bottom_height),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        if state.status_message != "Connected!" {
                            ui.colored_label(egui::Color32::YELLOW, &state.status_message);
                            ui.add_space(10.0);
                        }

                        let text_edit = egui::TextEdit::singleline(&mut state.input_buffer)
                            .hint_text("Type a message and press Enter...")
                            .desired_width(ui.available_width() - 80.0);
                        
                        let response = ui.add(text_edit);
                        
                        let mut do_send = false;
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            do_send = true;
                            response.request_focus();
                        }

                        if ui.button("Send").clicked() {
                            do_send = true;
                        }

                        if do_send {
                            let text = state.input_buffer.trim().to_string();
                            if !text.is_empty() {
                                let _ = state.outgoing_tx.send(text);
                                state.input_buffer.clear();
                            }
                        }
                    },
                );
            });
        });
}
