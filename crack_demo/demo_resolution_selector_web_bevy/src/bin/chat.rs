use bevy_egui::EguiPlugin;
use demo_resolution_selector_web_bevy::{
    basic_app::make_basic_app,
    plugins::network::{NetworkPlugin, global_chat_ui::GlobalChatPlugin},
    utils::setup_debug_scene::SetupDebugScenePlugin,
};

fn main() {
    make_basic_app("Bevy Chat Client")
        .add_plugins(EguiPlugin::default())
        .add_plugins(SetupDebugScenePlugin)
        .add_plugins(NetworkPlugin)
        .add_plugins(GlobalChatPlugin { always_visible: true })
        .run();
}
