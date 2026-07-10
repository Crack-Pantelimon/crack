# Car rider positions & seated-pose T-pose — plan

**Sub-problem:** Two bugs when spawning a car from the menu:
1. **Riders sit too far forward** — driver + front-passenger heads float out
   through the windshield. All occupants should shift **back ~0.5 m** in car-local
   space.
2. **T-pose passengers** — some occupants show the bind-pose T instead of the
   seated idle animation.

---

## Part 1 — shift every rider back 0.5 m

There are **two** seat-offset sources, both in car-local space; both must move:

- **Passengers (AI, seats 1–3)** — `CAR_SEAT_OFFSETS`
  ([spawn_car.rs:22-27](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs#L22-L27)),
  consumed when the passenger controller is spawned as a child of the car
  ([spawn_ai.rs:77-96](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/spawn_ai.rs#L77-L96))
  and when the seat world position is computed
  ([spawn_car.rs:262](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs#L262)).
- **The live player driver mesh (seat 0)** — `CarSeatOffset` default
  ([interaction_ui.rs:163-170](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L163-L170)),
  applied by `apply_seat_offset`
  ([interaction_ui.rs:631-632](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L631-L632)).
  (Note: `CAR_SEAT_OFFSETS[0]` exists but seat 0 is skipped in the passenger loop
  at [spawn_car.rs:258-261](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/driving_plugin/spawn_car.rs#L258-L261) —
  the driver goes through `SpawnPlayerDriverEvent` → `CarSeatOffset`. Keep them
  consistent anyway so an eventual AI-driver path lines up.)

**Change:** add `+0.5` to the **Z** of all four `CAR_SEAT_OFFSETS` entries and of
`CarSeatOffset::default().offset.z` (0.0 → 0.5).

> ⚠️ **Direction check.** The user specified *positive* Z = "back". But the raw
> seat data has rear seats at `z = -0.7` and front seats at `z = +0.15`, i.e. the
> data alone implies rear-of-car = **−Z**. This is muddied by the `y_rot = PI` /
> `Quat::from_rotation_y(PI)` applied to each seated mesh
> ([spawn_ai.rs:83](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/spawn_ai.rs#L83)).
> Implement `+0.5 Z` as instructed, then **verify in-app**: if it pushes heads
> *further* out the windshield, flip the sign to `−0.5 Z`. This is a one-character
> change either way; don't agonize, just look at the running car.

Y-offset (seat height) and the `y_rot` facing are unchanged — only the
forward/back axis moves.

## Part 2 — T-pose passengers

### Root cause
Passengers are AI peds; `ai_animation` picks their clip:
```
} else if car_passenger.is_some() {
    &["Sitting_Idle_Loop"]
```
([anim_ai.rs:64-65](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/anim_ai.rs#L64-L65)),
triggering a `PedestrianAnimationControlEvent` **only when the clip name changes**
([anim_ai.rs:73-83](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrian_ai/anim_ai.rs#L73-L83)).
That sets `TargetAnimation`, applied by `play_animations_system`.

The bug is in `play_animations_system`: it gates the desired clip on the
**per-model** gltf's own named animations —
```
let anim_name = if gltf.named_animations.contains_key(desired.as_str()) {
    desired
} else if let Some(def) = anims.default_animation() { def } ...
```
([animation.rs:211-217](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L211-L217)) —
but then it **plays from the shared graph** `anims.nodes`
([animation.rs:230](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L230)).
The shared `AnimationGraph` is built once from the *first* pedestrian asset and
reused for all models (they share bone names). So when a specific passenger's own
gltf happens **not** to list `Sitting_Idle_Loop` among its `named_animations`, the
gate falls back to `default_animation()` = **`A_TPose`**
([animation.rs:60-66](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/animation.rs#L60-L66)) →
the T-pose. Because `ai_animation` only re-fires on a clip *change* and a seated
passenger's clip never changes, it never recovers.

The **driver mesh never hits this** because it's driven per-frame via `node_for`
against the shared `anims.nodes` with a fallback list
(`["Driving_Loop", "Sitting_Idle_Loop", "Sitting_Enter"]`,
[interaction_ui.rs:584](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L584)) —
it doesn't consult the per-model gltf at all.

### Fix options (recommend A)

- **A. Resolve against the shared graph, not the per-model gltf** *(root fix)*.
  In `play_animations_system` replace the
  `gltf.named_animations.contains_key(desired)` check with
  `anims.nodes.contains_key(&desired)` (the graph is uniform across models, so the
  per-model gltf check is the wrong gate). This fixes T-pose for **every** shared
  clip, not just seated passengers. Verify it doesn't regress models that
  genuinely lack a clip — but since play already indexes `anims.nodes`, any clip
  in the graph is playable on any model.
- **B. Drive passengers like the driver mesh** *(local fix)*. Mark passenger
  models `ManualAnimation` and add a small per-frame system using `node_for(&anims,
  &["Sitting_Idle_Loop", "Sitting_Enter", "Idle_Loop"])`, mirroring
  `drive_driver_mesh_animation`. More code, but keeps the shared resolver
  untouched.
- **C. Cheap mitigation** — make `ai_animation` re-emit the seated clip every N
  seconds (or drop the "only on change" guard for `car_passenger`). Papers over
  the fallback race but does **not** fix a genuinely missing per-model clip, so it
  won't help if option-A's diagnosis (per-model gltf gate) is the true cause.

Recommend **A**; it's the smallest change that addresses the actual gate, and it
also hardens every other AI/idle clip against the same T-pose fallback.

## Test

`/run`, from the freecam menu spawn a car with a full set of passengers (see the
passenger list built in
[interaction_ui.rs:108-118](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/interaction_ui.rs#L108-L118) /
[click_spawn_select_controls.rs:53-63](crack_demo/demo_resolution_selector_web_bevy/src/plugins/cars_driving/click_spawn_select_controls.rs#L53-L63)):
- Every seat shows the seated idle pose — **no T-poses**, even across repeated
  spawns that roll different random pedestrian models.
- Driver + front-passenger heads sit inside the cabin, not through the windshield;
  rear passengers aren't clipping the rear glass. Adjust the ±0.5 Z sign if needed.

use "unset ARGV0" to escape the cursor unknwon proxy name problem with the appimage issue and check the code builds using "cargo check --bin ... --package ..."