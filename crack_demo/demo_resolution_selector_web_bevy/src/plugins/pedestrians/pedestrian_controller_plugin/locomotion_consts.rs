//! Shared locomotion speed caps, animation thresholds, and footstep playback tuning.

/// Minimum horizontal speed before locomotion clips replace idle.
pub const MOVE_ANIM_THRESHOLD: f32 = 0.25;

/// Non-sprint jog speed cap (end of the walk ramp).
pub const JOG_SPEED: f32 = 4.0;
/// Sprint ramps from `1 * JOG_SPEED` up to `SPRINT_MAX_MULT * JOG_SPEED` while Shift is held.
pub const SPRINT_MAX_MULT: f32 = 2.25;
/// sprint ramp time constant.
pub const SPRINT_RAMP_TIME: f32 = 2.5;

/// Walk ramp starts at this speed and reaches [`JOG_SPEED`] over [`WALK_RAMP_TIME`].
pub const WALK_START_SPEED: f32 = 1.0;
/// walk ramp time constant.
pub const WALK_RAMP_TIME: f32 = 1.5;

/// Walk band midpoint: `Walk_Loop` below, `Jog_Fwd_Loop` at/above (within non-sprint locomotion).
pub const WALK_ANIM_TOP: f32 = JOG_SPEED * 0.5;

/// Sprint band midpoint: `Jog_Fwd_Loop` below, `Sprint_Loop` above.
pub const SPRINT_ANIM_START: f32 = (JOG_SPEED + JOG_SPEED * SPRINT_MAX_MULT) * 0.5;

const FOOTSTEP_SPEED_WALK: f32 = 0.3;
const FOOTSTEP_SPEED_JOG: f32 = 1.17;
const FOOTSTEP_SPEED_SPRINT: f32 = 0.92;

/// Horizontal speed cap for the walk ramp at `walk_secs` seconds of continuous walking.
pub fn walk_speed_cap(walk_secs: f32) -> f32 {
    let t = (walk_secs / WALK_RAMP_TIME).clamp(0.0, 1.0);
    WALK_START_SPEED + (JOG_SPEED - WALK_START_SPEED) * t
}

/// Horizontal speed cap for the sprint ramp at `sprint_secs` seconds of continuous sprinting.
pub fn sprint_speed_cap(sprint_secs: f32) -> f32 {
    let t = (sprint_secs / SPRINT_RAMP_TIME).clamp(0.0, 1.0);
    JOG_SPEED * (1.0 + (SPRINT_MAX_MULT - 1.0) * t)
}

/// Inverse of [`walk_speed_cap`]: seed `walk_secs` from current horizontal speed (e.g. on sprint release).
pub fn walk_secs_from_speed(speed: f32) -> f32 {
    if speed <= WALK_START_SPEED {
        return 0.0;
    }
    let t = ((speed - WALK_START_SPEED) / (JOG_SPEED - WALK_START_SPEED)).clamp(0.0, 1.0);
    t * WALK_RAMP_TIME
}

/// Footstep loop playback speed aligned to the same speed bands as locomotion animation clips.
pub fn footstep_playback_speed(speed: f32) -> f32 {
    if speed <= WALK_ANIM_TOP {
        FOOTSTEP_SPEED_WALK
    } else if speed <= SPRINT_ANIM_START {
        FOOTSTEP_SPEED_JOG
    } else {
        FOOTSTEP_SPEED_SPRINT
    }
}
