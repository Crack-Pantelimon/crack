Make a detailed technical plan and write it into _slop/plan_vfx_shader_v1.md for the following features:

- car explosion effect cosmetic
  - when car explodes, flash a couple sphere gizmos of different colors red through orange into yellow with the different damage radius
  - implement a light-weight smoke explosion shader in our app
  - implemenet a light-weight fireball explosion shader 
  - another very lightweight black smoke - not exploding but emanating from the object e.g. exploded car that smokes but isnt aflame anymore.

- pistol shot shader effects:
  - keep the gizmos but play them at max alppha = 0.3. 
  - add light-weight shader for bullet tracer (different than gizmo in its quality)
  - add bullet hit shader with more sparks to replace the gizmo physics based system
  - add bullet fire sparks in another very simple very light shader for the gun
  - add smoke when firing the gun

- add shader-based clouds to the map, this should also be very cheap on the gpu
- all these different effect catgories can be switched on/off (default on) from a new debug window called "VFX Shaders Controls". These should also have some simple sliders to control the various particle shaders we implement. Do not use the "hanabi" library because that doesn't work under web opengl based graphics that we target , as it requires compute shaders.
- all new code should go in a new system VisualFXPlugin that will listen for all the events and have the UI there . Mention the UI should run in the EguiPrimaryContextPass system not in update otherwise we will crash. 

Write code samples, research online reference each shader type we will be implementing, review each performance impact and expected gpu work, and explain why that is the right approach. We want the lightest possible graphics workload so we can work in browser on weak machines.

we are using bevy 0.19 look up custom shader examples online. 