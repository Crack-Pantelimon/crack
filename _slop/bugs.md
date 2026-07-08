- when in pedestrian control and in car control mode, we capture the mouse. when we are in the mode that we must capture the mouse, we try to recenter the mouse also to the center of the screen (to avoid runoff) and the camera is controlled by mouse without having to click-drag on the camera. In any other modes remaining (freecam mode, the debug scene modes) we do not capture the camera. Even in these modes, when the camera is captured, pressing escape will release it. Clicking on a game area (outside of the menu and uis) will capture it again. 

- 3d audio left and right is wrong, switch them around

- ui: chat window and other ui elements do not block keyboard actions from happening in the main game. all keyboard and mouse events should check we are not inside a ui window. 

- death animation for player doesn't play, their thing is just despawned. we should spawn another pedestrian glb there of the same type in the same transform as the dead one we despawned but with the death animation on it, with no looping, and keep the mesh in the last animation position (dead on the ground) for 10 more seconds and only then despawn this death prop. 

- melee attacks don't work online - flash on screen for 0.1s a yellow gizmo showing the attack area of the melee attack (a high cube of 1x1x2m in front of the character's hips that goes from under their feet to above their head) and do this for all types of pedestrians: player controlled, traffic controlled, multiplayer controlled.  and then to register melee hit, get physics intersection between the collider for that cube in that position, and the enemy we could hit (use the spatial query feature to get only things that we can hit - pedestrians and cars -  at most 2 cube sides away from the cube center)

- traffic pedestrian ai - with melee, it wants to climb on top of the thing it wants to hit (pedestrian or car) instead of hitting at a comfortable distance. When the target is near, it should aim at the center of the target and start hitting with melee weapon until target is dead, not climb on top of it. 

- use SQL api in the bevy app to save and load the random UserSecretKey and pass that to all our network managers. multiple tabs will use differnet NodeSecretKey randomly every time, but the same UserSecretKey will be used for all their sessions. 
