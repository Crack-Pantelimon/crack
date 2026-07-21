use crate::plugins::network::{ChatBubbles, ChatState, NetworkConnectionState};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// global chat ui state.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct GlobalChatUiState {
/// show window field.
    pub show_window: bool,
/// always visible field.
    pub always_visible: bool,
}

impl Default for GlobalChatUiState {
    fn default() -> Self {
        Self {
            show_window: false,
            always_visible: false,
        }
    }
}

/// global chat plugin.
pub struct GlobalChatPlugin {
/// always visible field.
    pub always_visible: bool,
}

impl Default for GlobalChatPlugin {
    fn default() -> Self {
        Self {
            always_visible: false,
        }
    }
}

impl Plugin for GlobalChatPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalChatUiState {
            show_window: false,
            always_visible: self.always_visible,
        });
        app.add_systems(bevy_egui::EguiPrimaryContextPass, draw_chat_ui_system);
    }
}

fn draw_chat_ui_system(
    mut contexts: EguiContexts,
    mut chat_ui_state: ResMut<GlobalChatUiState>,
    chat_state: Option<ResMut<ChatState>>,
    network_state: Res<State<NetworkConnectionState>>,
    mut bubbles: Option<ResMut<ChatBubbles>>,
    time: Res<Time>,
) {
    if !chat_ui_state.always_visible && !chat_ui_state.show_window {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let Some(mut state) = chat_state else {
        return;
    };

    // The window is being shown this frame, so the user is looking at the chat:
    // clear the unread notification badge.
    state.unread_count = 0;

    let mut show_window = chat_ui_state.show_window;

    let mut window = egui::Window::new("Global Chat")
        .default_width(520.0)
        .default_height(380.0)
        .resizable(true);

    if !chat_ui_state.always_visible {
        window = window.open(&mut show_window);
    }

    window.show(ctx, |ui| {
        match network_state.get() {
            NetworkConnectionState::Connecting => {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.heading(egui::RichText::new("Loading").size(32.0));
                    ui.add_space(20.0);
                    ui.colored_label(egui::Color32::YELLOW, &state.status_message);
                    ui.add_space(50.0);
                });
            }
            NetworkConnectionState::Connected => {
                let available_height = ui.available_height();
                let bottom_height = 40.0;
                let top_height = available_height - bottom_height - 15.0;

                ui.horizontal(|ui| {
                    // Presence list panel on the left (150px wide)
                    ui.allocate_ui_with_layout(
                        egui::vec2(150.0, top_height),
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
                                            let c =
                                                egui::Color32::from_rgb(color.0, color.1, color.2);
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
                                            let c =
                                                egui::Color32::from_rgb(color.0, color.1, color.2);
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
                        egui::vec2(150.0, bottom_height),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let c = egui::Color32::from_rgb(
                                state.own_color.0,
                                state.own_color.1,
                                state.own_color.2,
                            );
                            ui.label("You:");
                            ui.colored_label(c, &state.own_nickname);
                        },
                    );

                    ui.separator();

                    // Bottom right: Chatbox
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), bottom_height),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let text_edit = egui::TextEdit::singleline(&mut state.input_buffer)
                                .hint_text("Type a message and press Enter...")
                                .desired_width(ui.available_width() - 80.0);

                            let response = ui.add(text_edit);

                            let mut do_send = false;
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                do_send = true;
                                response.request_focus();
                            }

                            if ui.button("Send").clicked() {
                                do_send = true;
                            }

                            if do_send {
                                let text = state.input_buffer.trim().to_string();
                                if !text.is_empty() {
                                    let _ = state.outgoing_tx.try_send(text.clone());
                                    if let Some(ref mut bubbles) = bubbles {
                                        let is_longer = text.chars().count() > 70;
                                        let mut bubble_text: String =
                                            text.chars().take(70).collect();
                                        if is_longer {
                                            bubble_text.push('…');
                                        }
                                        bubbles.own =
                                            Some((bubble_text, time.elapsed_secs_f64() + 3.0));
                                    }
                                    state.input_buffer.clear();
                                }
                            }
                        },
                    );
                });
            }
        }
    });

    if !chat_ui_state.always_visible {
        chat_ui_state.show_window = show_window;
    }
}
