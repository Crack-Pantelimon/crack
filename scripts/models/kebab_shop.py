import bpy
import math
import os
import argparse

# Helper to clear scene
def clear_scene():
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete()

# Helper to create texture material
def create_texture_material(name, img_path, color=None, metallic=0.0, roughness=0.5):
    mat = bpy.data.materials.new(name=name)
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    nodes.clear()
    
    output = nodes.new('ShaderNodeOutputMaterial')
    output.location = (400, 0)
    bsdf = nodes.new('ShaderNodeBsdfPrincipled')
    bsdf.location = (0, 0)
    bsdf.inputs['Metallic'].default_value = metallic
    bsdf.inputs['Roughness'].default_value = roughness
    
    if img_path and os.path.exists(img_path):
        tex_image = nodes.new('ShaderNodeTexImage')
        tex_image.location = (-400, 0)
        tex_image.image = bpy.data.images.load(img_path)
        
        mapping = nodes.new('ShaderNodeMapping')
        mapping.location = (-600, 0)
        
        tex_coord = nodes.new('ShaderNodeTexCoord')
        tex_coord.location = (-800, 0)
        
        links.new(tex_coord.outputs['UV'], mapping.inputs['Vector'])
        links.new(mapping.outputs['Vector'], tex_image.inputs['Vector'])
        links.new(tex_image.outputs['Color'], bsdf.inputs['Base Color'])
    elif color:
        bsdf.inputs['Base Color'].default_value = (color[0], color[1], color[2], 1.0)
        
    links.new(bsdf.outputs['BSDF'], output.inputs['Surface'])
    return mat

def set_viewport_to_material_shading():
    """Sets the active viewport shading mode to Material Preview for immediate visual feedback."""
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type == 'VIEW_3D':
                for space in area.spaces:
                    if space.type == 'VIEW_3D':
                        space.shading.type = 'MATERIAL'

def generate_kebab_shop(texture_path, output_dir):
    """
    Models the modern Socului Kebab shop interior, matching layout, wall colors, text, and stools.
    """
    print(f"[ASSET GENERATOR - KEBAB] Generating Kebab Shop using texture: {texture_path}")
    clear_scene()

    parts = []

    # 1. Floor (Light gray polished tile)
    bpy.ops.mesh.primitive_plane_add(size=8.0, location=(0, 0, 0))
    floor = bpy.context.active_object
    floor.name = "Floor"
    floor_mat = create_texture_material("Floor_Mat", None, color=[0.85, 0.85, 0.85], roughness=0.15)
    floor.data.materials.append(floor_mat)
    parts.append(floor)

    # 2. Back Wall (Red Accent Wall with "SOCULUI KEBAB" text)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0, 4.0, 1.5))
    back_wall = bpy.context.active_object
    back_wall.name = "Back_Wall"
    back_wall.scale = (8.0, 0.1, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    
    # Socului Red accent color
    red_wall_mat = create_texture_material("Red_Wall_Mat", None, color=[0.65, 0.05, 0.05], roughness=0.8)
    back_wall.data.materials.append(red_wall_mat)
    parts.append(back_wall)

    # Add 3D Text "SOCULUI KEBAB"
    white_text_mat = create_texture_material("White_Text_Mat", None, color=[1.0, 1.0, 1.0], roughness=0.5)
    
    # SOCULUI (Top line)
    bpy.ops.object.text_add(location=(-2.2, 3.85, 2.0))
    txt_socului = bpy.context.active_object
    txt_socului.name = "Text_Socului"
    txt_socului.data.body = "SOCULUI"
    txt_socului.data.extrude = 0.06
    txt_socului.scale = (0.55, 0.55, 0.55)
    txt_socului.rotation_euler = (math.radians(90), 0, 0)
    txt_socului.data.materials.append(white_text_mat)
    parts.append(txt_socului)

    # KEBAB (Bottom line)
    bpy.ops.object.text_add(location=(-1.6, 3.85, 1.3))
    txt_kebab = bpy.context.active_object
    txt_kebab.name = "Text_Kebab"
    txt_kebab.data.body = "KEBAB"
    txt_kebab.data.extrude = 0.06
    txt_kebab.scale = (0.45, 0.45, 0.45)
    txt_kebab.rotation_euler = (math.radians(90), 0, 0)
    txt_kebab.data.materials.append(white_text_mat)
    parts.append(txt_kebab)

    # 3. Off-white Side Walls
    off_white_mat = create_texture_material("Off_White_Wall_Mat", None, color=[0.9, 0.9, 0.88], roughness=0.9)
    
    # Left Wall (West)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(-4.0, 0, 1.5))
    left_wall = bpy.context.active_object
    left_wall.name = "Left_Wall"
    left_wall.scale = (0.1, 8.0, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    left_wall.data.materials.append(off_white_mat)
    parts.append(left_wall)

    # Right Wall (East)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(4.0, 0, 1.5))
    right_wall = bpy.context.active_object
    right_wall.name = "Right_Wall"
    right_wall.scale = (0.1, 8.0, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    right_wall.data.materials.append(off_white_mat)
    parts.append(right_wall)

    # 4. Wooden Bar Counter
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(-2.0, 1.5, 0.5))
    counter_base = bpy.context.active_object
    counter_base.name = "Counter_Base"
    counter_base.scale = (3.5, 0.8, 1.0)
    bpy.ops.object.transform_apply(scale=True)
    
    wood_mat = create_texture_material("Counter_Wood_Mat", None, color=[0.7, 0.45, 0.25], roughness=0.6)
    counter_base.data.materials.append(wood_mat)
    parts.append(counter_base)

    # Photo texture applied to the front panel facing customer dining area
    panel_mat = create_texture_material("Kebab_Photo_Mat", texture_path, roughness=0.5)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(-2.0, 1.09, 0.5))
    photo_panel = bpy.context.active_object
    photo_panel.name = "Photo_Panel"
    photo_panel.scale = (3.4, 0.01, 0.95)
    bpy.ops.object.transform_apply(scale=True)
    photo_panel.data.materials.append(panel_mat)
    
    # UV Map panel
    bpy.context.view_layer.objects.active = photo_panel
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project()
    bpy.ops.object.mode_set(mode='OBJECT')
    parts.append(photo_panel)

    # 5. Plant Shelf Grid (White grid structure behind the counter)
    plant_shelf_mat = create_texture_material("Plant_Shelf_Mat", None, color=[0.95, 0.95, 0.95], roughness=0.3)
    bpy.ops.mesh.primitive_grid_add(x_subdivisions=5, y_subdivisions=5, size=2.5, location=(-2.0, 2.2, 1.8))
    plant_grid = bpy.context.active_object
    plant_grid.name = "Plant_Grid"
    # Extrude it to a cage structure using Wireframe Modifier
    wire = plant_grid.modifiers.new(name="Wireframe", type='WIREFRAME')
    wire.thickness = 0.04
    plant_grid.data.materials.append(plant_shelf_mat)
    parts.append(plant_grid)

    # Green potted plants inside grid boxes
    green_mat = create_texture_material("Plant_Green_Mat", None, color=[0.1, 0.5, 0.15], roughness=0.8)
    for px, pz in [(-2.8, 1.2), (-2.0, 1.2), (-1.2, 1.2), (-2.5, 2.0), (-1.5, 2.0)]:
        bpy.ops.mesh.primitive_ico_sphere_add(radius=0.25, subdivisions=2, location=(px, 2.2, pz))
        plant = bpy.context.active_object
        plant.name = "Grid_Plant"
        plant.data.materials.append(green_mat)
        parts.append(plant)

    # 6. Bar Stools (Alternating Black and Red)
    chrome_mat = create_texture_material("Chrome_Mat", None, color=[0.8, 0.8, 0.8], metallic=1.0, roughness=0.15)
    red_stool_mat = create_texture_material("Red_Stool_Mat", None, color=[0.85, 0.05, 0.05], roughness=0.6)
    black_stool_mat = create_texture_material("Black_Stool_Mat", None, color=[0.05, 0.05, 0.05], roughness=0.7)

    # 5 Stools placed along the bar counter
    stool_x_coords = [-3.2, -2.6, -2.0, -1.4, -0.8]
    for idx, sx in enumerate(stool_x_coords):
        # Base plate
        bpy.ops.mesh.primitive_cylinder_add(radius=0.25, depth=0.03, location=(sx, 0.5, 0.015))
        base = bpy.context.active_object
        base.name = f"Stool_Base_{idx}"
        base.data.materials.append(chrome_mat)
        parts.append(base)

        # Pole
        bpy.ops.mesh.primitive_cylinder_add(radius=0.04, depth=0.6, location=(sx, 0.5, 0.3))
        pole = bpy.context.active_object
        pole.name = f"Stool_Pole_{idx}"
        pole.data.materials.append(chrome_mat)
        parts.append(pole)

        # Seat (alternating black and red)
        bpy.ops.mesh.primitive_cylinder_add(radius=0.2, depth=0.08, location=(sx, 0.5, 0.6))
        seat = bpy.context.active_object
        seat.name = f"Stool_Seat_{idx}"
        
        active_stool_mat = red_stool_mat if idx % 2 == 1 else black_stool_mat
        seat.data.materials.append(active_stool_mat)
        parts.append(seat)

    # 7. Customer Tables (Round pedestal) & Web-like/Lattice Dining Chairs
    table_white_mat = create_texture_material("Table_White_Mat", None, color=[0.92, 0.92, 0.92], roughness=0.2)
    chair_white_mat = create_texture_material("Chair_White_Mat", None, color=[0.9, 0.9, 0.9], roughness=0.5)
    chair_red_mat = create_texture_material("Chair_Red_Mat", None, color=[0.8, 0.05, 0.05], roughness=0.5)

    # Dining set 1
    t1_pos = (2.0, -2.0)
    # Round Table
    bpy.ops.mesh.primitive_cylinder_add(radius=0.6, depth=0.05, location=(t1_pos[0], t1_pos[1], 0.75))
    tab1 = bpy.context.active_object
    tab1.data.materials.append(table_white_mat)
    parts.append(tab1)
    # Table leg
    bpy.ops.mesh.primitive_cylinder_add(radius=0.06, depth=0.7, location=(t1_pos[0], t1_pos[1], 0.35))
    tab1_leg = bpy.context.active_object
    tab1_leg.data.materials.append(chrome_mat)
    parts.append(tab1_leg)

    # Web/Lattice Chairs
    for idx, c_offset in enumerate([(-0.8, 0.0), (0.8, 0.0)]):
        cx = t1_pos[0] + c_offset[0]
        cy = t1_pos[1] + c_offset[1]
        
        # Grid lattice seat using wireframe modifier
        bpy.ops.mesh.primitive_grid_add(x_subdivisions=5, y_subdivisions=5, size=0.4, location=(cx, cy, 0.45))
        c_seat = bpy.context.active_object
        c_seat.name = f"Lattice_Chair_Seat_{idx}"
        wire_seat = c_seat.modifiers.new(name="WireframeSeat", type='WIREFRAME')
        wire_seat.thickness = 0.025
        
        # Alternate white and red chairs
        c_mat = chair_white_mat if idx % 2 == 0 else chair_red_mat
        c_seat.data.materials.append(c_mat)
        parts.append(c_seat)

        # Backrest (extrude a vertical lattice backrest)
        bpy.ops.mesh.primitive_grid_add(x_subdivisions=5, y_subdivisions=5, size=0.4, location=(cx + c_offset[0]*0.2, cy, 0.75), rotation=(0, math.radians(90), 0))
        c_back = bpy.context.active_object
        c_back.name = f"Lattice_Chair_Back_{idx}"
        wire_back = c_back.modifiers.new(name="WireframeBack", type='WIREFRAME')
        wire_back.thickness = 0.025
        c_back.data.materials.append(c_mat)
        parts.append(c_back)

        # Chair legs
        for lx, ly in [(-0.15, -0.15), (0.15, -0.15), (-0.15, 0.15), (0.15, 0.15)]:
            bpy.ops.mesh.primitive_cylinder_add(radius=0.02, depth=0.4, location=(cx + lx, cy + ly, 0.2))
            c_leg = bpy.context.active_object
            c_leg.data.materials.append(chrome_mat)
            parts.append(c_leg)

    # 8. Juice Dispenser (on left side of wooden counter)
    jd_x, jd_y, jd_z = (-3.2, 1.4, 1.0)
    # Silver Base box
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(jd_x, jd_y, jd_z + 0.15))
    jd_base = bpy.context.active_object
    jd_base.name = "Juice_Dispenser_Base"
    jd_base.scale = (0.4, 0.4, 0.3)
    bpy.ops.object.transform_apply(scale=True)
    jd_base.data.materials.append(chrome_mat)
    parts.append(jd_base)

    # Yellow juice tank (translucent/emissive yellow look)
    yellow_juice_mat = create_texture_material("Juice_Yellow_Mat", None, color=[0.95, 0.75, 0.0], roughness=0.3)
    bpy.ops.mesh.primitive_cylinder_add(radius=0.15, depth=0.4, location=(jd_x, jd_y, jd_z + 0.5))
    jd_tank = bpy.context.active_object
    jd_tank.name = "Juice_Tank"
    jd_tank.data.materials.append(yellow_juice_mat)
    parts.append(jd_tank)

    # Dispenser lid (chrome cap)
    bpy.ops.mesh.primitive_cylinder_add(radius=0.16, depth=0.03, location=(jd_x, jd_y, jd_z + 0.71))
    jd_lid = bpy.context.active_object
    jd_lid.name = "Juice_Lid"
    jd_lid.data.materials.append(chrome_mat)
    parts.append(jd_lid)

    # 9. Join all kebab shop parts
    bpy.ops.object.select_all(action='DESELECT')
    main_shop = parts[0]
    main_shop.select_set(True)
    for p in parts[1:]:
        p.select_set(True)
    bpy.context.view_layer.objects.active = main_shop
    bpy.ops.object.join()
    main_shop.name = "Kebab_Shop"

    # Set viewport shading to MATERIAL and pack textures
    set_viewport_to_material_shading()
    try:
        bpy.ops.file.pack_all()
    except Exception as e:
        print(f"Warning: Could not pack textures: {e}")

    # 10. Export GLB and save .blend project file
    os.makedirs(output_dir, exist_ok=True)
    export_path_glb = os.path.join(output_dir, "kebab_shop.glb")
    export_path_blend = os.path.join(output_dir, "kebab_shop.blend")
    
    bpy.ops.export_scene.gltf(filepath=export_path_glb, export_format='GLB')
    bpy.ops.wm.save_as_mainfile(filepath=export_path_blend)
    print(f"[ASSET GENERATOR - KEBAB] Kebab Shop exported successfully to {export_path_glb} and saved to {export_path_blend}")


def main():
    import sys
    argv = sys.argv
    if "--" in argv:
        args_to_parse = argv[argv.index("--") + 1:]
    else:
        args_to_parse = []

    parser = argparse.ArgumentParser(description="Kebab Shop asset generator")
    parser.add_argument("--output-dir", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/_data/blender_generated/kebab_shop", help="Target output directory")
    parser.add_argument("--kebab-texture", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/images/IMG-20170701-WA0024.webp", help="Path to kebab shop texture image")
    
    args = parser.parse_args(args_to_parse)
    generate_kebab_shop(args.kebab_texture, args.output_dir)

if __name__ == "__main__":
    main()
