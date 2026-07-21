use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::plugins::cars_driving::car_info;
use crate::plugins::cars_driving::driving_plugin::spawn_car::{
    ActivePlayerVehicle, Car, CarHealth, WheelAssets, select_car_wheel,
};
use crate::plugins::cars_driving::driving_plugin::{
    CarDriveState, CarWheelsContactData, CosmeticWheel, GamePhysicsLayer,
};
use crate::plugins::network::{ChatBubbles, ChatState};
use crate::plugins::pedestrians::animation::{
    ActiveOneShot, CurrentPlayingAnimation, NetworkDriven, TargetAnimation,
};
use crate::plugins::pedestrians::pedestrian_controller_plugin::{
    CharacterController, CharacterScale, Climbing, ControlledCharacter, Grounded, LocomotionInput,
    MainCamera, Rolling, character_collision_bundle,
};
use crate::plugins::pedestrians::{
    ModelRoot, PedestrianAnimations, PedestrianUrl, SpawnPedestrianEvent, locomotion_clip,
};
use crate::plugins::states::{GameControlState, InitialMapLoadFinished, NetworkConnectionState};
use crate::plugins::weapons::weapon_shooting::{MeleeDebugBox, MeleeDebugBoxes, ShotTracer};
use crate::plugins::weapons::{
    BulletSpark, BulletSparks, EquippedWeapon, FireGunEvent, GunState, ShotTracers, WeaponId,
    WeaponManifest, WeaponModel,
};
use crate::ui_egui::UiState;
use net_crackpipe::PublicKey;

// ---------------------------------------------------------------------------------------------
// Gameplay Room / Protocol Definition
// ---------------------------------------------------------------------------------------------

// The room type + message/presence types live in game_logic::network (the
// abstract chat machinery is in net_crackpipe; all game-specific message
// types belong to the game crates). Re-exported here so game code keeps its
// existing import paths.
pub use game_logic::network::{GameplayChatMessageContent, GameplayPresence, GameplaySyncRoomType};

// ---------------------------------------------------------------------------------------------
// Game-side Update Payloads
// ---------------------------------------------------------------------------------------------

/// game update.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameUpdate {
    /// Sender's monotonic time in seconds, for interpolation/extrapolation.
    pub t: f64,
/// state field.
    pub state: PlayerStateMsg,
    /// One-shot events accumulated since the previous update (never dropped by rate limiting).
    pub events: Vec<PlayerEventMsg>,
}

/// player state msg.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlayerStateMsg {
    /// GameControlState::MapFreecam — show a camera gizmo at this pose on other clients.
    Camera { pos: [f32; 3], rot: [f32; 4] },
    /// GameControlState::ControllingPedestrian
    OnFoot {
        /// Pedestrian model URL for the controlled character.
        model_url: String,
        /// Character scale multiplier.
        scale: f32,
/// Documented public item.
        pos: [f32; 3],
/// Documented public item.
        rot: [f32; 4],
/// Documented public item.
        vel: [f32; 3], // LinearVelocity, for extrapolation + anim speed
/// Documented public item.
        grounded: bool,
/// Documented public item.
        aiming: bool,   // crosshair/aim state (GameControlState + weapon raised)
/// Documented public item.
        weapon: String, // WeaponId label/serialized id
/// Documented public item.
        ammo: u32,      // GunState clip, for HUD-over-head later; cheap to include
/// Documented public item.
        health: f32,    // current HP (victim-authoritative)
    },
    /// GameControlState::DrivingCar
    InCar {
/// Documented public item.
        car_type: String, // car_info car type name -> resolves glb via get_car_asset
/// Documented public item.
        pos: [f32; 3],
/// Documented public item.
        rot: [f32; 4],
/// Documented public item.
        vel: [f32; 3],
/// Documented public item.
        speed_kmh: f32,
/// Documented public item.
        steer: f32,  // CarDriveState.current_steer_integrated -> front wheel pose
/// Documented public item.
        health: f32, // CarHealth.current
    },
}

/// player event msg.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlayerEventMsg {
    /// A gunshot: replayed on remote clients for tracer VFX + victim-side hit test.
    Shoot {
/// Documented public item.
        origin: [f32; 3],
/// Documented public item.
        dir: [f32; 3],
/// Documented public item.
        damage: f32,
    },
/// reload variant.
    Reload,
/// jump variant.
    Jump,
/// climb start variant.
    ClimbStart,
/// roll variant.
    Roll,
/// melee variant.
    Melee {
/// Documented public item.
        origin: [f32; 3],
/// Documented public item.
        rotation: [f32; 4],
/// Documented public item.
        is_melee: bool,
    },
}

// ---------------------------------------------------------------------------------------------
// Bevy Resources / Component Types
// ---------------------------------------------------------------------------------------------

/// game sync channels.
#[derive(Resource)]
pub struct GameSyncChannels {
/// outgoing tx field.
    pub outgoing_tx: async_channel::Sender<Vec<u8>>,
/// incoming rx field.
    pub incoming_rx: async_channel::Receiver<GameSyncInbound>,
}

/// game sync inbound.
pub struct GameSyncInbound {
/// from node id field.
    pub from_node_id: PublicKey,
/// nickname field.
    pub nickname: String,
/// color field.
    pub color: (u8, u8, u8),
/// id field.
    pub id: i64,
/// payload field.
    pub payload: Vec<u8>,
}

/// multiplayer config.
#[derive(Resource)]
pub struct MultiplayerConfig {
/// send hz field.
    pub send_hz: f32,
/// window open field.
    pub window_open: bool,
}

impl Default for MultiplayerConfig {
    fn default() -> Self {
        Self {
            send_hz: 20.0,
            window_open: false,
        }
    }
}

/// outbound events.
#[derive(Resource, Default)]
pub struct OutboundEvents(pub Vec<PlayerEventMsg>);

// NOTE: If a reconnect or lifecycle disconnect flow is added in the future,
// this resource must be cleared/reset on disconnect to avoid leftover state.
/// seen msg ids.
#[derive(Resource, Default)]
pub struct SeenMsgIds {
/// ids field.
    pub ids: HashSet<i64>,
/// ring field.
    pub ring: VecDeque<i64>,
}

impl SeenMsgIds {
/// is new.
    pub fn is_new(&mut self, id: i64) -> bool {
        if self.ids.contains(&id) {
            false
        } else {
            self.ids.insert(id);
            self.ring.push_back(id);
            if self.ring.len() > 1024 {
                if let Some(oldest) = self.ring.pop_front() {
                    self.ids.remove(&oldest);
                }
            }
            true
        }
    }
}

// NOTE: If a reconnect or lifecycle disconnect flow is added in the future,
// this resource must be cleared/reset on disconnect to avoid leftover state.
/// remote players.
#[derive(Resource, Default)]
pub struct RemotePlayers(pub HashMap<PublicKey, RemotePlayer>);

/// remote player.
pub struct RemotePlayer {
/// nickname field.
    pub nickname: String,
/// color field.
    pub color: (u8, u8, u8),
/// prev local t field.
    pub prev_local_t: f64,
/// latest local t field.
    pub latest_local_t: f64,
/// prev field.
    pub prev: Option<GameUpdate>,
/// latest field.
    pub latest: Option<GameUpdate>,
/// avatar field.
    pub avatar: RemoteAvatar,
/// pending events field.
    pub pending_events: Vec<PlayerEventMsg>,
}

/// remote avatar.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteAvatar {
/// none variant.
    None,
/// camera variant.
    Camera,
    /// Remote pedestrian avatar with spawned root entity.
    OnFoot {
        /// Scene root entity for the remote pedestrian.
        root: Entity,
        /// Model URL used to spawn the avatar mesh.
        model_url: String,
    },
    /// Remote player driving a vehicle.
    InCar {
        /// Scene root entity for the remote car.
        root: Entity,
        /// Car type identifier from the spawn manifest.
        car_type: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum AvatarKind {
    None,
    Camera,
    OnFoot { model_url: String },
    InCar { car_type: String },
}

/// remote avatar marker.
#[derive(Component)]
pub struct RemoteAvatarMarker {
/// node id field.
    pub node_id: PublicKey,
}

// NOTE: If a reconnect or lifecycle disconnect flow is added in the future,
// this resource must be cleared/reset on disconnect to avoid leftover state.
/// multiplayer stats.
#[derive(Resource, Default)]
pub struct MultiplayerStats {
/// connected field.
    pub connected: bool,
/// msgs sent field.
    pub msgs_sent: u64,
/// msgs recv field.
    pub msgs_recv: u64,
/// bytes sent field.
    pub bytes_sent: u64,
/// bytes recv field.
    pub bytes_recv: u64,
/// dup drops field.
    pub dup_drops: u64,
/// decode errors field.
    pub decode_errors: u64,
/// channel full drops field.
    pub channel_full_drops: u64,

    // Per-second window tracking
/// window start field.
    pub window_start: f64,
/// window msgs sent field.
    pub window_msgs_sent: u64,
/// window msgs recv field.
    pub window_msgs_recv: u64,
/// rate msgs sent field.
    pub rate_msgs_sent: f64,
/// rate msgs recv field.
    pub rate_msgs_recv: f64,
}

fn tick_multiplayer_stats_window(time: Res<Time>, mut stats: ResMut<MultiplayerStats>) {
    let now = time.elapsed_secs_f64();
    if stats.window_start == 0.0 {
        stats.window_start = now;
    }
    let elapsed = now - stats.window_start;
    if elapsed >= 1.0 {
        stats.rate_msgs_sent = stats.window_msgs_sent as f64 / elapsed;
        stats.rate_msgs_recv = stats.window_msgs_recv as f64 / elapsed;
        stats.window_msgs_sent = 0;
        stats.window_msgs_recv = 0;
        stats.window_start = now;
    }
}

// ---------------------------------------------------------------------------------------------
// Bevy Plugin Implementation
// ---------------------------------------------------------------------------------------------

/// multiplayer plugin.
pub struct MultiplayerPlugin;

impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MultiplayerConfig>()
            .init_resource::<OutboundEvents>()
            .init_resource::<SeenMsgIds>()
            .init_resource::<RemotePlayers>()
            .init_resource::<MultiplayerStats>()
            .add_observer(multiplayer_fire_gun_observer)
            .add_systems(
                Update,
                (
                    tick_multiplayer_stats_window,
                    collect_outbound_events,
                    send_local_state.run_if(in_state(InitialMapLoadFinished::Finished)),
                    receive_game_sync,
                    reconcile_remote_avatars,
                    interpolate_remote_avatars,
                    update_remote_animations,
                    apply_remote_events,
                    draw_camera_gizmos,
                )
                    .chain()
                    .run_if(in_state(NetworkConnectionState::Connected)),
            )
            .add_systems(
                bevy_egui::EguiPrimaryContextPass,
                (
                    draw_remote_billboards.run_if(in_state(NetworkConnectionState::Connected)),
                    draw_self_billboard.run_if(in_state(NetworkConnectionState::Connected)),
                    multiplayer_debug_ui,
                ),
            );
    }
}

// ---------------------------------------------------------------------------------------------
// System Implementations
// ---------------------------------------------------------------------------------------------

fn collect_outbound_events(
    controlled: Res<ControlledCharacter>,
    q_locomotion: Query<&LocomotionInput, With<CharacterController>>,
    q_climbing: Query<&Climbing, Added<Climbing>>,
    q_rolling: Query<&Rolling, Added<Rolling>>,
    q_melee: Query<
        (
            &crate::plugins::weapons::weapon_shooting::PendingMeleeHit,
            &Transform,
        ),
        Added<crate::plugins::weapons::weapon_shooting::PendingMeleeHit>,
    >,
    mut outbound: ResMut<OutboundEvents>,
    mut last_jump: Local<bool>,
) {
    let Some(controller) = controlled.controller else {
        return;
    };

    // Jump
    if let Ok(input) = q_locomotion.get(controller) {
        if input.jump {
            if !*last_jump {
                outbound.0.push(PlayerEventMsg::Jump);
            }
            *last_jump = true;
        } else {
            *last_jump = false;
        }
    }

    // Climb
    if q_climbing.contains(controller) {
        outbound.0.push(PlayerEventMsg::ClimbStart);
    }

    // Roll
    if q_rolling.contains(controller) {
        outbound.0.push(PlayerEventMsg::Roll);
    }

    // Melee
    for (hit, transform) in q_melee.iter() {
        outbound.0.push(PlayerEventMsg::Melee {
            origin: transform.translation.to_array(),
            rotation: transform.rotation.to_array(),
            is_melee: hit.is_melee,
        });
    }
}

fn multiplayer_fire_gun_observer(
    trigger: On<FireGunEvent>,
    controlled: Res<ControlledCharacter>,
    shooters: Query<&EquippedWeapon>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut outbound: ResMut<OutboundEvents>,
) {
    let shooter = trigger.event().shooter;
    if Some(shooter) != controlled.controller {
        return;
    }
    let Ok(equipped) = shooters.get(shooter) else {
        return;
    };
    let WeaponId::Gun(info) = &equipped.0 else {
        return;
    };

    let Some(cam) = camera.iter().next() else {
        return;
    };
    let origin = cam.translation();
    let dir = cam.forward();

    outbound.0.push(PlayerEventMsg::Shoot {
        origin: origin.into(),
        dir: (*dir).to_array(),
        damage: info.damage,
    });
}

fn send_local_state(
    time: Res<Time>,
    config: Res<MultiplayerConfig>,
    mut timer: Local<f32>,
    control_state: Res<State<GameControlState>>,
    camera_query: Query<&GlobalTransform, With<MainCamera>>,
    controlled: Res<ControlledCharacter>,
    q_ped: Query<(
        &Transform,
        &LinearVelocity,
        &CharacterScale,
        Has<Grounded>,
        Option<&EquippedWeapon>,
        Option<&GunState>,
        &crate::plugins::pedestrian_ai::faction::Health,
        Option<&PedestrianUrl>,
    )>,
    active_vehicle: Query<Entity, With<ActivePlayerVehicle>>,
    q_car: Query<(
        &Transform,
        &LinearVelocity,
        &Car,
        &CarDriveState,
        &CarHealth,
    )>,
    mouse: Res<ButtonInput<MouseButton>>,
    channels: Option<Res<GameSyncChannels>>,
    mut outbound_events: ResMut<OutboundEvents>,
    mut stats: ResMut<MultiplayerStats>,
) {
    let Some(channels) = channels.as_ref() else {
        return;
    };
    *timer += time.delta_secs();
    let interval = 1.0 / config.send_hz;
    if *timer < interval {
        return;
    }
    *timer = 0.0;

    let state_msg = match control_state.get() {
        GameControlState::MapFreecam => {
            if let Some(cam) = camera_query.iter().next() {
                PlayerStateMsg::Camera {
                    pos: cam.translation().into(),
                    rot: cam.rotation().into(),
                }
            } else {
                return;
            }
        }
        GameControlState::ControllingPedestrian => {
            if let Some(controller) = controlled.controller {
                if let Ok((transform, vel, scale, grounded, equipped, gun_state, health, url)) =
                    q_ped.get(controller)
                {
                    let model_url = url.map(|u| u.0.clone()).unwrap_or_default();
                    let weapon_label = equipped
                        .map(|eq| eq.0.label())
                        .unwrap_or_else(|| "Unarmed".to_string());
                    let ammo = gun_state.map(|g| g.rounds).unwrap_or(0);
                    let aiming = mouse.pressed(MouseButton::Right);

                    PlayerStateMsg::OnFoot {
                        model_url,
                        scale: scale.0,
                        pos: transform.translation.into(),
                        rot: transform.rotation.into(),
                        vel: vel.0.into(),
                        grounded,
                        aiming,
                        weapon: weapon_label,
                        ammo,
                        health: health.current,
                    }
                } else {
                    return;
                }
            } else {
                return;
            }
        }
        GameControlState::DrivingCar => {
            if let Some(av) = active_vehicle.iter().next() {
                if let Ok((transform, vel, car, drive_state, health)) = q_car.get(av) {
                    PlayerStateMsg::InCar {
                        car_type: car._car_type.clone(),
                        pos: transform.translation.into(),
                        rot: transform.rotation.into(),
                        vel: vel.0.into(),
                        speed_kmh: vel.0.length() * 3.6,
                        steer: drive_state.current_steer_integrated,
                        health: health.current,
                    }
                } else {
                    return;
                }
            } else {
                return;
            }
        }
    };

    let events = std::mem::take(&mut outbound_events.0);
    let update = GameUpdate {
        t: time.elapsed_secs_f64(),
        state: state_msg,
        events,
    };

    match postcard::to_allocvec(&update) {
        Ok(payload) => {
            let payload_len = payload.len() as u64;
            if let Err(_err) = channels.outgoing_tx.try_send(payload) {
                stats.channel_full_drops += 1;
                let mut old_events = update.events;
                old_events.append(&mut outbound_events.0);
                outbound_events.0 = old_events;
            } else {
                stats.msgs_sent += 1;
                stats.window_msgs_sent += 1;
                stats.bytes_sent += payload_len;
            }
        }
        Err(_err) => {
            let mut old_events = update.events;
            old_events.append(&mut outbound_events.0);
            outbound_events.0 = old_events;
        }
    }
}

fn receive_game_sync(
    time: Res<Time>,
    channels: Option<Res<GameSyncChannels>>,
    mut remote_players: ResMut<RemotePlayers>,
    mut seen_ids: ResMut<SeenMsgIds>,
    mut stats: ResMut<MultiplayerStats>,
    mut commands: Commands,
) {
    let Some(channels) = channels.as_ref() else {
        return;
    };
    let now = time.elapsed_secs_f64();

    while let Ok(inbound) = channels.incoming_rx.try_recv() {
        stats.msgs_recv += 1;
        stats.window_msgs_recv += 1;
        stats.bytes_recv += inbound.payload.len() as u64;

        if !seen_ids.is_new(inbound.id) {
            stats.dup_drops += 1;
            continue;
        }

        let mut update: GameUpdate = match postcard::from_bytes(&inbound.payload) {
            Ok(u) => u,
            Err(e) => {
                stats.decode_errors += 1;
                tracing::warn!("Failed to decode GameUpdate: {:?}", e);
                continue;
            }
        };

        use std::collections::hash_map::Entry;
        let player = match remote_players.0.entry(inbound.from_node_id) {
            Entry::Vacant(v) => {
                commands.trigger(
                    crate::plugins::notifications::NotificationEvent::PlayerJoinedGame {
                        nickname: inbound.nickname.clone(),
                        color: inbound.color,
                    },
                );
                v.insert(RemotePlayer {
                    nickname: inbound.nickname,
                    color: inbound.color,
                    prev_local_t: now,
                    latest_local_t: now,
                    prev: None,
                    latest: None,
                    avatar: RemoteAvatar::None,
                    pending_events: Vec::new(),
                })
            }
            Entry::Occupied(o) => o.into_mut(),
        };

        player.prev = player.latest.clone();
        player.prev_local_t = player.latest_local_t;
        player.pending_events.extend(update.events.drain(..));
        player.latest = Some(update);
        player.latest_local_t = now;
    }
}

fn reconcile_remote_avatars(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    wheel_assets: Option<Res<WheelAssets>>,
    mut remote_players: ResMut<RemotePlayers>,
    weapon_manifest: Res<WeaponManifest>,
    mut q_equipped: Query<&mut EquippedWeapon>,
) {
    let now = time.elapsed_secs_f64();
    let mut to_remove = Vec::new();

    for (node_id, player) in remote_players.0.iter_mut() {
        // Despawn peers silent for more than 10 seconds. Kept generous so a
        // brief GameSync gap (gossip mesh reshuffle, GC hitch) doesn't churn a
        // leave/rejoin; the direct-mesh join_peers fix keeps the stream flowing.
        if now - player.latest_local_t > 10.0 {
            to_remove.push(*node_id);
            continue;
        }

        let Some(ref latest) = player.latest else {
            continue;
        };

        // Determine desired avatar state
        let desired_kind = match latest.state.clone() {
            PlayerStateMsg::Camera { .. } => AvatarKind::Camera,
            PlayerStateMsg::OnFoot { model_url, .. } => AvatarKind::OnFoot { model_url },
            PlayerStateMsg::InCar { car_type, .. } => AvatarKind::InCar { car_type },
        };

        let current_kind = match &player.avatar {
            RemoteAvatar::None => AvatarKind::None,
            RemoteAvatar::Camera => AvatarKind::Camera,
            RemoteAvatar::OnFoot { model_url, .. } => AvatarKind::OnFoot {
                model_url: model_url.clone(),
            },
            RemoteAvatar::InCar { car_type, .. } => AvatarKind::InCar {
                car_type: car_type.clone(),
            },
        };

        if desired_kind != current_kind {
            // Despawn old avatar
            match player.avatar {
                RemoteAvatar::OnFoot { root, .. } => {
                    commands.entity(root).despawn();
                }
                RemoteAvatar::InCar { root, .. } => {
                    commands.entity(root).despawn();
                }
                _ => {}
            }
            player.avatar = RemoteAvatar::None;

            // Spawn new avatar
            match desired_kind {
                AvatarKind::Camera => {
                    player.avatar = RemoteAvatar::Camera;
                }
                AvatarKind::OnFoot { model_url } => {
                    if model_url.is_empty() {
                        continue;
                    }

                    let pos = match latest.state.clone() {
                        PlayerStateMsg::OnFoot { pos, .. } => Vec3::from_array(pos),
                        _ => Vec3::ZERO,
                    };
                    let rot = match latest.state.clone() {
                        PlayerStateMsg::OnFoot { rot, .. } => Quat::from_array(rot),
                        _ => Quat::IDENTITY,
                    };
                    let scale = match latest.state.clone() {
                        PlayerStateMsg::OnFoot { scale, .. } => scale,
                        _ => 1.0,
                    };
                    let health = match latest.state.clone() {
                        PlayerStateMsg::OnFoot { health, .. } => health,
                        _ => 100.0,
                    };
                    let weapon = match latest.state.clone() {
                        PlayerStateMsg::OnFoot { weapon, .. } => weapon,
                        _ => "Unarmed".to_string(),
                    };

                    let root = commands.spawn((
                        Name::new(format!("RemotePlayer_{}", player.nickname)),
                        Transform::from_translation(pos).with_rotation(rot),
                        Visibility::default(),
                        InheritedVisibility::default(),
                        RemoteAvatarMarker { node_id: *node_id },
                        NetworkDriven,
                        LinearVelocity::ZERO,
                        crate::plugins::pedestrians::pedestrian_controller_plugin::AnimState::default(),
                        crate::plugins::pedestrians::pedestrian_controller_plugin::CombatState::default(),
                        crate::plugins::pedestrians::pedestrian_controller_plugin::CharacterScale(scale),
                        crate::plugins::pedestrian_ai::faction::Health { current: health, max: 100.0 },
                        EquippedWeapon(WeaponId::from_label(&weapon, &weapon_manifest)),
                        // Kinematic capsule matching the local character's physics
                        // footprint (root = capsule center), so the local car and
                        // player collide with remote pedestrians.
                        RigidBody::Kinematic,
                        character_collision_bundle(),
                    )).id();

                    // Intermediate scale node
                    let scale_node = commands.spawn((
                        Name::new("RemotePedestrianScaleNode"),
                        ChildOf(root),
                        Transform::from_xyz(0.0, -crate::plugins::pedestrians::pedestrian_controller_plugin::CAPSULE_HALF_HEIGHT, 0.0)
                            .with_scale(Vec3::splat(scale)),
                        Visibility::default(),
                        InheritedVisibility::default(),
                    )).id();

                    commands.trigger(SpawnPedestrianEvent {
                        url: PedestrianUrl(model_url.clone()),
                        position: pos,
                        controller: root,
                        parent: scale_node,
                    });

                    player.avatar = RemoteAvatar::OnFoot { root, model_url };
                }
                AvatarKind::InCar { car_type } => {
                    if car_type.is_empty() {
                        continue;
                    }

                    let pos = match latest.state.clone() {
                        PlayerStateMsg::InCar { pos, .. } => Vec3::from_array(pos),
                        _ => Vec3::ZERO,
                    };
                    let rot = match latest.state.clone() {
                        PlayerStateMsg::InCar { rot, .. } => Quat::from_array(rot),
                        _ => Quat::IDENTITY,
                    };
                    let health = match latest.state.clone() {
                        PlayerStateMsg::InCar { health, .. } => health,
                        _ => 1000.0,
                    };

                    let root = spawn_cosmetic_car(
                        &mut commands,
                        &asset_server,
                        wheel_assets.as_ref().map(|w| w.as_ref()),
                        *node_id,
                        &player.nickname,
                        &car_type,
                        pos,
                        rot,
                        health,
                    );

                    player.avatar = RemoteAvatar::InCar { root, car_type };
                }
                _ => {}
            }
        } else {
            // desired_kind == current_kind: update EquippedWeapon if OnFoot and weapon changed
            if let RemoteAvatar::OnFoot { root, .. } = player.avatar {
                if let PlayerStateMsg::OnFoot {
                    weapon: latest_weapon_label,
                    ..
                } = &latest.state
                {
                    if let Ok(mut equipped) = q_equipped.get_mut(root) {
                        if equipped.0.label() != *latest_weapon_label {
                            equipped.0 =
                                WeaponId::from_label(latest_weapon_label, &weapon_manifest);
                        }
                    } else {
                        commands
                            .entity(root)
                            .insert(EquippedWeapon(WeaponId::from_label(
                                latest_weapon_label,
                                &weapon_manifest,
                            )));
                    }
                }
            }
        }
    }

    // Cleanup timed out players
    for node_id in to_remove {
        if let Some(player) = remote_players.0.remove(&node_id) {
            commands.trigger(
                crate::plugins::notifications::NotificationEvent::PlayerLeftGame {
                    nickname: player.nickname.clone(),
                    color: player.color,
                },
            );
            match player.avatar {
                RemoteAvatar::OnFoot { root, .. } => {
                    commands.entity(root).despawn();
                }
                RemoteAvatar::InCar { root, .. } => {
                    commands.entity(root).despawn();
                }
                _ => {}
            }
        }
    }
}

fn spawn_cosmetic_car(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    wheel_assets: Option<&WheelAssets>,
    node_id: PublicKey,
    nickname: &str,
    car_type: &str,
    pos: Vec3,
    rot: Quat,
    health: f32,
) -> Entity {
    let default_drive_state = CarDriveState::default();
    let car_asset_handle = car_info::get_car_asset(car_type, asset_server);

    let car_entity = commands
        .spawn((
            Name::new(format!("RemoteCar_{}", nickname)),
            Transform::from_translation(pos).with_rotation(rot),
            Visibility::default(),
            InheritedVisibility::default(),
            default_drive_state,
            CarWheelsContactData::default(),
            LinearVelocity::ZERO,
            Car {
                _car_type: car_type.to_string(),
            },
            CarHealth {
                current: health,
                max: 1000.0,
            },
            WorldAssetRoot(car_asset_handle),
            RemoteAvatarMarker { node_id },
            // Kinematic obstacle driven by the interpolated network pose: the
            // local (dynamic) car collides with it, while this car's pose stays
            // authoritative on its owner's simulation. Filter is [Car] only —
            // a kinematic body has no response to the static map anyway.
            (
                RigidBody::Kinematic,
                ColliderConstructorHierarchy::new(ColliderConstructor::ConvexDecompositionFromMesh)
                    .with_default_layers(CollisionLayers::new(
                        [GamePhysicsLayer::Car],
                        [GamePhysicsLayer::Car],
                    )),
            ),
        ))
        .id();

    let wheel_handle = if let Some(w) = wheel_assets {
        select_car_wheel(car_type, w, asset_server)
    } else {
        car_info::get_wheel_asset("car-wheel_00003_", asset_server)
    };

    for i in 0..4 {
        commands.spawn((
            WorldAssetRoot(wheel_handle.clone()),
            Transform::from_scale(Vec3::splat(0.6)),
            CosmeticWheel {
                wheel_idx: i,
                parent_car: car_entity,
                accumulated_rotation: 0.0,
                measured_radius: None,
            },
        ));
    }

    car_entity
}

fn interpolate_remote_avatars(
    time: Res<Time>,
    config: Res<MultiplayerConfig>,
    remote_players: Res<RemotePlayers>,
    mut q_transforms: Query<(Entity, &mut Transform, &RemoteAvatarMarker)>,
    mut q_vel: Query<&mut LinearVelocity>,
    mut q_drive: Query<&mut CarDriveState>,
    mut q_health: Query<&mut crate::plugins::pedestrian_ai::faction::Health>,
    mut q_car_health: Query<&mut CarHealth>,
) {
    let now = time.elapsed_secs_f64();
    let interpolation_delay = 1.5 * (1.0 / config.send_hz) as f64;
    let render_t = now - interpolation_delay;

    for (entity, mut transform, marker) in q_transforms.iter_mut() {
        let Some(player) = remote_players.0.get(&marker.node_id) else {
            continue;
        };
        let Some(ref latest) = player.latest else {
            continue;
        };

        // Extrapolate or Interpolate
        if let Some(ref prev) = player.prev {
            let denom = player.latest_local_t - player.prev_local_t;
            if denom > 0.001 {
                let fraction = ((render_t - player.prev_local_t) / denom) as f32;

                if fraction >= 0.0 && fraction <= 1.0 {
                    let prev_state = prev.state.clone();
                    let latest_state = latest.state.clone();
                    // Interpolate between prev and latest!
                    match (prev_state, latest_state) {
                        (
                            PlayerStateMsg::OnFoot {
                                pos: p_pos,
                                rot: p_rot,
                                vel: p_vel,
                                health: p_h,
                                ..
                            },
                            PlayerStateMsg::OnFoot {
                                pos: l_pos,
                                rot: l_rot,
                                vel: l_vel,
                                health: l_h,
                                ..
                            },
                        ) => {
                            let p_pos = Vec3::from_array(p_pos);
                            let l_pos = Vec3::from_array(l_pos);
                            let p_rot = Quat::from_array(p_rot);
                            let l_rot = Quat::from_array(l_rot);

                            transform.translation = p_pos.lerp(l_pos, fraction);
                            transform.rotation = p_rot.slerp(l_rot, fraction);

                            if let Ok(mut vel) = q_vel.get_mut(entity) {
                                let p_vel = Vec3::from_array(p_vel);
                                let l_vel = Vec3::from_array(l_vel);
                                vel.0 = p_vel.lerp(l_vel, fraction);
                            }

                            if let Ok(mut hp) = q_health.get_mut(entity) {
                                hp.current = p_h + (l_h - p_h) * fraction;
                            }
                        }
                        (
                            PlayerStateMsg::InCar {
                                pos: p_pos,
                                rot: p_rot,
                                vel: p_vel,
                                steer: p_steer,
                                health: p_h,
                                ..
                            },
                            PlayerStateMsg::InCar {
                                pos: l_pos,
                                rot: l_rot,
                                vel: l_vel,
                                steer: l_steer,
                                health: l_h,
                                ..
                            },
                        ) => {
                            let p_pos = Vec3::from_array(p_pos);
                            let l_pos = Vec3::from_array(l_pos);
                            let p_rot = Quat::from_array(p_rot);
                            let l_rot = Quat::from_array(l_rot);

                            transform.translation = p_pos.lerp(l_pos, fraction);
                            transform.rotation = p_rot.slerp(l_rot, fraction);

                            if let Ok(mut vel) = q_vel.get_mut(entity) {
                                let p_vel = Vec3::from_array(p_vel);
                                let l_vel = Vec3::from_array(l_vel);
                                vel.0 = p_vel.lerp(l_vel, fraction);
                            }

                            if let Ok(mut hp) = q_car_health.get_mut(entity) {
                                hp.current = p_h + (l_h - p_h) * fraction;
                            }

                            if let Ok(mut drive) = q_drive.get_mut(entity) {
                                drive.current_steer_integrated =
                                    p_steer + (l_steer - p_steer) * fraction;
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
            }
        }

        // Extrapolate from latest
        let extrap_t = (render_t - player.latest_local_t).max(0.0) as f32;
        let extrap_t = extrap_t.min(0.2); // cap extrapolation at 200ms

        match latest.state.clone() {
            PlayerStateMsg::Camera { .. } => {
                // Camera peers have no entity; nicknames handled in draw_remote_billboards.
            }
            PlayerStateMsg::OnFoot {
                pos,
                rot,
                vel,
                health,
                ..
            } => {
                let l_pos = Vec3::from_array(pos);
                let l_vel = Vec3::from_array(vel);
                transform.translation = l_pos + l_vel * extrap_t;
                transform.rotation = Quat::from_array(rot);

                if let Ok(mut v) = q_vel.get_mut(entity) {
                    v.0 = l_vel;
                }
                if let Ok(mut hp) = q_health.get_mut(entity) {
                    hp.current = health;
                }
            }
            PlayerStateMsg::InCar {
                pos,
                rot,
                vel,
                steer,
                health,
                ..
            } => {
                let l_pos = Vec3::from_array(pos);
                let l_vel = Vec3::from_array(vel);
                transform.translation = l_pos + l_vel * extrap_t;
                transform.rotation = Quat::from_array(rot);

                if let Ok(mut v) = q_vel.get_mut(entity) {
                    v.0 = l_vel;
                }
                if let Ok(mut hp) = q_car_health.get_mut(entity) {
                    hp.current = health;
                }
                if let Ok(mut drive) = q_drive.get_mut(entity) {
                    drive.current_steer_integrated = steer;
                }
            }
        }
    }
}

fn select_animation(anims: &PedestrianAnimations, candidates: &[&str]) -> String {
    for &c in candidates {
        if anims.nodes.contains_key(c) {
            return c.to_string();
        }
    }
    anims
        .default_animation()
        .unwrap_or_else(|| "A_TPose".to_string())
}

fn find_model_entity(
    root: Entity,
    q_models: &Query<(Entity, &ChildOf), With<ModelRoot>>,
    q_parents: &Query<&ChildOf>,
) -> Option<Entity> {
    for (m_ent, child_of) in q_models.iter() {
        let mut cur = child_of.parent();
        loop {
            if cur == root {
                return Some(m_ent);
            }
            if let Ok(c) = q_parents.get(cur) {
                cur = c.parent();
            } else {
                break;
            }
        }
    }
    None
}

fn find_animation_player(
    model_ent: Entity,
    player_entities: &[Entity],
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    for &player_ent in player_entities {
        let mut cur = player_ent;
        loop {
            if cur == model_ent {
                return Some(player_ent);
            }
            if let Ok(child_of) = parents.get(cur) {
                cur = child_of.parent();
            } else {
                break;
            }
        }
    }
    None
}

fn update_remote_animations(
    mut commands: Commands,
    anims: Res<PedestrianAnimations>,
    q_remote_roots: Query<
        (
            Entity,
            &LinearVelocity,
            &crate::plugins::pedestrian_ai::faction::Health,
            &crate::plugins::pedestrians::pedestrian_controller_plugin::CharacterScale,
        ),
        With<NetworkDriven>,
    >,
    q_models: Query<(Entity, &ChildOf), With<ModelRoot>>,
    q_parents: Query<&ChildOf>,
) {
    if !anims.ready {
        return;
    }

    for (root_entity, velocity, health, char_scale) in q_remote_roots.iter() {
        if let Some(model_ent) = find_model_entity(root_entity, &q_models, &q_parents) {
            let horizontal_speed = Vec2::new(velocity.0.x, velocity.0.z).length();
            let anim_name = if health.current <= 0.0 {
                select_animation(&anims, &["Death01", "Death"])
            } else {
                select_animation(&anims, locomotion_clip(horizontal_speed, false, false))
            };

            let mut anim_speed = 1.0 / char_scale.0;
            if anim_name == "Walk_Loop" && horizontal_speed > 0.0 {
                anim_speed *= (horizontal_speed / 1.5).clamp(0.5, 1.5);
            } else if anim_name == "Jog_Fwd_Loop" && horizontal_speed > 0.0 {
                anim_speed *= (horizontal_speed / 3.0).clamp(0.5, 1.5);
            } else if anim_name == "Sprint_Loop" && horizontal_speed > 0.0 {
                anim_speed *= (horizontal_speed / 5.0).clamp(0.5, 1.5);
            }

            commands.entity(model_ent).insert(TargetAnimation {
                name: anim_name,
                speed: anim_speed,
            });
        }
    }
}
fn select_node(
    anims: &PedestrianAnimations,
    candidates: &[&str],
) -> Option<(AnimationNodeIndex, String)> {
    for &c in candidates {
        if let Some(&node) = anims.nodes.get(c) {
            return Some((node, c.to_string()));
        }
    }
    None
}

fn apply_remote_events(
    mut commands: Commands,
    anims: Res<PedestrianAnimations>,
    mut remote_players: ResMut<RemotePlayers>,
    q_transforms: Query<(Entity, &Transform, &RemoteAvatarMarker)>,
    weapon_models: Query<(Entity, &GlobalTransform), With<WeaponModel>>,
    parents: Query<&ChildOf>,
    spatial: SpatialQuery,
    controlled: Res<ControlledCharacter>,
    active_vehicle: Query<Entity, With<ActivePlayerVehicle>>,
    mut q_car_health: Query<&mut CarHealth>,
    mut tracers: ResMut<ShotTracers>,
    mut sparks: ResMut<BulletSparks>,
    mut debug_boxes: ResMut<MeleeDebugBoxes>,
    q_models: Query<(Entity, &ChildOf), With<ModelRoot>>,
    mut q_players: Query<(
        Entity,
        &mut AnimationPlayer,
        Option<&mut CurrentPlayingAnimation>,
    )>,
    queries: (
        Query<(), With<CharacterController>>,
        Query<&RemoteAvatarMarker>,
        Query<&crate::plugins::pedestrians::pedestrian_controller_plugin::CharacterScale>,
        Query<&Children>,
    ),
) {
    let (q_controller, q_remote_marker, q_scales, q_children) = queries;
    let local_controller = controlled.controller;
    let local_vehicle = active_vehicle.iter().next();

    let player_entities: Vec<Entity> = q_players.iter().map(|(e, _, _)| e).collect();

    let keys: Vec<PublicKey> = remote_players.0.keys().cloned().collect();
    for key in keys {
        let mut events = Vec::new();
        let mut latest_state = None;
        let mut avatar = RemoteAvatar::None;
        if let Some(player) = remote_players.0.get_mut(&key) {
            events = std::mem::take(&mut player.pending_events);
            latest_state = player.latest.clone();
            avatar = player.avatar.clone();
        }
        if events.is_empty() {
            continue;
        }

        let fallback_pos = if let Some(ref latest) = latest_state {
            match &latest.state {
                PlayerStateMsg::Camera { pos, .. } => Vec3::from_array(*pos),
                PlayerStateMsg::OnFoot { pos, .. } => Vec3::from_array(*pos),
                PlayerStateMsg::InCar { pos, .. } => Vec3::from_array(*pos),
            }
        } else {
            Vec3::ZERO
        };

        let avatar_entity = match avatar {
            RemoteAvatar::OnFoot { root, .. } => Some(root),
            RemoteAvatar::InCar { root, .. } => Some(root),
            _ => None,
        };

        let pos = if let Some(ent) = avatar_entity {
            if let Ok((_, transform, _)) = q_transforms.get(ent) {
                transform.translation
            } else {
                fallback_pos
            }
        } else {
            fallback_pos
        };

        let muzzle = if let Some(ent) = avatar_entity {
            find_muzzle_position(ent, pos, &weapon_models, &parents)
        } else {
            pos + Vec3::Y * 0.4
        };

        for event in events {
            match event {
                PlayerEventMsg::Shoot {
                    origin,
                    dir,
                    damage,
                } => {
                    let origin = Vec3::from_array(origin);
                    let dir = Vec3::from_array(dir);

                    // Audio
                    commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                        fx: crate::plugins::audio::audio_fx::AudioFxEventType::GunShot {
                            sound_idx: 0,
                        },
                        position: muzzle,
                        follow: None,
                    });

                    // VFX. Exclude the shooter's whole avatar subtree: its
                    // colliders live on GLB child entities (car body meshes),
                    // so excluding only the root would let the replayed shot
                    // impact the shooter's own avatar at the muzzle.
                    let filter = if let Some(ent) = avatar_entity {
                        SpatialQueryFilter::from_excluded_entities(collect_subtree(
                            ent,
                            &q_children,
                        ))
                    } else {
                        SpatialQueryFilter::default()
                    };
                    let range = 150.0;
                    if let Ok(ray_dir) = Dir3::new(dir) {
                        if let Some(hit) = spatial.cast_ray(origin, ray_dir, range, true, &filter) {
                            let impact = origin + dir * hit.distance;
                            let normal: Vec3 = hit.normal;
                            let reflect =
                                (dir - 2.0 * dir.dot(normal) * normal).normalize_or_zero();

                            tracers.0.push(ShotTracer {
                                from: muzzle,
                                to: impact,
                                reflect_to: Some(impact + reflect * 0.5),
                                ttl: 0.05,
                            });

                            commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                                fx: crate::plugins::audio::audio_fx::AudioFxEventType::BulletImpact,
                                position: impact,
                                follow: None,
                            });

                            // Hit sparks
                            let is_person = is_person_hit_entity(
                                hit.entity,
                                &parents,
                                &q_controller,
                                &q_remote_marker,
                                &remote_players,
                            );
                            for _ in 0..4 {
                                let rx = rand::random::<f32>() - 0.5;
                                let ry = rand::random::<f32>() - 0.5;
                                let rz = rand::random::<f32>() - 0.5;
                                let jump_dir =
                                    (reflect + Vec3::new(rx, ry, rz) * 0.5).normalize_or_zero();
                                let speed = if is_person {
                                    rand::random::<f32>() * 1.5 + 0.8
                                } else {
                                    rand::random::<f32>() * 4.0 + 3.0
                                };
                                sparks.0.push(BulletSpark {
                                    position: impact,
                                    velocity: jump_dir * speed,
                                    is_person,
                                    lifetime: 1.0,
                                });
                            }

                            // Victim-authoritative damage
                            let hit_root = get_root_entity(hit.entity, &parents);
                            if Some(hit_root) == local_controller {
                                commands.trigger(
                                    crate::plugins::pedestrian_ai::combat::DamageEvent {
                                        target: hit_root,
                                        amount: damage,
                                        source: avatar_entity.unwrap_or(Entity::PLACEHOLDER),
                                    },
                                );
                            } else if Some(hit_root) == local_vehicle {
                                if let Ok(mut car_hp) = q_car_health.get_mut(hit_root) {
                                    car_hp.current = (car_hp.current - damage).max(0.0);
                                }
                            }
                        } else {
                            // Missed
                            tracers.0.push(ShotTracer {
                                from: muzzle,
                                to: origin + dir * range,
                                reflect_to: None,
                                ttl: 0.05,
                            });
                        }
                    }
                }
                PlayerEventMsg::Reload => {
                    commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                        fx: crate::plugins::audio::audio_fx::AudioFxEventType::GunReload,
                        position: muzzle,
                        follow: None,
                    });
                }
                PlayerEventMsg::Jump => {
                    if anims.ready {
                        if let Some(root) = avatar_entity {
                            if let Some(model_ent) = find_model_entity(root, &q_models, &parents) {
                                if let Some(player_ent) =
                                    find_animation_player(model_ent, &player_entities, &parents)
                                {
                                    if let Ok((_, mut player, mut current_playing)) =
                                        q_players.get_mut(player_ent)
                                    {
                                        if let Some((node_index, name)) =
                                            select_node(&anims, &["Jump_Start", "Jump_Loop"])
                                        {
                                            player.stop_all();
                                            let scale = q_scales.get(root).map_or(1.0, |s| s.0);
                                            let speed = 1.0 / scale;
                                            player.play(node_index).set_speed(speed);
                                            commands.entity(player_ent).insert(ActiveOneShot {
                                                node: node_index,
                                                name: name.clone(),
                                            });
                                            if let Some(ref mut curr) = current_playing {
                                                curr.name = name;
                                                curr.speed = speed;
                                            } else {
                                                commands.entity(player_ent).insert(
                                                    CurrentPlayingAnimation { name, speed },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                PlayerEventMsg::ClimbStart => {
                    commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                        fx: crate::plugins::audio::audio_fx::AudioFxEventType::Climb,
                        position: pos,
                        follow: None,
                    });
                    if anims.ready {
                        if let Some(root) = avatar_entity {
                            if let Some(model_ent) = find_model_entity(root, &q_models, &parents) {
                                if let Some(player_ent) =
                                    find_animation_player(model_ent, &player_entities, &parents)
                                {
                                    if let Ok((_, mut player, mut current_playing)) =
                                        q_players.get_mut(player_ent)
                                    {
                                        if let Some((node_index, name)) =
                                            select_node(&anims, &["Roll", "Jump_Loop"])
                                        {
                                            player.stop_all();
                                            let scale = q_scales.get(root).map_or(1.0, |s| s.0);
                                            let speed = 1.0 / scale;
                                            player.play(node_index).set_speed(speed);
                                            commands.entity(player_ent).insert(ActiveOneShot {
                                                node: node_index,
                                                name: name.clone(),
                                            });
                                            if let Some(ref mut curr) = current_playing {
                                                curr.name = name;
                                                curr.speed = speed;
                                            } else {
                                                commands.entity(player_ent).insert(
                                                    CurrentPlayingAnimation { name, speed },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                PlayerEventMsg::Roll => {
                    if anims.ready {
                        if let Some(root) = avatar_entity {
                            if let Some(model_ent) = find_model_entity(root, &q_models, &parents) {
                                if let Some(player_ent) =
                                    find_animation_player(model_ent, &player_entities, &parents)
                                {
                                    if let Ok((_, mut player, mut current_playing)) =
                                        q_players.get_mut(player_ent)
                                    {
                                        if let Some((node_index, name)) =
                                            select_node(&anims, &["Roll"])
                                        {
                                            player.stop_all();
                                            let scale = q_scales.get(root).map_or(1.0, |s| s.0);
                                            let speed = 1.0 / scale;
                                            player.play(node_index).set_speed(speed);
                                            commands.entity(player_ent).insert(ActiveOneShot {
                                                node: node_index,
                                                name: name.clone(),
                                            });
                                            if let Some(ref mut curr) = current_playing {
                                                curr.name = name;
                                                curr.speed = speed;
                                            } else {
                                                commands.entity(player_ent).insert(
                                                    CurrentPlayingAnimation { name, speed },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                PlayerEventMsg::Melee {
                    origin,
                    rotation,
                    is_melee,
                } => {
                    let origin = Vec3::from_array(origin);
                    let rotation = Quat::from_array(rotation);

                    // 1. Play Whoosh Sound
                    commands.trigger(crate::plugins::audio::audio_fx::AudioFxEvent {
                        fx: crate::plugins::audio::audio_fx::AudioFxEventType::MeleeWhoosh {
                            volume: if is_melee { 1.0 } else { 0.4 },
                        },
                        position: origin,
                        follow: None,
                    });

                    // 2. Play animation on remote player's avatar
                    if anims.ready {
                        if let Some(root) = avatar_entity {
                            if let Some(model_ent) = find_model_entity(root, &q_models, &parents) {
                                if let Some(player_ent) =
                                    find_animation_player(model_ent, &player_entities, &parents)
                                {
                                    if let Ok((_, mut player, mut current_playing)) =
                                        q_players.get_mut(player_ent)
                                    {
                                        let anim_candidates = if is_melee {
                                            &["Sword_Attack"][..]
                                        } else {
                                            &["Punch_Jab", "Punch_Cross"][..]
                                        };
                                        if let Some((node_index, name)) =
                                            select_node(&anims, anim_candidates)
                                        {
                                            player.stop_all();
                                            let scale = q_scales.get(root).map_or(1.0, |s| s.0);
                                            let speed = 1.0 / scale;
                                            player.play(node_index).set_speed(speed);
                                            commands.entity(player_ent).insert(ActiveOneShot {
                                                node: node_index,
                                                name: name.clone(),
                                            });
                                            if let Some(ref mut curr) = current_playing {
                                                curr.name = name;
                                                curr.speed = speed;
                                            } else {
                                                commands.entity(player_ent).insert(
                                                    CurrentPlayingAnimation { name, speed },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 3. Add debug box for visual wireframe
                    debug_boxes.0.push(MeleeDebugBox {
                        position: origin + rotation * Vec3::Z * 1.0,
                        rotation,
                        ttl: 0.1,
                    });

                    // 4. Victim-authoritative damage to local player
                    if let Some(local_ent) = local_controller {
                        // Check if the local controller is within the 1x1x2m box in front of the remote hips
                        let box_shape = Collider::cuboid(1.0, 1.0, 2.0);
                        let filter = SpatialQueryFilter::default();
                        let intersections = spatial.shape_intersections(
                            &box_shape,
                            origin + rotation * Vec3::Z * 1.0,
                            rotation,
                            &filter,
                        );
                        // If local controller (or any of its child entities) is in the intersections list, the local player is hit!
                        let mut hit = false;
                        for ent in intersections {
                            let mut cur = ent;
                            loop {
                                if cur == local_ent {
                                    hit = true;
                                    break;
                                }
                                match parents.get(cur) {
                                    Ok(child_of) => cur = child_of.parent(),
                                    Err(_) => break,
                                }
                            }
                            if hit {
                                break;
                            }
                        }

                        if hit {
                            let amount = if is_melee {
                                crate::plugins::pedestrian_ai::combat::SWORD_DAMAGE
                            } else {
                                crate::plugins::pedestrian_ai::combat::PUNCH_DAMAGE
                            };
                            commands.trigger(crate::plugins::pedestrian_ai::combat::DamageEvent {
                                target: local_ent,
                                amount,
                                source: avatar_entity.unwrap_or(Entity::PLACEHOLDER),
                            });
                        }
                    }
                } // _ => {}
            }
        }
    }
}
// ---------------------------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------------------------

fn collect_subtree(root: Entity, children: &Query<&Children>) -> Vec<Entity> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        if let Ok(kids) = children.get(out[i]) {
            out.extend(kids.iter());
        }
        i += 1;
    }
    out
}

fn get_root_entity(mut entity: Entity, parents: &Query<&ChildOf>) -> Entity {
    while let Ok(child_of) = parents.get(entity) {
        entity = child_of.parent();
    }
    entity
}

fn find_muzzle_position(
    root: Entity,
    fallback: Vec3,
    weapon_models: &Query<(Entity, &GlobalTransform), With<WeaponModel>>,
    parents: &Query<&ChildOf>,
) -> Vec3 {
    for (w_entity, gt) in weapon_models.iter() {
        let mut cur = w_entity;
        let mut found = false;
        while let Ok(child_of) = parents.get(cur) {
            let p = child_of.parent();
            if p == root {
                found = true;
                break;
            }
            cur = p;
        }
        if found {
            return gt.translation();
        }
    }
    fallback + Vec3::Y * 0.4
}

fn is_person_hit_entity(
    hit_entity: Entity,
    parents: &Query<&ChildOf>,
    q_controller: &Query<(), With<CharacterController>>,
    q_remote_marker: &Query<&RemoteAvatarMarker>,
    remote_players: &RemotePlayers,
) -> bool {
    let mut cur = hit_entity;
    loop {
        if q_controller.contains(cur) {
            return true;
        }
        if let Ok(marker) = q_remote_marker.get(cur) {
            if let Some(player) = remote_players.0.get(&marker.node_id) {
                if matches!(player.avatar, RemoteAvatar::OnFoot { .. }) {
                    return true;
                }
            }
        }
        match parents.get(cur) {
            Ok(child_of) => cur = child_of.parent(),
            Err(_) => break,
        }
    }
    false
}

fn draw_camera_gizmos(mut gizmos: Gizmos, remote_players: Res<RemotePlayers>) {
    for player in remote_players.0.values() {
        let Some(ref latest) = player.latest else {
            continue;
        };
        let state_clone = latest.state.clone();
        if let PlayerStateMsg::Camera { pos, rot } = state_clone {
            let pos = Vec3::from_array(pos);
            let rot = Quat::from_array(rot);
            let color = Color::srgb(
                player.color.0 as f32 / 255.0,
                player.color.1 as f32 / 255.0,
                player.color.2 as f32 / 255.0,
            );

            let forward = rot * Vec3::NEG_Z;
            let up = rot * Vec3::Y;
            let right = rot * Vec3::X;

            let aspect = 1.3;
            let size = 0.4;
            let dist = 0.6;

            let center_fwd = pos + forward * dist;
            let h = up * (size * 0.5);
            let w = right * (size * aspect * 0.5);

            let tl = center_fwd + h - w;
            let tr = center_fwd + h + w;
            let bl = center_fwd - h - w;
            let br = center_fwd - h + w;

            gizmos.line(pos, tl, color);
            gizmos.line(pos, tr, color);
            gizmos.line(pos, bl, color);
            gizmos.line(pos, br, color);

            gizmos.line(tl, tr, color);
            gizmos.line(tr, br, color);
            gizmos.line(br, bl, color);
            gizmos.line(bl, tl, color);

            gizmos.line(pos + up * 0.1, pos + up * 0.3, color);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// UI / Billboard Renderers
// ---------------------------------------------------------------------------------------------

fn draw_remote_billboards(
    mut contexts: EguiContexts,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    remote_players: Res<RemotePlayers>,
    q_transforms: Query<(&Transform, &RemoteAvatarMarker)>,
    q_ped_scale: Query<&CharacterScale>,
    mut bubbles: Option<ResMut<ChatBubbles>>,
    time: Res<Time>,
    _q_local_ped: Query<(&Transform, Option<&CharacterScale>), With<CharacterController>>,
    q_local_car: Query<&Transform, With<ActivePlayerVehicle>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Some((camera, camera_transform)) = q_camera.iter().next() else {
        return;
    };

    let now = time.elapsed_secs_f64();
    if let Some(ref mut bubbles) = bubbles {
        bubbles.by_node.retain(|_, (_, expiry)| *expiry > now);
    }

    for (transform, marker) in q_transforms.iter() {
        let Some(player) = remote_players.0.get(&marker.node_id) else {
            continue;
        };

        let mut height = 1.2;
        if let RemoteAvatar::OnFoot { root, .. } = player.avatar {
            if let Ok(scale) = q_ped_scale.get(root) {
                height = 1.8 * scale.0;
            } else {
                height = 1.8;
            }
        } else if let RemoteAvatar::InCar { .. } = player.avatar {
            height = 1.6;
        }

        let world_pos = transform.translation + Vec3::Y * height;
        if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, world_pos) {
            let egui_pos = egui::pos2(screen_pos.x, screen_pos.y);

            let mut show_bubble = None;
            if let Some(ref bubbles) = bubbles {
                if let Some((bubble_text, expiry)) = bubbles.by_node.get(&marker.node_id) {
                    if *expiry > now {
                        show_bubble = Some(bubble_text.clone());
                    }
                }
            }

            egui::Area::new(egui::Id::new(format!("billboard_{}", marker.node_id)))
                .fixed_pos(egui_pos)
                .pivot(egui::Align2::CENTER_BOTTOM)
                .show(ctx, |ui| {
                    ui.vertical(|ui| {
                        if let Some(text) = show_bubble {
                            egui::Frame::default()
                                .fill(egui::Color32::from_black_alpha(180))
                                .corner_radius(4)
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .show(ui, |ui| {
                                    ui.label(text);
                                });
                        }

                        let color =
                            egui::Color32::from_rgb(player.color.0, player.color.1, player.color.2);
                        ui.colored_label(color, &player.nickname);

                        let mut hp_pct = None;
                        if let Some(ref update) = player.latest {
                            match update.state {
                                PlayerStateMsg::OnFoot { health, .. } => {
                                    hp_pct = Some(health / 100.0);
                                }
                                PlayerStateMsg::InCar { health, .. } => {
                                    hp_pct = Some(health / 1000.0);
                                }
                                _ => {}
                            }
                        }

                        if let Some(pct) = hp_pct {
                            let pct = pct.clamp(0.0, 1.0);
                            let bar_color = if pct > 0.5 {
                                egui::Color32::GREEN
                            } else if pct > 0.25 {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::RED
                            };
                            let width = 50.0;
                            let height_bar = 4.0;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(width, height_bar),
                                egui::Sense::hover(),
                            );
                            ui.painter()
                                .rect_filled(rect, 1.0, egui::Color32::DARK_GRAY);
                            let mut filled_rect = rect;
                            filled_rect.set_width(width * pct);
                            ui.painter().rect_filled(filled_rect, 1.0, bar_color);
                        }
                    });
                });
        }
    }

    // Second pass: Draw labels for Camera players (who don't have spawned avatar entities)
    for (node_id, player) in remote_players.0.iter() {
        if player.avatar != RemoteAvatar::Camera {
            continue;
        }
        let Some(ref latest) = player.latest else {
            continue;
        };
        if let PlayerStateMsg::Camera { pos, .. } = &latest.state {
            let world_pos = Vec3::from_array(*pos) + Vec3::Y * 0.3; // offset slightly above camera center
            if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, world_pos) {
                let egui_pos = egui::pos2(screen_pos.x, screen_pos.y);

                egui::Area::new(egui::Id::new(format!("billboard_cam_{:?}", node_id)))
                    .fixed_pos(egui_pos)
                    .pivot(egui::Align2::CENTER_BOTTOM)
                    .show(ctx, |ui| {
                        ui.vertical(|ui| {
                            if let Some(ref bubbles) = bubbles {
                                if let Some((bubble_text, expiry)) = bubbles.by_node.get(node_id) {
                                    if *expiry > now {
                                        egui::Frame::default()
                                            .fill(egui::Color32::from_black_alpha(180))
                                            .corner_radius(4)
                                            .inner_margin(egui::Margin::symmetric(8, 4))
                                            .show(ui, |ui| {
                                                ui.label(bubble_text);
                                            });
                                    }
                                }
                            }

                            let color = egui::Color32::from_rgb(
                                player.color.0,
                                player.color.1,
                                player.color.2,
                            );
                            ui.colored_label(color, &player.nickname);
                        });
                    });
            }
        }
    }

    // Third pass: Draw own chat bubble if present and not expired (only when in car; when on foot, draw_self_billboard handles it)
    if let Some(ref bubbles) = bubbles {
        if let Some((bubble_text, expiry)) = &bubbles.own {
            if *expiry > now {
                let mut local_pos = None;
                let mut height = 1.2;
                if let Some(car_transform) = q_local_car.iter().next() {
                    local_pos = Some(car_transform.translation);
                    height = 1.6;
                }

                if let Some(pos) = local_pos {
                    let world_pos = pos + Vec3::Y * height;
                    if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, world_pos) {
                        let egui_pos = egui::pos2(screen_pos.x, screen_pos.y);
                        egui::Area::new(egui::Id::new("own_chat_bubble"))
                            .fixed_pos(egui_pos)
                            .pivot(egui::Align2::CENTER_BOTTOM)
                            .show(ctx, |ui| {
                                egui::Frame::default()
                                    .fill(egui::Color32::from_black_alpha(180))
                                    .corner_radius(4)
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.label(bubble_text);
                                    });
                            });
                    }
                }
            }
        }
    }
}

fn draw_self_billboard(
    mut contexts: EguiContexts,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    controlled: Res<ControlledCharacter>,
    q_controlled_data: Query<(
        &GlobalTransform,
        &crate::plugins::pedestrian_ai::faction::Health,
        &CharacterScale,
    )>,
    chat_state: Res<ChatState>,
    bubbles: Res<ChatBubbles>,
    time: Res<Time>,
) {
    let Some(controller) = controlled.controller else {
        return;
    };
    let Ok((gt, health, scale)) = q_controlled_data.get(controller) else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Some((camera, camera_transform)) = q_camera.iter().next() else {
        return;
    };

    let world_pos = gt.translation() + Vec3::Y * 1.8 * scale.0;
    if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, world_pos) {
        let egui_pos = egui::pos2(screen_pos.x, screen_pos.y);

        let now = time.elapsed_secs_f64();
        let show_bubble = bubbles.own.as_ref().and_then(|(text, expiry)| {
            if *expiry > now {
                Some(text.clone())
            } else {
                None
            }
        });

        egui::Area::new(egui::Id::new("billboard_self"))
            .fixed_pos(egui_pos)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    if let Some(text) = show_bubble {
                        egui::Frame::default()
                            .fill(egui::Color32::from_black_alpha(180))
                            .corner_radius(4)
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.label(text);
                            });
                    }

                    let color = egui::Color32::from_rgb(
                        chat_state.own_color.0,
                        chat_state.own_color.1,
                        chat_state.own_color.2,
                    );
                    ui.colored_label(color, &chat_state.own_nickname);

                    let pct = (health.current / health.max).clamp(0.0, 1.0);
                    let bar_color = if pct > 0.5 {
                        egui::Color32::GREEN
                    } else if pct > 0.25 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::RED
                    };
                    let width = 50.0;
                    let height_bar = 4.0;
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(width, height_bar), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 1.0, egui::Color32::DARK_GRAY);
                    let mut filled_rect = rect;
                    filled_rect.set_width(width * pct);
                    ui.painter().rect_filled(filled_rect, 1.0, bar_color);
                });
            });
    }
}

fn multiplayer_debug_ui(
    mut contexts: EguiContexts,
    ui_state: Option<ResMut<UiState>>,
    mut config: ResMut<MultiplayerConfig>,
    stats: Res<MultiplayerStats>,
    remote_players: Res<RemotePlayers>,
    time: Res<Time>,
    connection_state: Res<State<NetworkConnectionState>>,
) {
    let show = ui_state.map(|s| s.show_multiplayer_debug).unwrap_or(true);
    if !show {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let now = time.elapsed_secs_f64();

    egui::Window::new("Multiplayer Networking")
        .default_open(true)
        .show(ctx, |ui| {
            ui.heading("Configuration");
            ui.add(egui::Slider::new(&mut config.send_hz, 5.0..=30.0).text("Send Hz"));

            ui.separator();

            ui.heading("Statistics");
            ui.label(format!("Connection: {:?}", connection_state.get()));
            ui.label(format!(
                "game room: {}",
                if stats.connected { "joined" } else { "joining" }
            ));
            ui.label(format!(
                "Sent: {} msgs ({:.2} KB) — {:.1} msgs/s",
                stats.msgs_sent,
                stats.bytes_sent as f64 / 1024.0,
                stats.rate_msgs_sent
            ));
            ui.label(format!(
                "Received: {} msgs ({:.2} KB) — {:.1} msgs/s",
                stats.msgs_recv,
                stats.bytes_recv as f64 / 1024.0,
                stats.rate_msgs_recv
            ));
            ui.label(format!("Duplicates Dropped: {}", stats.dup_drops));
            ui.label(format!("Decode Errors: {}", stats.decode_errors));
            ui.label(format!("Channel-Full Drops: {}", stats.channel_full_drops));

            ui.separator();

            ui.heading("Connected Peers");
            if remote_players.0.is_empty() {
                ui.label("No other players connected.");
            } else {
                egui::Grid::new("peers_grid")
                    .num_columns(4)
                    .spacing([15.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Nickname");
                        ui.label("Avatar");
                        ui.label("Last Msg");
                        ui.label("Health");
                        ui.end_row();

                        for player in remote_players.0.values() {
                            let color = egui::Color32::from_rgb(
                                player.color.0,
                                player.color.1,
                                player.color.2,
                            );
                            ui.colored_label(color, &player.nickname);

                            let (avatar_str, hp_str) = match &player.avatar {
                                RemoteAvatar::None => ("None".to_string(), "N/A".to_string()),
                                RemoteAvatar::Camera => ("Camera".to_string(), "N/A".to_string()),
                                RemoteAvatar::OnFoot { .. } => {
                                    let hp = player
                                        .latest
                                        .as_ref()
                                        .map(|l| match &l.state {
                                            PlayerStateMsg::OnFoot { health, .. } => {
                                                format!("{:.0}/100", health)
                                            }
                                            _ => "100/100".to_string(),
                                        })
                                        .unwrap_or_else(|| "100/100".to_string());
                                    ("On Foot".to_string(), hp)
                                }
                                RemoteAvatar::InCar { car_type, .. } => {
                                    let hp = player
                                        .latest
                                        .as_ref()
                                        .map(|l| match &l.state {
                                            PlayerStateMsg::InCar { health, .. } => {
                                                format!("{:.0}/1000", health)
                                            }
                                            _ => "1000/1000".to_string(),
                                        })
                                        .unwrap_or_else(|| "1000/1000".to_string());
                                    (format!("In Car ({})", car_type), hp)
                                }
                            };

                            ui.label(avatar_str);
                            ui.label(format!("{:.1}s ago", now - player.latest_local_t));
                            ui.label(hp_str);
                            ui.end_row();
                        }
                    });
            }
        });
}
