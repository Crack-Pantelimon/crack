# Weapon idle orientation — plan

**Sub-problem:** When the character is in a non-aiming, non-shooting animation
(idle, crouch, walk, jog, sprint, …) the held weapon should point **forward-and-up**:
the normalized average of the character's animation *forward* (running direction)
and world *up*, expressed in **global** space (i.e. ~45° toward the sky along the
facing direction).

---

## Where it is today

All weapon orientation lives in `update_weapon_transforms`
([weapon_attach.rs:405-505](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L405-L505)).
The gun branch has three cases:

1. **Remote player** — aims along the avatar's facing: `aim_dir = root_rot * Vec3::Z`
   ([weapon_attach.rs:451-456](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L451-L456)).
2. **Local, aiming** (`rig.aiming || in_combat`) — aims at the crosshair raycast target
   ([weapon_attach.rs:458-475](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L458-L475)).
3. **Local, idle** — `transform.rotation = Quat::IDENTITY`, i.e. the barrel just
   inherits the wrist bone orientation
   ([weapon_attach.rs:476-479](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L476-L479)).

Case 3 is what we are replacing. The whole "point the weapon" math already exists
at the bottom of the gun branch
([weapon_attach.rs:482-501](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L482-L501)):
given a world-space `aim_dir`, it builds a basis (`x_axis = dir`, `y_axis` up-ish,
`z_axis` sideways) and converts it into the wrist-local rotation:
`transform.rotation = wrist_rot.inverse() * target_world_rot`.

So the only new work is **computing an `aim_dir` for the idle case** and feeding it
through that same block instead of forcing `Quat::IDENTITY`.

## The forward vector

"Character animation forward (the running direction)" = the character's facing yaw.
The remote branch already uses `root_rot * Vec3::Z` as facing, and the controller's
forward axis is `+Z` (`MODEL_FORWARD_OFFSET = 0.0`,
[mod.rs:143](crack_demo/demo_resolution_selector_web_bevy/src/plugins/pedestrians/pedestrian_controller_plugin/mod.rs#L143);
`face_movement`/`face_aim` orient the body's +Z toward movement/aim).

For the **local** player we need the controlled character's world rotation. It is
reachable via the `ControlledCharacter` resource (already a `Res` param here):
`controlled.controller` is the controller entity; query its `GlobalTransform`
(add `Query<&GlobalTransform>`-style access — `global_transforms` is already a param)
and take `rot * Vec3::Z`.

For the **remote** branch we already have `root_rot`; the same "forward + up" rule
should apply there too so held-but-not-aiming remote avatars match.

## The idle aim direction

```
let forward = (char_rot * Vec3::Z).normalize_or_zero();   // running/facing dir, planar+
let idle_dir = (forward + Vec3::Y).normalize_or_zero();   // 45° up average of fwd & world-up
```

Because `forward` is horizontal and `Vec3::Y` is unit-up, their normalized sum sits
at exactly 45° above the horizon along the facing direction — which is the spec.
(Guard the zero case: if `forward` is ~zero, fall back to `Vec3::Y` or keep the
previous frame's rotation.)

## Implementation steps

1. In `update_weapon_transforms`, resolve `char_rot` for both the remote and local
   paths (remote: existing `root_rot`; local: `GlobalTransform` of
   `controlled.controller`, decomposed to rotation).
2. Replace the `else { transform.rotation = Quat::IDENTITY; }` idle branch
   ([weapon_attach.rs:476-479](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L476-L479))
   with `aim_dir = Some((char_forward + Vec3::Y).normalize_or_zero())`.
3. For the **remote** branch, also switch from pure `root_rot * Vec3::Z` to the
   `forward + up` average **only when the remote avatar is not aiming/shooting**.
   Remote aim state isn't tracked here today; simplest first cut is to apply the
   45°-up rule unconditionally for remotes (they have no crosshair anyway), and
   revisit if remote shooting poses look wrong.
4. Let the shared basis block at
   [weapon_attach.rs:482-501](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L482-L501)
   convert `aim_dir` → wrist-local rotation. No change needed there.

## Edge cases / gotchas

- **Basis degeneracy:** the existing code guards `x_axis.cross(Vec3::Y)` going
  near-zero by falling back to `x_axis.cross(Vec3::Z)`
  ([weapon_attach.rs:488-491](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L488-L491)).
  Our idle `aim_dir` is 45° up so it never aligns with `Vec3::Y`; the guard stays
  fine but keep it.
- **Melee** is untouched — it keeps the fixed `Quat::from_rotation_x(90°)`
  ([weapon_attach.rs:433-435](crack_demo/demo_resolution_selector_web_bevy/src/plugins/weapons/weapon_attach.rs#L433-L435)).
- **Transition pop:** switching from IDENTITY-inherit to a fixed 45° global pose may
  visibly snap when the player stops aiming. If it reads badly, smooth
  `transform.rotation` toward the target with a `Quat::slerp` per frame (dt-scaled),
  but try the hard set first — the wrist already moves under animation so it may hide it.
- The rule must be **global**, not wrist-relative: we compute a world-space
  `aim_dir` and let the existing `wrist_rot.inverse() * target_world_rot` step
  cancel out the animated forearm swing, so the barrel holds the 45°-up line even
  as the arm bobs during the run cycle. That is exactly what the aiming path
  already does, so behavior is consistent.

## Test

Run the app (`/run`), spawn a controlled ped with a gun, and:
- Stand idle → barrel points forward+up ~45°.
- Walk/jog/sprint in a circle → barrel tracks the facing direction, still 45° up,
  steady despite the arm-swing animation.
- Hold RMB → snaps to crosshair aim (unchanged). Release → returns to 45° up.


use "unset ARGV0" to escape the cursor unknwon proxy name problem with the appimage issue and check the code builds using "cargo check --bin ... --package ..."