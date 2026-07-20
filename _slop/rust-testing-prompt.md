we have a lot of crates in rust_pkg and all these require some extra testing and documentation. Review all the creates under rust_pkg and each one should have a readme and agents md that describes what it is, how to use it, in a couple paragraphs max. In agents, there will be some auto-generated content, make sure to edit the text above that. Keep any pre-existing details and add more past trouble we've had with that crate in the past (look at the memory). The files should also contain any "gotcha"s or patterns that are present in the code, as well as instructions to run the tests (give the path to the test.sh script, see below).

Then, each of these creates will need some tests. Each crate should have its own "test.sh" executable bash script that will do "cd" into its parent dir (so we always test from the same location) and then test with both wasm target (using wasm-pack, it's in the container) and with normal `cargo test` (will run on x86 64 architecture in our specific case). Each crate should have at least one smoke test per module, but feel free to add 2-3 extra tests per module to ensure its complete lifecycle of the specific crate. 

Then, we can attempt the same in the rust crates that are present under crack_demo - these crates depend on the rust_pkg crates, so they should be informative of the actual usage of the crates themselves. The game_logic crate holds logic implementations - this one will need some tests also run on wasm+native. The worker crates for wasm and native threads can be instantiated and checked with a "ping" api call in a headless test, just to check they build and run. These can only be tested on their respective platform, through.

Then, we have the main bevy game. We can test that by adding new infrastructure functions for headless bevy apps that we control them manually step by step. What we want is to have a new function "make_headless_app" that will spawn an app with the default plugin very differently set up to allow headless testing (so no UI, no GUI, and step by step control of the simulation). Then, next to each binary main function (and next to the main game main function) we have tests that make the default app, add the same plugins as the main function we're testing (maybe with some different configurations for headless: true to avoid depending on any interface stuff, but if really needed, most systems and plugins shouldn't require this). Once we have this very basic smoke test passing for all the crates, the main game crate, we can continue with further testing. 

Finally, make a root level "test.sh" script that goes through every single test.sh file created above, one by one, and runs them, in order, stopping at first failure, printing the failure on screen and exiting in that case (or saying everything is OK and returning in the second case). It will also be a bash script that runs cd to its file's parent dir (which is the root of the repo) and then just uses ./<dir>/test.sh lines for all tests we wrote.

I added some example code where you can see how to test using wasm32 platform (reference: _slop/examples/Sparganothis-v2/test.sh)
 
    RUSTFLAGS='--cfg getrandom_backend="wasm_js"' wasm-pack test --node game  --features getrandom/wasm_js
    cargo test

As you can see, the wasm test (using wasm-pack) may require some extra configs and arguments for random library configuration. We should attempt to make this as automatic as possible, for example gating cfg blocks by platform under the crate's .cargo/config.toml and setting up the features using platform switches in the crate's Cargo.toml, if possible. If it's not possible, we can keep these unchanged, as they will be saved in each test.sh script to be used as-is. Additionally, each inner test script (the ones that run wasm-pack test and/or cargo test) should take the cargo description. 

For the main bevy apps, here's a known configuration of tests that is known to work in headless mode, along with running a number of frames of physics simulation.

        https://github.com/johnny-smitherson/spacejwz/blob/master/game/src/game_plugin.rs

        #[test]
        fn game_plugin_survives_hundred_frames_of_random_input() {
            let mut app = App::new();
            app.insert_resource(StressRng(ChaCha8Rng::seed_from_u64(0xFEED_FACE_D00D)));
            app.add_plugins((
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window: None,
                        exit_condition: ExitCondition::DontExit,
                        ..default()
                    })
                    .set(RenderPlugin {
                        render_creation: WgpuSettings {
                            backends: None,
                            ..default()
                        }
                        .into(),
                        ..default()
                    })
                    .build()
                    .disable::<WinitPlugin>()
                    .disable::<TransformPlugin>(),
                BigSpaceDefaultPlugins,
                GamePlugin,
            ));
            app.add_systems(
                PreUpdate,
                inject_random_keyboard.before(crate::input::capture_input),
            );

            for _ in 0..100 {
                app.update();
            }

            let mut q = app.world_mut().query_filtered::<Entity, With<PlayerShip>>();
            assert_eq!(q.iter(app.world()).count(), 1);
        }

As you can see, we can spawn a differnt config of the default plugin, and then put all of our rest of our plugins - so we should be off with just having a second function make_headless_app() to replace the make_app() function that is used normally. 

You can look at more test examples in:
- how to test apps _slop/examples/bevy/how_to_test_apps.rs
- how to test systems: _slop/examples/bevy/how_to_test_systems.rs

In your investigation and implementation of these features, use only bash commands that start with "docker exec crackd-dev ..." - all tooling (cargo, rust, wasm-pack, etc.) is only available inside the dev container. Tests are to be run only inside this dev container. The dev container's target directory is bound to a separate vollume, so only look at the target dir inside the container by using commands like 'rg', 'fzf' etc to figure out what works and what doesn't. 

Do not run any commands like cargo, python, bash, etc. without running that in the container using docker exec. 