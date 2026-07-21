use crate::plugins::audio::{PlaySoundEvent, SoundManifest, master_volume_linear};
use crate::plugins::cars_driving::driving_plugin::CarDriveState;
use crate::plugins::pedestrians::pedestrian_controller_plugin::{
    CharacterController, Grounded, MOVE_ANIM_THRESHOLD, footstep_playback_speed,
};
use avian3d::prelude::LinearVelocity;
use bevy::audio::GlobalVolume;
use bevy::audio::SpatialAudioSink;
use bevy::prelude::*;

/// gunshot sounds constant.
pub const GUNSHOT_SOUNDS: &[&str] = &[
    "weapons/guns/gunshot-22lr-snap.mp3",
    "weapons/guns/gunshot-50cal.mp3",
    "weapons/guns/gunshot_echo.mp3",
    "weapons/guns/gunshot-pistol1911.mp3",
    "weapons/guns/gunshot-pistol-9mm.mp3",
    "weapons/guns/gunshot-pistol-sharp.mp3",
];

/// bullet impact sounds constant.
pub const BULLET_IMPACT_SOUNDS: &[&str] = &[
    "weapons/guns/bullet-impact-1.mp3",
    "weapons/guns/bullet-impact-2.mp3",
    "weapons/guns/bullet-impact-3.mp3",
    "weapons/guns/bullet-impact-ground.mp3",
];

/// engine idle sounds constant.
pub const ENGINE_IDLE_SOUNDS: &[&str] = &[
    "car-sounds/engine-idle-2.mp3",
    "car-sounds/engine-idle-3.mp3",
    "car-sounds/engine-truck-idle.mp3",
];

/// car bump sounds constant.
pub const CAR_BUMP_SOUNDS: &[&str] = &[
    "car-sounds/car-crash-bump.mp3",
    "car-sounds/car_crash_bump_2.mp3",
];

/// car crash sounds constant.
pub const CAR_CRASH_SOUNDS: &[&str] =
    &["car-sounds/car-crash-1.mp3", "car-sounds/car-crash-v2.mp3"];

/// footstep sound constant.
pub const FOOTSTEP_SOUND: &str = "pedestrian-sounds/barefoot_footsteps_on_gravel.mp3";

/// audio fx event type.
#[derive(Clone, Copy, Debug)]
pub enum AudioFxEventType {
    /// Gunshot using the sound index chosen at weapon equip time.
    GunShot { sound_idx: usize },
/// gun reload variant.
    GunReload,
/// empty click variant.
    EmptyClick,
/// bullet impact variant.
    BulletImpact, // random from BULLET_IMPACT_SOUNDS
/// draw gun variant.
    DrawGun,      // get_weapon_from_holster
/// draw melee variant.
    DrawMelee,    // sword-getout
    /// Melee swing whoosh at the given linear volume.
    MeleeWhoosh { volume: f32 },
/// melee hit meat variant.
    MeleeHitMeat, // sword_hit_meat
/// melee clash variant.
    MeleeClash,
/// punch hit variant.
    PunchHit,
    /// Vehicle impact scaled by relative collision speed.
    CarCrash { rel_speed: f32 },
/// gear shift whoosh variant.
    GearShiftWhoosh,
/// footstep loop variant.
    FootstepLoop,                    // looped, attached
    /// Looping engine idle clip index from ENGINE_IDLE_SOUNDS.
    EngineLoop { sound_idx: usize },
/// climb variant.
    Climb,
/// death thud variant.
    DeathThud, // played when a pedestrian dies
}

/// audio fx event.
#[derive(Event, Clone, Copy, Debug)]
pub struct AudioFxEvent {
/// fx field.
    pub fx: AudioFxEventType,
/// position field.
    pub position: Vec3,
/// follow field.
    pub follow: Option<Entity>, // for loops
}

/// audio fx observer.
pub fn audio_fx_observer(
    trigger: On<AudioFxEvent>,
    manifest: Res<SoundManifest>,
    mut commands: Commands,
) {
    if !manifest.loaded {
        return;
    }
    let ev = trigger.event();

    let (path, volume, speed, looped) = match ev.fx {
        AudioFxEventType::GunShot { sound_idx } => {
            let idx = sound_idx % GUNSHOT_SOUNDS.len();
            let path = GUNSHOT_SOUNDS[idx];
            // small +/- 10% speed jitter
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            (path, 1.0, 1.0 + jitter, false)
        }
        AudioFxEventType::GunReload => {
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            ("weapons/guns/gun_reload_clip.mp3", 1.0, 1.0 + jitter, false)
        }
        AudioFxEventType::EmptyClick => {
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            ("weapons/guns/gun_dry_fire.mp3", 0.9, 1.0 + jitter, false)
        }
        AudioFxEventType::BulletImpact => {
            let idx = (_crack_utils::random_u32() as usize) % BULLET_IMPACT_SOUNDS.len();
            let path = BULLET_IMPACT_SOUNDS[idx];
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            (path, 1.0, 1.0 + jitter, false)
        }
        AudioFxEventType::DrawGun => ("weapons/guns/get_weapon_from_holster.mp3", 1.0, 1.0, false),
        AudioFxEventType::DrawMelee => ("weapons/melee/sword-getout.mp3", 1.0, 1.0, false),
        AudioFxEventType::MeleeWhoosh { volume } => {
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            (
                "weapons/melee/sword_whoosh.mp3",
                volume,
                1.0 + jitter,
                false,
            )
        }
        AudioFxEventType::MeleeHitMeat => {
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            ("weapons/melee/sword_hit_meat.mp3", 1.0, 1.0 + jitter, false)
        }
        AudioFxEventType::MeleeClash => {
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            ("weapons/melee/sword_clash.mp3", 1.0, 1.0 + jitter, false)
        }
        AudioFxEventType::PunchHit => {
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            ("weapons/melee/punch-hit.mp3", 1.0, 1.0 + jitter, false)
        }
        AudioFxEventType::CarCrash { rel_speed } => {
            let jitter = ((_crack_utils::random_u32() % 1000) as f32 / 1000.0) * 0.2 - 0.1;
            let (path, vol) = if rel_speed < 6.0 {
                let idx = (_crack_utils::random_u32() as usize) % CAR_BUMP_SOUNDS.len();
                let vol = (rel_speed / 6.0).clamp(0.2, 1.0);
                (CAR_BUMP_SOUNDS[idx], vol)
            } else {
                let idx = (_crack_utils::random_u32() as usize) % CAR_CRASH_SOUNDS.len();
                let vol = (rel_speed / 12.0).clamp(0.5, 2.0);
                (CAR_CRASH_SOUNDS[idx], vol)
            };
            (path, vol, 1.0 + jitter, false)
        }
        AudioFxEventType::GearShiftWhoosh => {
            ("car-sounds/engine-turbocharger-whoosh.mp3", 0.8, 1.0, false)
        }
        AudioFxEventType::FootstepLoop => (FOOTSTEP_SOUND, 0.6, 1.0, true),
        AudioFxEventType::EngineLoop { sound_idx } => {
            let idx = sound_idx % ENGINE_IDLE_SOUNDS.len();
            (ENGINE_IDLE_SOUNDS[idx], 1.0, 1.0, true)
        }
        AudioFxEventType::Climb => ("weapons/melee/sword_whoosh.mp3", 0.8, 0.7, false),
        AudioFxEventType::DeathThud => ("misc-sounds/deep-thud.mp3", 1.0, 1.0, false),
    };

    if let Some(entry) = manifest.get(path) {
        commands.trigger(PlaySoundEvent {
            handle: entry.handle.clone(),
            position: ev.position,
            volume: volume * entry.volume,
            speed,
            attenuation: entry.attenuation,
            follow: ev.follow,
            looped,
        });
    }
}

/// engine sound emitter.
#[derive(Component)]
pub struct EngineSoundEmitter {
/// emitter field.
    pub emitter: Entity,
}

/// spawn car engine sounds.
pub fn spawn_car_engine_sounds(
    mut commands: Commands,
    query: Query<
        Entity,
        (
            With<crate::plugins::cars_driving::driving_plugin::spawn_car::Car>,
            Without<EngineSoundEmitter>,
        ),
    >,
    manifest: Res<SoundManifest>,
) {
    if !manifest.loaded {
        return;
    }
    for car_entity in &query {
        let sound_idx = (_crack_utils::random_u32() as usize) % ENGINE_IDLE_SOUNDS.len();

        let emitter = commands
            .spawn((
                Name::new("CarEngineEmitter"),
                Transform::IDENTITY,
                Visibility::default(),
                InheritedVisibility::default(),
            ))
            .id();
        commands.entity(car_entity).add_child(emitter);

        commands.trigger(AudioFxEvent {
            fx: AudioFxEventType::EngineLoop { sound_idx },
            position: Vec3::ZERO,
            follow: Some(emitter),
        });

        commands
            .entity(car_entity)
            .insert(EngineSoundEmitter { emitter });
    }
}

/// manage car engine sound pitch volume.
pub fn manage_car_engine_sound_pitch_volume(
    query: Query<(&CarDriveState, &EngineSoundEmitter)>,
    mut sinks: Query<&mut SpatialAudioSink>,
    children_query: Query<&Children>,
    manifest: Res<SoundManifest>,
    global_volume: Res<GlobalVolume>,
) {
    let master = master_volume_linear(&global_volume);
    for (drive_state, emitter) in &query {
        let mut target_child = None;
        if let Ok(children) = children_query.get(emitter.emitter) {
            for child in children.iter() {
                if sinks.get(child).is_ok() {
                    target_child = Some(child);
                    break;
                }
            }
        }

        if let Some(child) = target_child {
            if let Ok(mut sink) = sinks.get_mut(child) {
                let rpm_pct = ((drive_state.engine_rpm - 800.0) / (6500.0 - 800.0)).clamp(0.0, 1.0);
                let playback_speed = 0.33 + rpm_pct * (3.0 - 0.33);
                sink.set_speed(playback_speed);

                let base_vol = manifest
                    .get("car-sounds/engine-idle-2.mp3")
                    .map(|e| e.volume)
                    .unwrap_or(0.6);
                let throttle_vol = (1.0 + drive_state.avg_accelerate * 0.5) * base_vol;
                sink.set_volume(bevy::audio::Volume::Linear(throttle_vol * master));
            }
        }
    }
}

/// footstep emitter.
#[derive(Component)]
pub struct FootstepEmitter {
/// emitter field.
    pub emitter: Entity,
}

/// manage footsteps system.
pub fn manage_footsteps_system(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &LinearVelocity,
            Has<Grounded>,
            Option<&FootstepEmitter>,
        ),
        With<CharacterController>,
    >,
    mut sinks: Query<&mut SpatialAudioSink>,
    children_query: Query<&Children>,
    manifest: Res<SoundManifest>,
    global_volume: Res<GlobalVolume>,
) {
    let master = master_volume_linear(&global_volume);
    let footstep_base_vol = manifest
        .get(FOOTSTEP_SOUND)
        .map(|e| 0.6 * e.volume)
        .unwrap_or(0.6);
    for (char_entity, velocity, grounded, emitter_opt) in &query {
        let emitter_entity = if let Some(emitter) = emitter_opt {
            emitter.emitter
        } else {
            let child_entity = commands
                .spawn((
                    Name::new("FootstepEmitter"),
                    Transform::from_translation(Vec3::new(0.0, -0.8, 0.0)),
                    Visibility::default(),
                    InheritedVisibility::default(),
                ))
                .id();
            commands.entity(char_entity).add_child(child_entity);

            commands.trigger(AudioFxEvent {
                fx: AudioFxEventType::FootstepLoop,
                position: Vec3::ZERO,
                follow: Some(child_entity),
            });

            commands.entity(char_entity).insert(FootstepEmitter {
                emitter: child_entity,
            });
            child_entity
        };

        let mut target_child = None;
        if let Ok(children) = children_query.get(emitter_entity) {
            for child in children.iter() {
                if sinks.get(child).is_ok() {
                    target_child = Some(child);
                    break;
                }
            }
        }

        if let Some(child) = target_child {
            if let Ok(mut sink) = sinks.get_mut(child) {
                let speed = Vec2::new(velocity.x as f32, velocity.z as f32).length();
                let should_play = grounded && speed > MOVE_ANIM_THRESHOLD;
                sink.set_volume(bevy::audio::Volume::Linear(footstep_base_vol * master));
                if should_play {
                    sink.play();
                    sink.set_speed(footstep_playback_speed(speed));
                } else {
                    sink.pause();
                }
            }
        }
    }
}
