# Plan: Audio, car-park, weapon, reload fixes + animation-driver refactor slice


---

## 1. Master volume affects live spatial sinks
1. **Volume slider does nothing.** Options > Sound > Volume writes `GlobalVolume`, but Bevy only samples `GlobalVolume` when a sink is *created*. The dominant sounds are long-lived looping spatial sinks (car engine, footsteps) created once at spawn, so they never pick up slider changes. The engine loop even overwrites its own sink volume every frame with no master factor.

**Root cause:** `GlobalVolume` is spawn-time only; looping sinks and the engine bypass it.

**Approach:** treat `UiState.master_volume` as the authoritative master and multiply it into every live sink each frame (or on change). The two long-lived loops already have per-frame management systems — thread the master factor through them, and add master scaling for footsteps.

- `plugins/audio/audio_fx.rs`:
  - `manage_car_engine_sound_pitch_volume` ([audio_fx.rs:202](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/audio_fx.rs#L202)) already sets sink volume every frame — multiply `throttle_vol` by master. Add `Res<UiState>` (or `Res<GlobalVolume>`) and use `throttle_vol * master`.
  - `manage_footsteps_system` ([audio_fx.rs:241](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/audio_fx.rs#L241)) — where it sets footstep sink volume, multiply by master.
- One-shot emitters ([audio/mod.rs:210](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/mod.rs#L210)) already read `GlobalVolume` at spawn, which the slider updates — new shots respect it. To make *in-flight* one-shots and any other loops also track the slider, add a small system that, when `master_volume` changes, iterates **all** `SpatialAudioSink`s and rescales. Simplest robust option: keep a `base_volume` on each managed emitter and set `volume = base * master`; for generic one-shots, relying on spawn-time `GlobalVolume` is acceptable since they're transient.
- Keep `GlobalVolume` in sync (already done at [ui_egui.rs:272](crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs#L272)) so newly-spawned sinks start correct.

**Decision to make during impl:** single source of truth — read `master_volume` from `Res<GlobalVolume>` (already synced) inside the audio systems, so audio code doesn't depend on `UiState`.

- Audio: `plugins/audio/audio_fx.rs`, `plugins/audio/mod.rs`, `ui_egui.rs`

Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`) - may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 







## 2. Fix car-park sleep/hover jitter

- Car park: `plugins/cars_driving/driving_plugin/mod.rs`

2. **"Car park" jitter.** When a car settles, the park logic sleeps the physics body and stops the raycast-hover controller; any nudge wakes it, hover re-lifts it to ride height, it re-settles and re-sleeps — the visible fall/get-up cycle. (User chose to fix the existing sleep/hover interaction, **not** add support cubes.)
**File:** `plugins/cars_driving/driving_plugin/mod.rs`, `apply_car_steering_and_drive` ([mod.rs:576-592](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L576-L592)) and the parked early-out ([mod.rs:483-492](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L483-L492)).

**Root cause:** entering park does `remove::<SleepingDisabled>()` + `SleepBody`, then the parked branch `continue`s and skips the hover controller. Asleep + no hover = the car rests on/sinks into its collider; a nudge wakes it and hover snaps it up → oscillation.

**Fix:** stop sleeping the body for parking. Instead, while parked, **keep running the hover controller but zero out drive/lateral forces**, and hard-freeze residual velocity so it stays put:
- In the enter-park block, do **not** `remove::<SleepingDisabled>()` / `SleepBody`. Set `parked = true`, zero `lin_vel`/`ang_vel`.
- In the parked branch ([mod.rs:483](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/mod.rs#L483)), don't blanket-`continue`. Still compute wheel heights + apply the vertical ride-height hover (so it holds at `suspension_rest`), but skip throttle/steering and clamp planar + angular velocity to ~0. Unpark on input or speed as today.
- Keep the `unpark_car` path intact for the wake case; since we no longer sleep, `WakeBody`/`SleepingDisabled` toggling can be simplified (the car is always `SleepingDisabled`).

Net: parked car is held at ride height continuously (no drop), no repeated wake/lift. Verify it doesn't drift on slopes (the velocity clamp handles this).

Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`) - may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 




## 3. Weapon-wheel debounce (0.15s)
- Weapon wheel: `plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs`

3. **Weapon wheel too twitchy** — no cross-frame debounce.

**File:** `plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs`, `weapon_wheel` ([interaction_ui.rs:794](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L794)).

- Add `time: Res<Time>` and `mut next_switch: Local<f32>` (mirror the `Local<f32>` + `time.elapsed_secs()` pattern at [traffic/spawn.rs:48](crack_demo/demo_resolution_selector_web_bevy/src/plugins/traffic/spawn.rs#L48)).
- After collapsing wheel input to a `step` (existing logic already dedups multi-notch per frame), if `time.elapsed_secs() < *next_switch`, `return`. On an accepted switch set `*next_switch = time.elapsed_secs() + 0.15`.






## 4. Blocking, interruptible, duration-matched reload
4. **Reload is instant & non-blocking** — clip refills the same frame, firing isn't blocked, reload can't be interrupted, and the reload animation plays at a fixed 1× regardless of intended duration.

- Reload: `plugins/weapons/weapon_shooting.rs`, `plugins/weapons/weapon_manifest.rs`, `game_logic/src/weapon.rs`, `game_logic/src/worker/weapon_impl.rs`, `plugins/pedestrians/pedestrian_controller_plugin/animation.rs`

**Manifest — hand-picked reload durations:**
- Add `reload_secs: f32` to `WeaponEntry` ([game_logic/src/weapon.rs:4](crack_demo/game_logic/src/weapon.rs#L4)) and `GunInfo` ([weapon_manifest.rs:6](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_manifest.rs#L6)).
- The manifest is a remote CSV (`3d_weapons/out2/manifest.txt`) parsed in `fetch_weapon_manifest` ([weapon_impl.rs:36](crack_demo/game_logic/src/worker/weapon_impl.rs#L36)). Add an optional 9th CSV column `reload_secs` with a sensible default (e.g. `2.0`) so it works without touching the remote file. Provide the "hand-picked values" as a per-weapon lookup keyed on the weapon file name/path (a small `match` in the client when building `GunInfo`), overriding the default. Melee weapons → `1.0`.


Run `cargo build` (and `cargo clippy`) for the demo crate + `game_logic` to catch the `WeaponEntry`/`GunInfo` field additions.


**Blocking + duration (state):** `plugins/weapons/weapon_shooting.rs`
- Add a reload timer to `GunState` ([weapon_shooting.rs:15](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L15)): `reload_timer: f32` (0 = not reloading).
- `reload_gun_observer` ([weapon_shooting.rs:288](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L288)): instead of instant refill, set `reload_timer = reload_secs` (skip if already reloading or clip full).
- Add a `tick_reload` system: decrement `reload_timer` by `time.delta_secs()`; when it crosses 0, refill `rounds = clip_size` and play the reload-complete SFX (move the existing `GunReload` audio trigger here or keep at start).
- `fire_gun_observer` ([weapon_shooting.rs:124](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_shooting.rs#L124)): bail if `reload_timer > 0`.

**Animation gating + speed match:** `plugins/pedestrians/pedestrian_controller_plugin/animation.rs`
- In `drive_character_animation`, gate the fire branch ([animation.rs:318](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L318)) on `gun_state.reload_timer <= 0.0` so you can't shoot mid-reload.
- Reload branch ([animation.rs:378](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L378)): set the one-shot speed to `natural_reload_len / reload_secs` so the clip stretches/compresses to the intended duration (mirror the melee `NATURAL_SWING_SECS / swing_secs` speed logic at [animation.rs:336](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L336)). Get the natural clip length from `AnimationInfo`/clip duration if available, else a `NATURAL_RELOAD_SECS` constant.
- **Interrupt** reloading when weapon-switching / climbing / sprinting / rolling: in scope of `drive_character_animation` the query already exposes `Has<Climbing>`, `Has<Rolling>`, `&MovementModifiers` (sprint). When any becomes active while `reload_timer > 0`, set `reload_timer = 0` **without** refilling (cancel). Weapon switch: cancel in the `EquipWeaponEvent` path ([interaction_ui.rs:787](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L787), [interaction_ui.rs:829](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L829)) or in `equip_weapon_observer`.
- AI path already has its own `AiCombatTimers.reload_timer` / `RELOAD_TIME` — leave AI unchanged (it triggers `ReloadGunEvent`; with the new observer it becomes duration-based automatically, which is fine).

Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`) - may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 



## 5. Minimal refactor slice: shared animation selection + explicit network driver

5. **Driver/entity coupling** — animation speed→clip selection is copy-pasted 3× (player/AI/network) with drifting thresholds; remote network avatars carry dead `LocomotionInput`. User approved a **minimal refactor slice**: unify the animation-selection logic + formalize network as an explicit driver.

Goal: remove the 3× duplication of speed→clip logic and stop pretending remote avatars are locomotion-driven. **No behavior change intended.**

- **Extract one speed→clip helper** into `plugins/pedestrians/animation.rs` (the shared, event-driven anim module). Signature ~ `fn locomotion_clip(speed: f32, crouch: bool, sprint: bool) -> &'static [&'static str]`, encoding the canonical thresholds (`MOVE_ANIM_THRESHOLD`, `WALK_MAX_SPEED`, `JOG_MAX_SPEED` from [pedestrian_controller_plugin/mod.rs:86-88](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L86)). Make those constants the single source.
  - Player `drive_character_animation` base-candidate match ([animation.rs:265-286](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/animation.rs#L265-L286)) calls it for the grounded/moving case.
  - AI `anim_ai.rs:53-75` (drops its re-declared duplicate thresholds `anim_ai.rs:16-18`) calls it.
  - Network `multiplayer_plugin.rs:1084-1112` (drops hard-coded `4.5/1.5/0.1`) calls it.
- **Formalize the network driver:** add a `NetworkDriven` marker component on remote avatar roots. Stop spawning the dead `LocomotionInput` on remote OnFoot avatars ([multiplayer_plugin.rs:645-687](crack_demo/demo_resolution_selector_web_bevy/src/plugins/network/multiplayer_plugin.rs#L645)); they move purely by transform lerp (`interpolate_remote_avatars`), which is correct. Keep only the components the shared anim helper actually reads. This documents the "kinematic replay" driver as distinct from player/AI locomotion drivers.

Explicitly **out of scope** here (deferred): unifying `LocomotionInput`/`Drive` into one driver interface, the single spawn bundle for network (`character_physics_bundle`), and the shared traffic path-follow FSM.

- Refactor slice: `plugins/pedestrians/animation.rs`, `plugins/pedestrian_ai/anim_ai.rs`, `plugins/network/multiplayer_plugin.rs`

Build/run the native app (`cargo check` in `crack_demo/demo_resolution_selector_web_bevy`) - may need to unset ARGV0 env on that cargo command because we're running in cursor appimage 




---

## Critical files



## Verification

Build/run the native app (`cargo run` in `crack_demo/demo_resolution_selector_web_bevy`, or the project `/run` skill) and check each fix in-game:
1. **Audio:** drive a car (engine loop) and walk (footsteps); move the Options > Sound slider — volume of both loops must change live; 0% = silent, 100% = loud.
2. **Car park:** drive, stop, wait for it to settle — car holds ride height steadily, no bob/drop/oscillation; nudging it or driving un-parks smoothly.
3. **Weapon wheel:** spin the wheel fast — switches step at most ~every 0.15s, no rapid cycling.
4. **Reload:** press R — cannot fire until reload finishes; the animation visibly matches the configured duration (test a slow vs fast gun); switching weapon / sprinting / climbing / rolling mid-reload cancels it (clip stops, clip stays un-refilled). Empty a gun and confirm firing is blocked during reload.
5. **Refactor slice:** confirm player, an AI/traffic pedestrian, and a second networked client all still animate walk/jog/sprint correctly (no regressions from the shared helper); remote avatars still move.
