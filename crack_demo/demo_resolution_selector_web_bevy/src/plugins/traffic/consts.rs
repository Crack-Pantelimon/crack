// --- spawning ---
pub const SPAWN_INTERVAL_S: f32 = 0.1; // min time between network spawns
pub const SPAWN_MIN_CAMERA_DIST: f32 = 20.0; // pop-in guard
pub const CAR_SPAWN_SPACING: f32 = 8.0; // min dist to any existing car
pub const PED_SPAWN_SPACING: f32 = 4.0; // min dist to any existing traffic ped
pub const SPAWN_BEHIND_MAX_DOT: f32 = 0.15; // dot(cam_fwd, dir_to_point) must be < this
// (i.e. at/behind the camera side plane)
pub const FAST_FILL_FRACTION: f32 = 0.4; // density threshold for fast fill mode

// --- despawn ---
pub const OUT_OF_RANGE_FACTOR: f32 = 1.25; // * spawn_radius, hysteresis
pub const OUT_OF_VIEW_DESPAWN_S: f32 = 4.0; // secs occluded/out-of-frustum before despawn
pub const VIEW_RAYCAST_HZ: f32 = 4.0; // visibility check rate
pub const CAR_TOP_FUDGE: f32 = 0.95; // fraction of full height for view target

// --- stuck / recovery ---
pub const STUCK_SPEED_EPS: f32 = 0.5; // m/s below = "not moving"
pub const STUCK_TRIGGER_S: f32 = 1.5; // secs stuck before reverse maneuver
pub const REVERSE_DURATION_S: f32 = 1.0; // "move back 1s"
pub const STUCK_HARD_DESPAWN_S: f32 = 12.0; // give up entirely (fallback)

// --- routing ---
pub const WAYPOINT_REACHED_XZ: f32 = 4.0;
pub const LOOKAHEAD_XZ: f32 = 8.0;

// --- pedestrian traffic ---
pub const PED_ROAD_OFFSET: f32 = 5.0; // metres from road centre
pub const PED_WALK_SPEED: f32 = 1.6; // informational; AI walk speed governs
pub const PED_STUCK_REROUTE_S: f32 = 1.0; // secs still before random reroute

// --- collision damage ---
pub const CAR_HIT_KMH_TO_DAMAGE: f32 = 1.0; // 100 km/h -> 100 dmg
pub const CAR_HIT_MIN_KMH: f32 = 8.0; // below this, no damage
pub const CAR_HIT_COOLDOWN_S: f32 = 0.5; // per (car,victim) re-hit guard
