import os
import sys
import json
import bpy

# ==============================================================================
# CONFIGURATION
# Set the target folder path where GLB and JSON files will be exported.
# ==============================================================================
EXPORT_DIR = "/home/p/VIDOEGAME/crack/exported_objects"

def check_export_directory(path):
    """Checks if the export directory is empty. Exits the script if it is not."""
    if os.path.exists(path):
        if os.listdir(path):
            print(f"Error: Target export directory '{path}' is not empty. Exiting.")
            sys.exit(1)
    else:
        try:
            os.makedirs(path, exist_ok=True)
            print(f"Created export directory: {path}")
        except Exception as e:
            print(f"Error: Could not create export directory '{path}': {e}")
            sys.exit(1)

def sanitize_filename(name):
    """Sanitizes an object's name to make it a valid filename across OS platforms."""
    # Replace common illegal characters
    for c in ['/', '\\', ':', '*', '?', '"', '<', '>', '|']:
        name = name.replace(c, '_')
    return name

def get_object_world_vertices(obj):
    """
    Evaluates the object's mesh (including modifiers) and transforms all
    vertex coordinates to world space.
    """
    vertices = []
    
    # Check if the object can have mesh/geometric data
    if obj.type in {'MESH', 'CURVE', 'SURFACE', 'META', 'FONT'}:
        try:
            # Use dependency graph to get the evaluated object (with modifiers applied)
            depsgraph = bpy.context.evaluated_depsgraph_get()
            obj_eval = obj.evaluated_get(depsgraph)
            
            # Create a temporary mesh from the evaluated object
            mesh = obj_eval.to_mesh()
            if mesh:
                matrix_world = obj.matrix_world
                # Transform each vertex local coordinate to world space
                vertices = [matrix_world @ v.co for v in mesh.vertices]
                # Clear/free the temporary mesh from memory
                obj_eval.to_mesh_clear()
        except Exception as e:
            print(f"Warning: Could not get evaluated mesh for object '{obj.name}': {e}")
            # Fallback: get raw mesh vertices if it's a mesh type
            if obj.type == 'MESH' and obj.data:
                matrix_world = obj.matrix_world
                vertices = [matrix_world @ v.co for v in obj.data.vertices]
                
    return vertices

def run_export():
    print("Starting export script...")
    
    # 1. Ensure target directory exists and is empty
    check_export_directory(EXPORT_DIR)
    
    # Save the original active object and selected objects to restore them later
    original_active = bpy.context.view_layer.objects.active
    original_selection = [obj for obj in bpy.context.scene.objects if obj.select_get()]
    
    # Iterate over all objects in the scene
    all_objects = list(bpy.context.scene.objects)
    print(f"Found {len(all_objects)} objects to process.")
    
    # List to accumulate all object metadata
    exported_objects_metadata = []
    
    try:
        for obj in all_objects:
            # Generate filenames
            safe_name = sanitize_filename(obj.name)
            glb_filename = f"{safe_name}.glb"
            glb_path = os.path.join(EXPORT_DIR, glb_filename)
            
            # Save original visibility & selectability states
            orig_hide_viewport = obj.hide_viewport
            orig_hide_select = obj.hide_select
            
            try:
                # Temporarily make the object visible/selectable so we can select and export it
                obj.hide_viewport = False
                obj.hide_select = False
                
                # Deselect all and select only this object
                bpy.ops.object.select_all(action='DESELECT')
                obj.select_set(True)
                bpy.context.view_layer.objects.active = obj
                
                # Export the selected object as GLB
                print(f"Exporting object '{obj.name}' to '{glb_path}'...")
                bpy.ops.export_scene.gltf(
                    filepath=glb_path,
                    export_format='GLB',
                    use_selection=True,
                    export_materials='EXPORT',
                    export_image_format='JPEG'
                )
            except Exception as e:
                print(f"Error exporting GLB for object '{obj.name}': {e}")
                continue
            finally:
                # Restore original visibility/selectability states
                obj.hide_viewport = orig_hide_viewport
                obj.hide_select = orig_hide_select
            
            # Compute world vertices
            world_vertices = get_object_world_vertices(obj)
            vertex_count = len(world_vertices)
            
            # Calculate axis-aligned bounding box (AABB) in world space
            if world_vertices:
                min_x = min(v.x for v in world_vertices)
                max_x = max(v.x for v in world_vertices)
                min_y = min(v.y for v in world_vertices)
                max_y = max(v.y for v in world_vertices)
                min_z = min(v.z for v in world_vertices)
                max_z = max(v.z for v in world_vertices)
            else:
                # Default values if no mesh vertices are present (e.g. Empty, Camera, Light)
                min_x = max_x = min_y = max_y = min_z = max_z = 0.0
                
            aabb = {
                "minx": min_x,
                "maxx": max_x,
                "miny": min_y,
                "maxy": max_y,
                "minz": min_z,
                "maxz": max_z
            }
            
            # Create metadata JSON structure for the current object
            metadata = {
                "name": obj.name,
                "filename": glb_filename,
                "vertex_count": vertex_count,
                "aabb": aabb
            }
            exported_objects_metadata.append(metadata)
            
        # Write the single consolidated list JSON file
        list_json_path = os.path.join(EXPORT_DIR, "_list.json")
        print(f"Writing consolidated object metadata to '{list_json_path}'...")
        try:
            with open(list_json_path, 'w', encoding='utf-8') as f:
                json.dump(exported_objects_metadata, f, indent=4)
        except Exception as e:
            print(f"Error writing metadata list to file: {e}")
                
    finally:
        # Restore original selection and active object state
        bpy.ops.object.select_all(action='DESELECT')
        for obj in original_selection:
            try:
                obj.select_set(True)
            except Exception:
                pass
        bpy.context.view_layer.objects.active = original_active
        print("Export process finished and original selection restored.")

if __name__ == "__main__":
    run_export()
