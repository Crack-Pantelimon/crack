import bpy
import os
import mathutils

# Global Constant for the export directory
# NOTE: Windows users should use forward slashes (e.g., "C:/Users/Name/Desktop/Export")
EXPORT_DIR = "/path/to/your/export/directory"

def export_meshes_to_glb():
    # Ensure the export directory exists
    if not os.path.exists(EXPORT_DIR):
        os.makedirs(EXPORT_DIR)
        print(f"Created directory: {EXPORT_DIR}")

    # Deselect all objects to start fresh
    bpy.ops.object.select_all(action='DESELECT')

    # Filter for mesh objects only
    mesh_objects = [obj for obj in bpy.data.objects if obj.type == 'MESH']

    for obj in mesh_objects:
        # 1. Save original transform state to restore it later
        original_matrix = obj.matrix_world.copy()

        # 2. Select and make active
        obj.select_set(True)
        bpy.context.view_layer.objects.active = obj

        # 3. Move origin to Center of Gravity (Surface/Volume weights)
        # In Blender, "Center of Mass" is the closest native approximation to CG
        bpy.ops.object.origin_set(type='ORIGIN_CENTER_OF_VOLUME', center='MEDIAN')

        # 4. Snap the object to the world center (0, 0, 0)
        obj.location = (0.0, 0.0, 0.0)

        # Force a scene update to ensure transforms apply before export
        bpy.context.view_layer.update()

        # 5. Define export file path
        # Clean the object name for file system compatibility
        safe_name = "".join([c for c in obj.name if c.isalpha() or c.isdigit() or c in ' ._-']).rstrip()
        file_path = os.path.join(EXPORT_DIR, f"{safe_name}.glb")

        # 6. Export to GLTF/GLB (using the modern Blender 4.x/5.x API syntax)
        print(f"Exporting: {obj.name} -> {file_path}")
        bpy.ops.export_scene.gltf(
            filepath=file_path,
            export_format='GLB',
            use_selection=True,  # Crucial: only exports the active, recentered mesh
            export_apply=True     # Applies modifiers if you have any
        )

        # 7. Restore original position and deselect for the next loop
        obj.matrix_world = original_matrix
        obj.select_set(False)

    print("--- Export Complete! ---")

# Run the script
if __name__ == "__main__":
    export_meshes_to_glb()
