import bpy
import sys

bpy.ops.import_scene.gltf(filepath="_data/3d_data/3d_slop_models_clean/pedestrian/armin-1b.glb")
anims = []
for action in bpy.data.actions:
    anims.append(action.name)
print("ANIMATIONS_FOUND: " + str(anims))
