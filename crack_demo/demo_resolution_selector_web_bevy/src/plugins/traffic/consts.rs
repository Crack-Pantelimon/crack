// --- spawning ---
/// spawn interval s constant.
pub const SPAWN_INTERVAL_S: f32 = 0.1; // min time between network spawns
/// spawn min camera dist constant.
pub const SPAWN_MIN_CAMERA_DIST: f32 = 20.0; // pop-in guard
/// car spawn spacing constant.
pub const CAR_SPAWN_SPACING: f32 = 8.0; // min dist to any existing car
/// ped spawn spacing constant.
pub const PED_SPAWN_SPACING: f32 = 4.0; // min dist to any existing traffic ped
/// spawn behind max dot constant.
pub const SPAWN_BEHIND_MAX_DOT: f32 = 0.15; // dot(cam_fwd, dir_to_point) must be < this
// (i.e. at/behind the camera side plane)
/// fast fill fraction constant.
pub const FAST_FILL_FRACTION: f32 = 0.4; // density threshold for fast fill mode

// --- despawn ---
/// out of range factor constant.
pub const OUT_OF_RANGE_FACTOR: f32 = 1.25; // * spawn_radius, hysteresis
/// out of view despawn s constant.
pub const OUT_OF_VIEW_DESPAWN_S: f32 = 4.0; // secs occluded/out-of-frustum before despawn
/// view raycast hz constant.
pub const VIEW_RAYCAST_HZ: f32 = 4.0; // visibility check rate
/// car top fudge constant.
pub const CAR_TOP_FUDGE: f32 = 0.95; // fraction of full height for view target

// --- stuck / recovery ---
/// stuck speed eps constant.
pub const STUCK_SPEED_EPS: f32 = 0.5; // m/s below = "not moving"
/// stuck trigger s constant.
pub const STUCK_TRIGGER_S: f32 = 1.5; // secs stuck before reverse maneuver
/// reverse duration s constant.
pub const REVERSE_DURATION_S: f32 = 1.0; // "move back 1s"
/// stuck hard despawn s constant.
pub const STUCK_HARD_DESPAWN_S: f32 = 12.0; // give up entirely (fallback)

// --- routing ---
/// waypoint reached xz constant.
pub const WAYPOINT_REACHED_XZ: f32 = 4.0;
/// lookahead xz constant.
pub const LOOKAHEAD_XZ: f32 = 8.0;

// --- pedestrian traffic ---
/// ped road offset constant.
pub const PED_ROAD_OFFSET: f32 = 5.0; // metres from road centre
/// ped walk speed constant.
pub const PED_WALK_SPEED: f32 = 1.6; // informational; AI walk speed governs
/// ped stuck reroute s constant.
pub const PED_STUCK_REROUTE_S: f32 = 1.0; // secs still before random reroute

// --- collision damage ---
/// car hit kmh to damage constant.
pub const CAR_HIT_KMH_TO_DAMAGE: f32 = 1.0; // 100 km/h -> 100 dmg
/// car hit min kmh constant.
pub const CAR_HIT_MIN_KMH: f32 = 8.0; // below this, no damage
/// car hit cooldown s constant.
pub const CAR_HIT_COOLDOWN_S: f32 = 0.5; // per (car,victim) re-hit guard
