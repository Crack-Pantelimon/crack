//! Main binary: runs the full Pantelimon game with MainGamePlugin.

use demo_resolution_selector_web_bevy::main_game_plugin::MainGamePlugin;

fn main() {
    let mut app = demo_resolution_selector_web_bevy::basic_app::make_basic_app("Pantelimon");
    app.add_plugins(MainGamePlugin);
    app.run();
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_game_survives_ten_headless_frames() {
        let mut app = demo_resolution_selector_web_bevy::basic_app::make_headless_app("Pantelimon");
        app.add_plugins(demo_resolution_selector_web_bevy::main_game_plugin::MainGamePlugin);
        // `App::run()`'s default runner always calls these before the first
        // `update()` (see `bevy_app::app::run_once`); driving the app
        // manually in a test must do the same or plugins that defer setup to
        // `finish()`/`cleanup()` never get to run it.
        while app.plugins_state() == bevy::app::PluginsState::Adding {
            bevy::tasks::tick_global_task_pools_on_main_thread();
        }
        app.finish();
        app.cleanup();
        for _ in 0..10 {
            app.update();
        }
        let n = app
            .world_mut()
            .query::<&bevy::prelude::Camera>()
            .iter(app.world())
            .count();
        assert!(n >= 1, "expected >=1 camera, got {n}");
    }
}
