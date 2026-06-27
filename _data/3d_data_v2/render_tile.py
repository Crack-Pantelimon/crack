import bpy
import sys
import math
import numpy as np

def render_glb(glb_path, out_jpg_path):
    # Reset scene
    bpy.ops.wm.read_factory_settings(use_empty=True)
    
    # Import GLB
    try:
        bpy.ops.import_scene.gltf(filepath=glb_path)
    except Exception as e:
        print(f"Failed to import {glb_path}: {e}")
        return
    
    # Find all meshes
    meshes = [obj for obj in bpy.context.scene.objects if obj.type == 'MESH']
    if not meshes:
        print("No meshes found in GLB")
        return
        
    # Calculate bounding box of all meshes
    bbox_min = np.array([float('inf'), float('inf'), float('inf')])
    bbox_max = np.array([float('-inf'), float('-inf'), float('-inf')])
    for obj in meshes:
        matrix = obj.matrix_world
        for vertex in obj.data.vertices:
            world_pos = matrix @ vertex.co
            bbox_min = np.minimum(bbox_min, world_pos)
            bbox_max = np.maximum(bbox_max, world_pos)
            
    center = (bbox_min + bbox_max) / 2.0
    size = bbox_max - bbox_min
    max_dim = max(size)
    if max_dim == 0:
        max_dim = 1.0
        
    print(f"Bounding Box: min={bbox_min}, max={bbox_max}, center={center}, size={size}")
    
    # Calculate diagonal, far plane, near plane
    diagonal = np.linalg.norm(bbox_max - bbox_min)
    if diagonal == 0:
        diagonal = 1.0
    far_plane = 3.0 * diagonal
    near_plane = 0.01 * far_plane

    # Place camera pointing at the center
    cam_data = bpy.data.cameras.new(name="Camera")
    cam_data.clip_start = near_plane
    cam_data.clip_end = far_plane
    
    cam_object = bpy.data.objects.new(name="Camera", object_data=cam_data)
    bpy.context.collection.objects.link(cam_object)
    bpy.context.scene.camera = cam_object
    
    # Distance from center based on max_dim
    fov = cam_data.angle
    distance = max_dim / (2.0 * math.tan(fov / 2.0)) * 1.5
    
    # Place camera directly above the mesh
    cam_object.location = (
        center[0],
        center[1],
        center[2] + distance
    )
    
    # Point camera to center
    direction = bpy.data.objects.new(name="Target", object_data=None)
    bpy.context.collection.objects.link(direction)
    direction.location = center
    
    track = cam_object.constraints.new(type='TRACK_TO')
    track.target = direction
    track.track_axis = 'TRACK_NEGATIVE_Z'
    track.up_axis = 'UP_Y'
    
    # Update scene to apply constraint
    bpy.context.view_layer.update()
    
    # Add a sun light pointing downwards
    light_data = bpy.data.lights.new(name="Light", type='SUN')
    light_data.energy = 3.0
    light_object = bpy.data.objects.new(name="Light", object_data=light_data)
    bpy.context.collection.objects.link(light_object)
    light_object.location = (center[0] + 10, center[1] + 10, center[2] + 20)
    
    # Set render engine to Cycles for headless CPU rendering
    bpy.context.scene.render.engine = 'CYCLES'
    bpy.context.scene.cycles.device = 'CPU'
    bpy.context.scene.cycles.samples = 16
    
    # Set background color to light gray
    if not bpy.context.scene.world:
        bpy.context.scene.world = bpy.data.worlds.new("World")
    bpy.context.scene.world.use_nodes = True
    bg_node = bpy.context.scene.world.node_tree.nodes.get("Background")
    if bg_node:
        bg_node.inputs['Color'].default_value = (0.8, 0.8, 0.8, 1.0)
        bg_node.inputs['Strength'].default_value = 1.0
        
    # Set render settings
    bpy.context.scene.render.image_settings.file_format = 'JPEG'
    bpy.context.scene.render.filepath = out_jpg_path
    bpy.context.scene.render.resolution_x = 512
    bpy.context.scene.render.resolution_y = 512
    
    # Render
    bpy.ops.render.render(write_still=True)
    print(f"Rendered {glb_path} to {out_jpg_path}")

if __name__ == "__main__":
    try:
        args_idx = sys.argv.index("--")
        args = sys.argv[args_idx + 1:]
    except ValueError:
        args = []
        
    if len(args) < 2:
        print("Usage: blender -b -P render_tile.py -- <glb_path> <out_jpg_path>")
        sys.exit(1)
        
    render_glb(args[0], args[1])
