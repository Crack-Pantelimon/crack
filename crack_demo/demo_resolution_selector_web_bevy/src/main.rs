use demo_resolution_selector_web_bevy::main_game_plugin::MainGamePlugin;

fn main() {
    let mut app = demo_resolution_selector_web_bevy::basic_app::make_basic_app("Pantelimon");
    app.add_plugins(MainGamePlugin);
    app.run();
}
