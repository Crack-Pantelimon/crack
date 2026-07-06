# Game sound effects: manifest attenuation + AudioFxEvent pipeline

## Context

All sound assets in `_data/sound_data/sound-fx2/` load and play correctly (verified via the audio demo). Now we wire sounds into real gameplay: give every clip a hand-picked **attenuation distance** in the manifest (1 = dies off quickly, 100 = distance divided by 100, heard from far like gunshots), plumb it through the manifest reader and playback path, and add an `AudioFxEventType` enum + observer that converts gameplay events (shots, melee, footsteps, engine, crashes, gear shifts, weapon draws) into concrete spatial audio playback. Also fixes the engine RPM/gear sim so the car actually reaches `car_max_speed` in top gear.

All paths below are relative to `crack_demo/demo_resolution_selector_web_bevy/src/` unless noted.

**Key discoveries from exploration:**
- `AudioDemoPlugin` (`plugins/audio/mod.rs`) is registered **only** in `src/bin/audio_demo.rs`, NOT in `MainGamePlugin` (`main_game_plugin.rs`). Core audio must be split out and registered in the main game.
- Melee has **no hit detection anywhere** — LMB melee just plays `Sword_Attack` (`animation.rs:257`). Meat/clash sounds require adding a minimal hit check.
- Bevy 0.19 verified APIs: `PlaybackSettings.spatial_scale: Option<SpatialScale>` (per-emitter, `SpatialScale(Vec3)`); `SpatialAudioSink: AudioSinkPlayback` with `set_speed/set_volume/pause/play`. Attenuation D → `spatial_scale = SpatialScale::new(Vec3::splat(1.0 / D))`.
- `setup_spatial_listener` runs at `Startup` querying `Camera3d` — main-game camera may spawn later; also handle `Added<Camera3d>`.

## 1. Manifest → CSV with attenuation

Edit `_data/sound_data/sound-fx2/manifest.txt` in place to `path,attenuation` lines (values hand-picked):

```
ambient/ambient-crickets.mp3,40
car-sounds/car-crash-1.mp3,40
car-sounds/car_crash_bump_2.mp3,15
car-sounds/car-crash-bump.mp3,15
car-sounds/car-crash-v2.mp3,40
car-sounds/car-engine-1.mp3,25
car-sounds/car-tire-deflating.mp3,10
car-sounds/car-tire-on-gravel.mp3,12
car-sounds/car-tire-screech.mp3,25
car-sounds/engine-acceleration.mp3,25
car-sounds/engine-idle-2.mp3,20
car-sounds/engine-idle-3.mp3,20
car-sounds/engine-truck-idle.mp3,25
car-sounds/engine-turbocharger-whoosh.mp3,15
misc-sounds/bike-tire-cards-wheel-spokes.mp3,5
misc-sounds/coin-sound.mp3,5
misc-sounds/deep-thud.mp3,20
pedestrian-sounds/barefoot_footsteps_on_gravel.mp3,4
weapons/guns/bullet-impact-1.mp3,15
weapons/guns/bullet-impact-2.mp3,15
weapons/guns/bullet-impact-3.mp3,15
weapons/guns/bullet-impact-ground.mp3,12
weapons/guns/get_weapon_from_holster.mp3,5
weapons/guns/gun_reload_clip.mp3,8
weapons/guns/gunshot-22lr-snap.mp3,80
weapons/guns/gunshot-50cal.mp3,150
weapons/guns/gunshot_echo.mp3,120
weapons/guns/gunshot-pistol1911.mp3,100
weapons/guns/gunshot-pistol-9mm.mp3,100
weapons/guns/gunshot-pistol-sharp.mp3,100
weapons/guns/reload_revolver.mp3,8
weapons/melee/punch-hit.mp3,10
weapons/melee/sword_clash.mp3,30
weapons/melee/sword-getout.mp3,6
weapons/melee/sword_hit_meat.mp3,10
weapons/melee/sword_whoosh.mp3,6
```

## 2. Manifest reader + playback path (`plugins/audio/mod.rs`)

- `SoundEntry` gains `pub attenuation: f32`.
- `load_sound_manifest_system`: parse each line as `path[,attenuation]` (split on `,`, trim, default `1.0` if missing/unparseable — stays backward compatible).
- `SoundManifest` gains `pub fn get(&self, name: &str) -> Option<&SoundEntry>` (linear scan by `name`).
- `PlaySoundEvent` gains `pub attenuation: f32` and `pub follow: Option<Entity>` (loop-emitter attach) and `pub looped: bool`.
- `play_sound_observer`: sets `spatial_scale: Some(SpatialScale::new(Vec3::splat(1.0 / ev.attenuation.max(0.001))))`; `mode: Loop` + spawn as child of `follow` entity when requested, else one-shot `Despawn` at `position`. Existing demo call sites pass `attenuation: 1.0, follow: None, looped: false`.

### Plugin split
- Rename/refactor into `GameAudioPlugin` (core: TextAsset loader init, `SoundManifestLoadFinished` state, manifest load, spatial listener incl. `Added<Camera3d>` system, `play_sound_observer`, new `audio_fx` observer/systems) and keep `AudioDemoPlugin` as demo-UI-only (sliders, clip list, click-to-play, gizmo) that the demo bin adds on top.
- `src/bin/audio_demo.rs`: add `GameAudioPlugin` + `AudioDemoPlugin`.
- `main_game_plugin.rs`: add `GameAudioPlugin`.
- Guard against double-init of `TextAsset` loader (already init'd by pedestrian manifest plugin in main game — check `is_plugin_added`/use `init_asset_loader` idempotency; verify at build time).

## 3. New `plugins/audio/audio_fx.rs` — AudioFxEventType + static tables + observer

```rust
#[derive(Clone, Copy, Debug)]
pub enum AudioFxEventType {
    GunShot { sound_idx: usize },   // index into GUNSHOT_SOUNDS, chosen at equip
    GunReload,
    BulletImpact,                   // random from BULLET_IMPACT_SOUNDS
    DrawGun,                        // get_weapon_from_holster
    DrawMelee,                      // sword-getout
    MeleeWhoosh,
    MeleeHitMeat,                   // sword_hit_meat / punch-hit for unarmed
    MeleeClash,
    PunchHit,
    CarCrash { rel_speed: f32 },    // observer picks bump vs crash-1/v2 by speed
    GearShiftWhoosh,
    FootstepLoop,                   // looped, attached
    EngineLoop { sound_idx: usize },// looped, attached; index into ENGINE_IDLE_SOUNDS
}

#[derive(Event, Clone, Copy, Debug)]
pub struct AudioFxEvent {
    pub fx: AudioFxEventType,
    pub position: Vec3,
    pub follow: Option<Entity>,     // for loops
}
```

Static tables (hard-coded manifest paths):
- `GUNSHOT_SOUNDS`: 22lr-snap, 50cal, echo, pistol1911, pistol-9mm, pistol-sharp (6 entries)
- `BULLET_IMPACT_SOUNDS`: bullet-impact-1/2/3/ground
- `ENGINE_IDLE_SOUNDS`: engine-idle-2, engine-idle-3, engine-truck-idle
- `CAR_BUMP_SOUNDS`: car-crash-bump, car_crash_bump_2; `CAR_CRASH_SOUNDS`: car-crash-1, car-crash-v2
- singles: gun_reload_clip, get_weapon_from_holster, sword-getout, sword_whoosh, sword_hit_meat, sword_clash, punch-hit, engine-turbocharger-whoosh, barefoot_footsteps_on_gravel

`audio_fx_observer(On<AudioFxEvent>, Res<SoundManifest>, Commands)`:
- maps `fx` → manifest path (random pick where a table applies, `_crack_utils::random_u32` for wasm-safe RNG — same rand usage as the rest of the crate, whichever is already used in `collision_sparks.rs`), looks up `SoundEntry` (handle + attenuation), sets volume/speed per type (small ±10% speed jitter on one-shots like gunshots/impacts; CarCrash volume scales with `rel_speed`), then `commands.trigger(PlaySoundEvent { .. })`.
- `CarCrash`: `rel_speed < 6.0` → random bump (volume ∝ speed), else random crash.
- If manifest not loaded yet, drop the event silently.

Register observer + the loop-controller systems below in `GameAudioPlugin`.

## 4. Gameplay hooks

### Gunshot per-weapon sound (`plugins/weapons/weapon_attach.rs`)
- `equip_weapon_observer` (weapon_attach.rs:66-83): when inserting `GunState`, also pick `sound_idx = random % GUNSHOT_SOUNDS.len()` and store it — add field `pub gunshot_sound_idx: usize` to `GunState` (weapon_shooting.rs:16-20).
- Same observer: trigger `AudioFxEvent { fx: DrawGun / DrawMelee, .. }` on gun/melee equip (nothing for Unarmed). Needs a `Query<&GlobalTransform>` for the character position — add it to the observer.

### Gun fire + bullet impact (`plugins/weapons/weapon_shooting.rs`)
- `fire_gun_observer`: after `gun.rounds -= 1`, trigger `AudioFxEvent { fx: GunShot { sound_idx: gun.gunshot_sound_idx }, position: muzzle }` (muzzle already computed at :117-127).
- On raycast hit (:130): trigger `AudioFxEvent { fx: BulletImpact, position: impact }`.

### Reload (`weapon_shooting.rs:195`)
- `reload_gun_observer`: add `Query<&GlobalTransform>`; trigger `GunReload` at shooter position (only when clip wasn't already full).

### Melee (`plugins/pedestrians/pedestrian_controller_plugin/animation.rs` + new hit check)
- LMB melee branch (animation.rs:257-258): trigger `MeleeWhoosh` at weapon position (via `WeaponModelState.entity` → `GlobalTransform`, fallback character pos). Unarmed punch branch (:259-263): same whoosh (lower volume).
- New minimal hit detection (new small system in `plugins/weapons/weapon_shooting.rs` or `melee.rs`): on melee/unarmed attack insert `PendingMeleeHit { timer: ~0.25s }` on the attacker; when the timer fires, `SpatialQuery` sphere/ray cast forward from the character (~1.5 m, excluding self); classify with the existing `is_person_entity` helper (weapon_shooting.rs:57-80): person → `MeleeHitMeat` (sword) / `PunchHit` (unarmed), non-person hit → `MeleeClash` (sword only), no hit → nothing.

### Footsteps (`plugins/pedestrians/pedestrian_controller_plugin/`)
- New system (in the pedestrian controller plugin or audio_fx.rs): for entities with `CharacterController`, ensure a looping footstep emitter child exists (spawned via `AudioFxEvent { fx: FootstepLoop, follow: Some(entity) }` once, tracked with a `FootstepEmitter(Entity)` component; child offset ~`-Y*0.8` for feet).
- Each frame: query the child's `SpatialAudioSink`; `play()` when `grounded && speed > MOVE_ANIM_THRESHOLD (0.25)`, `pause()` otherwise; `set_speed` by tier (walk ~0.9, jog ~1.3, sprint ~1.7) using `WALK_MAX_SPEED`/`JOG_MAX_SPEED` constants from `mod.rs:83-85`.

### Car engine loop (`plugins/cars_driving/`)
- On car spawn (`driving_plugin/spawn_car.rs`, where `Car` + `CarDriveState` are inserted): pick random `EngineLoop { sound_idx }`, trigger with `follow: Some(car)`; track emitter entity in a new `EngineSoundEmitter { emitter: Entity }` component (or spawn lazily from a system that sees `Added<Car>` — lazy system is safer since the manifest may not be loaded at spawn; retry until spawned).
- Per-frame system: map `engine_rpm` linearly from [800, 6500] → sink speed [0.33, 3.0] via `SpatialAudioSink::set_speed`; slight volume increase with throttle.

### Gear shift whoosh + crash sounds (`plugins/cars_driving/driving_plugin/`)
- New small system after `apply_car_steering_and_drive`: `Local<HashMap<Entity, usize>>` of last gear; on upshift trigger `GearShiftWhoosh` at car position.
- `collision_sparks.rs` `handle_car_collisions`: after the rate-limiter accepts an event (:134-139), trigger `AudioFxEvent { fx: CarCrash { rel_speed }, position: collision_point }` (skip below rel_speed ~1.5 to avoid scrape spam).

## 5. Fix engine RPM / gear ratios (`driving_plugin/mod.rs:452-489`)

Problem: hardcoded `gear_ratios [3.5,2.1,1.4,1.0,0.8]` + `final_drive 3.7` give ~2000 RPM at 140 km/h in 5th — below the 1800 downshift threshold at cruise and never near redline, so gears hunt and the car "never reaches max speed on the final gear" RPM-wise.

Fix: derive ratios from `car_max_speed` and `wheel_radius`:
- Per-gear top speeds as fractions of `car_max_speed`: `GEAR_SPEED_FRACS = [0.18, 0.32, 0.50, 0.72, 1.0]`.
- `SHIFT_UP_RPM = 5500`, `MAX_RPM = 6500`, target RPM at gear-top-speed = `SHIFT_UP_RPM` (so top gear reads ~5500 at `car_max_speed`, i.e. max speed is hit in final gear near redline).
- `gear_ratio(i) = SHIFT_UP_RPM * 2π * wheel_radius / (GEAR_SPEED_FRACS[i] * car_max_speed_mps * 60.0 * FINAL_DRIVE)` — computed in a small helper fn used by both the RPM calc and shifting; reverse gear = ratio of gear 1.
- Downshift threshold: keep 1800 RPM; with these fracs an upshift lands at `5500 * frac_i/frac_{i+1}` ≥ ~3000 RPM, so no hunting.
- Drive force model is unchanged (it doesn't use gears).

## Files touched (summary)

- `_data/sound_data/sound-fx2/manifest.txt` — CSV rewrite
- `plugins/audio/mod.rs` — attenuation parse, PlaySoundEvent ext, plugin split (`GameAudioPlugin` + demo UI)
- `plugins/audio/audio_fx.rs` — NEW: enum, static tables, observer, loop-emitter systems
- `main_game_plugin.rs`, `src/bin/audio_demo.rs` — plugin registration
- `plugins/weapons/weapon_shooting.rs` — fire/reload/impact hooks, `GunState.gunshot_sound_idx`, melee hit-check system
- `plugins/weapons/weapon_attach.rs` — equip: sound idx + draw sounds
- `plugins/pedestrians/pedestrian_controller_plugin/animation.rs` — melee/punch whoosh trigger + pending-hit insert
- `plugins/pedestrians/pedestrian_controller_plugin/` — footstep loop system
- `plugins/cars_driving/driving_plugin/mod.rs` — gear-ratio fix + gear-shift detection
- `plugins/cars_driving/driving_plugin/spawn_car.rs` (or lazy `Added<Car>` system) — engine loop spawn
- `plugins/cars_driving/driving_plugin/collision_sparks.rs` — crash sound trigger

## Verification

1. `cargo check` / `cargo clippy` on the crate (native target).
2. Run the audio demo bin (`cargo run --bin audio_demo`): manifest still loads (36 clips), clicking ground plays with per-clip attenuation — a gunshot audible from far, footsteps only close.
3. Run the main game (native or `trunk serve` per project setup): 
   - spawn pedestrian → walk (footstep loop starts/stops, speeds by tier), scroll weapon wheel (holster/sword-getout), fire gun (per-gun consistent shot sound + impact sound at hit), R reload, melee LMB (whoosh; hit ped = meat, hit wall/car = clash).
   - spawn + drive car: engine loop pitch rises with RPM (0.33×–3.0×), gear upshifts fire turbo whoosh, speedometer shows gears 1→5 reaching max speed in 5th near redline, crash into wall slow = bump, fast = crash sound.
4. Confirm no double `TextAsset` loader registration panic in the main game.
