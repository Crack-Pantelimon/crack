- bvh does not actually cull enough. i put the character in a hole and it doesn't cull anything. only when we take a gun out and zoom in do we see anything culled in the minimap. putting the character in a hole does not help culling either. let's change our model such that instead of computing visibility from a sphere around the reference points, we will just use the points themselves instead, unchanged, as-is. as the only reference point right now is the camera, we will only be computing visibility with a sphere of radius 0.01 (instead of it being a single small point, to keep a hatch so that we may possibly go back to a sphere based model). The radius might have also shown a problem: if we have a large radius to check from, some of that will land under the ground. Think of a fix to implement this change and plan out the technical fixes in `_slop/fix_bvh_plan.md`


animations
- when in non-aiming and non-shooting animation e.g. idle, crouching, sprinting, etc. the weapon should always be facing towards the front of the character's animation but 45 degrees towards the sky, so you can average the UP vector and the character animation forward vector (the running direction) and normalize that and point the weapon in global space in that direction.  

- pedestrian: both walking   (moving without shift) and sprinting should accelerate. walking should show the walking animation for 0.5 of its acceleration and the jogging animation for the other half of its acceleration space. sprinting does the same: starts from the jog speed and accelerates towards sprint speed. the soudn for the walking is too fast, sounds about 3x faster than the walking speed. the running sound speed is about 10% too fast - lower that too. Review all the running/walking animation code and centralize it under one set of constants in the correct controller code.

- car spawning: spawning a car from the menu shows some of the characters inside in a t-pose isntead of the sitting animation. all characters inside a car need to move bash (so positive Z in the space of the car) about 0.5m - they now sit too in front and the front passenger and dirver's heads are floating out the windshield


# FEATURES

- when you die, respawn at hospital
- spawn traffic with randomly 1-4 ppl in car