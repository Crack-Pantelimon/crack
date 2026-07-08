- in main game, we have a Option > Sound > volume slider, but it doesnt actually change the sound at all. this is because it doesn't affect 3d sounds, i think. need to investigate and fix that 

- "car park" mode is busted, the car falls to the ground and gets up repeatedly. we need to spawn invisible "car park support" cube by shooting some rays from under the car at the corners and figuring out the distance, then parenting some invisible cubes at those points to support the car while sleeping. if the car is moving faster than 1km/h the cubes despawn. if the car despawns, the cubes despawn as well. 


- weapon switcher on mouse wheel should also have a debounce of 0.15s so when the user spams it it doesn't go so fast.

- gun reload animation should block further gun firing until it's finished. each gun's reload animation length should be added to the gun manifest by hand, pick some values. the melee weapons all have reload animation length of 1s. speed up or slow down the actual reload animation to match the reload of the gun. some actions intetrupt reloading, such as switching weapons, climbing, sprinting, etc. When these actions are received on the pedestrian, reloading stops.

- research if pedestrians and cars have code that's implemented multiple times: they can be npcs driven by some ai, player characters, or network player characters. we should make sure all pedestrian related code is in some common plugin for pedestrian behavior, and all these simulated pedestrians and cars are driven only by their events that they observe: player input, network player event or update message, npc ai decision. this means we should review all traffic plugins, car plugins, etc. for duplication and refactor the code in such a way to decouple the simulated entities (cars, pedestrians, weapons) from the methods they are driven (pc, npc, network pc)


