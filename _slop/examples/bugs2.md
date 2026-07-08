- camera phases through objects
  - can't look up because the ground renders in front
  - can't go through tight spaces
  - fix: run a ray trace from the player backwards towards where we want to put the camera, and go actually 90% of the way to the first collision, and put the camera there. 


- when someone talks in global chat, show a chat bubble with their name and the chat message text (max first 70 characters truncated with ... if longer) for 3s above their character.
- main game: show our own hp bar and character name as we show for the other players.
- car physics: car can never stay still (have speed of zero), it's always 1km/h either direction. need to add some logic to make it detect that it's currently slow and zero out its forces and put it in "park" mode, maybe even by putting some kind of support underneath it and putting it to sleep. 
