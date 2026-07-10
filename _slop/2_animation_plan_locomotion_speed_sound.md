# Locomotion animation / sound speed — plan

**Sub-problem:** Walking and sprinting should both *accelerate* (ramp up over
time), the animation should cross-fade walk→jog (walking) and jog→sprint
(sprinting) across the ramp, the footstep **sound** is mistuned (walk ~3× too
fast, run ~10% too fast), and all of the walk/run animation + sound constants
should be **centralized** in one place in the controller code.

---

## Current state (scattered)

Speed caps & anim thresholds — `pedestrian_controller_plugin/mod.rs`:
- `CROUCH_SPEED = 1.8`, `JOG_SPEED = 4.0`
  ([mod.rs:103-104](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L103-L104))
- `SPRINT_MAX_MULT = 2.25`, `SPRINT_RAMP_TIME = 2.5`
  ([mod.rs:106-107](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L106-L107))
- `MOVE_ANIM_THRESHOLD = 0.25`, `WALK_MAX_SPEED = 2.0`, `JOG_MAX_SPEED = 4.5`
  ([mod.rs:110-112](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L110-L112))

Speed cap application — `apply_speed_cap`
([controller.rs:195-236](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs#L195-L236)):
- **Sprint** ramps: `cap = JOG_SPEED * (1 + (SPRINT_MAX_MULT-1) * sprint_secs/SPRINT_RAMP_TIME)`.
- **Walk (no shift)** is a *flat* cap `JOG_SPEED` — `MOVE_ACCEL = 200.0`
  ([mod.rs:98](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L98))
  snaps the character to 4.0 almost instantly, so there is **no walk ramp today**.
- `sprint_secs` lives in `MovementModifiers`
  ([mod.rs:208-214](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L208-L214)).

Anim clip selection — `locomotion_clip`
([animation.rs:20-37](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L20-L37)):
idle / `Walk_Loop` (<2.0) / `Jog_Fwd_Loop` (<4.5) / `Sprint_Loop`. Shared by
player, AI (`ai_animation`), and network drivers, so it must stay a pure
`speed → clip` function.

Footstep sound — `manage_footsteps_system`
([audio_fx.rs:303-320](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/audio_fx.rs#L303-L320)):
```
let playback_speed = if speed < 2.2 { 0.9 } else if speed < 5.0 { 1.3 } else { 1.02 };
```
Hardcoded, in the audio module, disconnected from the anim constants.

---

## Target behavior

### 1. Walking accelerates (walk→jog anim across the ramp)
Add a **walk ramp** mirroring the sprint ramp. While moving with no shift / no
crouch, ramp the non-sprint cap from a low start speed up to `JOG_SPEED` over
`WALK_RAMP_TIME`:
- new field `walk_secs: f32` in `MovementModifiers`.
- in `apply_speed_cap`, the non-crouch / non-sprint branch becomes:
  `cap = WALK_START_SPEED + (JOG_SPEED - WALK_START_SPEED) * (walk_secs/WALK_RAMP_TIME)`.
- advance `walk_secs` while walking, reset to 0 when idle/sprint/crouch (same
  pattern as `sprint_secs` at
  [controller.rs:208-213](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/controller.rs#L208-L213)).

Animation split over the walk band `[MOVE_ANIM_THRESHOLD, JOG_SPEED]`:
- lower half → `Walk_Loop`, upper half → `Jog_Fwd_Loop`.
- `WALK_ANIM_TOP = JOG_SPEED * 0.5` (= 2.0, matches today's `WALK_MAX_SPEED`).

### 2. Sprinting accelerates (jog→sprint anim across the ramp)
The speed ramp already "starts from jog speed": at `sprint_secs = 0` the cap is
`JOG_SPEED * 1.0`, ramping to `JOG_SPEED * SPRINT_MAX_MULT` (= 9.0). Keep it.
Only the **anim split** changes so the jog clip holds through the bottom of the
sprint band and the sprint clip only kicks in up top:
- `SPRINT_ANIM_START = (JOG_SPEED + JOG_SPEED*SPRINT_MAX_MULT) * 0.5` (= 6.5),
  i.e. the midpoint of the sprint band.
- `Jog_Fwd_Loop` for `WALK_ANIM_TOP < speed <= SPRINT_ANIM_START`,
  `Sprint_Loop` for `speed > SPRINT_ANIM_START`.

Rewrite `locomotion_clip` to use these derived thresholds (replaces the current
`WALK_MAX_SPEED` / `JOG_MAX_SPEED` bands). It stays a pure `speed→clip` map, so
AI and network drivers get the same crossfade for free.

### 3. Footstep sound retune
User: walk sound ~3× too fast, run sound ~10% too fast. Retune the bands and
pull them out of `audio_fx.rs` into the shared constants:
- walk band `0.9 → ~0.3` (÷3).
- jog band `1.3 → ~1.17` (×0.9).
- sprint band `1.02 → ~0.92` (×0.9).
Align the band thresholds to the anim bands (`WALK_ANIM_TOP`, `SPRINT_ANIM_START`)
instead of the ad-hoc `2.2` / `5.0`, so sound and animation switch at the same
speeds. Prefer a single helper `footstep_playback_speed(speed) -> f32` living
next to the locomotion constants and called from `manage_footsteps_system`
([audio_fx.rs:310-317](crack_demo/demo_resolution_selector_web_bevy/src/plugins/audio/audio_fx.rs#L310-L317)).

## Centralization

Create one constants block (a new `locomotion_consts.rs` submodule of
`pedestrian_controller_plugin`, re-exported from its `mod.rs`) holding:
`MOVE_ANIM_THRESHOLD`, `JOG_SPEED`, `SPRINT_MAX_MULT`, `SPRINT_RAMP_TIME`,
`WALK_START_SPEED`, `WALK_RAMP_TIME`, `WALK_ANIM_TOP`, `SPRINT_ANIM_START`, and
the footstep-speed helper. Then:
- `apply_speed_cap` (controller.rs) reads the speed/ramp constants,
- `locomotion_clip` (animation.rs) reads `WALK_ANIM_TOP` / `SPRINT_ANIM_START`,
- `manage_footsteps_system` (audio_fx.rs) calls `footstep_playback_speed`.
Delete the now-unused `WALK_MAX_SPEED` / `JOG_MAX_SPEED` (or redefine them as the
new derived values) and fix the imports in
[animation.rs:10-12](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L10-L12).

## Decisions to make

- **Walk↔sprint ramp handoff:** when Shift is released mid-sprint the speed
  drops from the sprint cap back toward the walk cap. Decide whether `walk_secs`
  resumes from 0 (brief re-accelerate) or is seeded from current speed so it
  doesn't visibly lurch. Recommend seeding `walk_secs` so `cap(walk_secs) ≈`
  current horizontal speed on the frame Shift releases.
- **`WALK_START_SPEED` / `WALK_RAMP_TIME` values:** suggest `WALK_START_SPEED ≈
  1.0`, `WALK_RAMP_TIME ≈ 1.5` (shorter than the 2.5 s sprint ramp so walking
  reaches jog pace reasonably fast). Tune in-app.
- Exact footstep multipliers are perceptual — start at 0.3 / 1.17 / 0.92 and
  adjust by ear.

## Test

`/run`, control a ped:
- Hold W (no shift) from a stop → speed visibly ramps; `Walk_Loop` plays first,
  swaps to `Jog_Fwd_Loop` around half speed; footsteps sound ~3× slower than now
  and match stride.
- Add Shift → keeps accelerating from jog pace to sprint; anim holds jog then
  swaps to `Sprint_Loop` near top speed; run footsteps ~10% slower.
- Confirm AI peds and remote avatars show the same walk→jog→sprint crossfade
  (shared `locomotion_clip`).


use "unset ARGV0" to escape the cursor unknwon proxy name problem with the appimage issue and check the code builds using "cargo check --bin ... --package ..."