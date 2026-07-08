use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

/// Fire via commands.trigger(...) from anywhere. The plugin owns display.
#[derive(Event, Clone, Debug)]
pub enum NotificationEvent {
    MapLoaded,
    GeoJsonLoaded,
    NetworkConnected, // "network connected"
    GameNetworkOk,    // "game network ok"
    PlayerJoinedGame {
        nickname: String,
        color: (u8, u8, u8),
    },
    PlayerLeftGame {
        nickname: String,
        color: (u8, u8, u8),
    }, // cheap to add, symmetric
}

#[derive(Debug)]
pub struct ActiveNotification {
    pub text: String,
    pub stroke: egui::Color32, // border color, keeps the existing per-kind styling
    pub remaining: f32,        // seconds; default 3.0 like today
}

#[derive(Resource, Default)]
pub struct ActiveNotifications(pub Vec<ActiveNotification>);

pub struct TooltipNotificationPlugin;

impl Plugin for TooltipNotificationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveNotifications>()
            .add_observer(on_notification)
            .add_systems(Update, tick_notifications)
            .add_systems(EguiPrimaryContextPass, render_notifications);
    }
}

fn on_notification(trigger: On<NotificationEvent>, mut active: ResMut<ActiveNotifications>) {
    let (text, stroke) = match trigger.event() {
        NotificationEvent::MapLoaded => (
            "map loaded.".to_string(),
            egui::Color32::from_rgb(0, 180, 240),
        ),
        NotificationEvent::GeoJsonLoaded => (
            "geojson loaded.".to_string(),
            egui::Color32::from_rgb(0, 220, 80),
        ),
        NotificationEvent::NetworkConnected => (
            "network connected".to_string(),
            egui::Color32::from_rgb(0, 180, 240),
        ),
        NotificationEvent::GameNetworkOk => (
            "game network ok".to_string(),
            egui::Color32::from_rgb(0, 220, 80),
        ),
        NotificationEvent::PlayerJoinedGame { nickname, color } => (
            format!("player {} has joined game", nickname),
            egui::Color32::from_rgb(color.0, color.1, color.2),
        ),
        NotificationEvent::PlayerLeftGame { nickname, color } => (
            format!("player {} has left game", nickname),
            egui::Color32::from_rgb(color.0, color.1, color.2),
        ),
    };

    active.0.push(ActiveNotification {
        text,
        stroke,
        remaining: 3.0,
    });

    while active.0.len() > 6 {
        active.0.remove(0);
    }
}

fn tick_notifications(time: Res<Time>, mut active: ResMut<ActiveNotifications>) {
    let dt = time.delta_secs();
    for item in active.0.iter_mut() {
        item.remaining -= dt;
    }
    active.0.retain(|item| item.remaining > 0.0);
}

fn render_notifications(mut contexts: EguiContexts, active: Res<ActiveNotifications>) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if active.0.is_empty() {
        return;
    }

    let screen_rect = ctx.screen_rect();

    // Stack active entries vertically inside a bottom-left container.
    // Shift the Area's Y coordinate upwards based on number of active notifications
    // to prevent drawing off-screen.
    let base_y = screen_rect.max.y - 80.0;
    let offset_y = active.0.len().saturating_sub(1) as f32 * 38.0;
    let area_pos = egui::pos2(16.0, base_y - offset_y);

    egui::Area::new(egui::Id::new("loading_tooltips"))
        .fixed_pos(area_pos)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                for (i, notification) in active.0.iter().enumerate() {
                    if i > 0 {
                        ui.allocate_space(egui::Vec2::new(1.0, 4.0));
                    }
                    egui::Frame::window(ui.style())
                        .fill(egui::Color32::from_black_alpha(200))
                        .stroke(egui::Stroke::new(1.0, notification.stroke))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&notification.text)
                                    .color(egui::Color32::WHITE)
                                    .size(16.0)
                                    .strong(),
                            );
                        });
                }
            });
        });
}
