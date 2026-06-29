import bpy
import math
import os
import argparse

def clear_scene():
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete()

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
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type == 'VIEW_3D':
                for space in area.spaces:
                    if space.type == 'VIEW_3D':
                        space.shading.type = 'MATERIAL'

def generate_superbet_shop(texture_path, output_dir):
    print(f"[ASSET GENERATOR - SUPERBET] Generating Superbet Shop using texture: {texture_path}")
    clear_scene()

    parts = []

    # 1. Floor (Polished gray tiles)
    bpy.ops.mesh.primitive_plane_add(size=8.0, location=(0, 0, 0))
    floor = bpy.context.active_object
    floor.name = "Floor"
    floor_mat = create_texture_material("Floor_Mat", None, color=[0.85, 0.85, 0.85], roughness=0.2)
    floor.data.materials.append(floor_mat)
    parts.append(floor)

    # 2. Main Wall (North - Back Wall)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0, 4.0, 1.5))
    back_wall = bpy.context.active_object
    back_wall.name = "Back_Wall"
    back_wall.scale = (8.0, 0.1, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    
    wall_white_mat = create_texture_material("Wall_White_Mat", None, color=[0.92, 0.92, 0.92], roughness=0.9)
    back_wall.data.materials.append(wall_white_mat)
    parts.append(back_wall)

    # Left Wall
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(-4.0, 0, 1.5))
    left_wall = bpy.context.active_object
    left_wall.scale = (0.1, 8.0, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    left_wall.data.materials.append(wall_white_mat)
    parts.append(left_wall)

    # Right Wall (with red accent behind cashier counter)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(4.0, 0, 1.5))
    right_wall = bpy.context.active_object
    right_wall.scale = (0.1, 8.0, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    
    right_wall_mat = create_texture_material("Right_Wall_Red_Mat", None, color=[0.8, 0.05, 0.05], roughness=0.7)
    right_wall.data.materials.append(right_wall_mat)
    parts.append(right_wall)

    # 3. Superbet Sign (Red sign recess + White 3D Text)
    # Red background sign board
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(-1.0, 3.92, 1.6))
    sign_board = bpy.context.active_object
    sign_board.name = "Sign_Board"
    sign_board.scale = (2.2, 0.05, 0.9)
    bpy.ops.object.transform_apply(scale=True)
    superbet_red_mat = create_texture_material("Superbet_Red_Mat", None, color=[0.85, 0.05, 0.05], roughness=0.6)
    sign_board.data.materials.append(superbet_red_mat)
    parts.append(sign_board)

    # White 3D text "superbet"
    white_txt_mat = create_texture_material("Sign_White_Text_Mat", None, color=[1.0, 1.0, 1.0], roughness=0.4)
    bpy.ops.object.text_add(location=(-1.8, 3.88, 1.4))
    txt_obj = bpy.context.active_object
    txt_obj.name = "Text_Superbet"
    txt_obj.data.body = "superbet"
    txt_obj.data.extrude = 0.05
    txt_obj.scale = (0.42, 0.42, 0.42)
    txt_obj.rotation_euler = (math.radians(90), 0, 0)
    txt_obj.data.materials.append(white_txt_mat)
    parts.append(txt_obj)

    # 4. Cashier Counter & Desk (Right Foreground)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(2.0, 1.5, 0.5))
    counter = bpy.context.active_object
    counter.name = "Cashier_Counter"
    counter.scale = (3.0, 0.8, 1.0)
    bpy.ops.object.transform_apply(scale=True)
    
    counter_gray_mat = create_texture_material("Counter_Gray_Mat", None, color=[0.88, 0.88, 0.88], roughness=0.4)
    counter.data.materials.append(counter_gray_mat)
    parts.append(counter)

    # Transparent glass partition screen on counter
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(2.0, 1.5, 1.3))
    glass = bpy.context.active_object
    glass.name = "Glass_Partition"
    glass.scale = (2.8, 0.02, 0.6)
    bpy.ops.object.transform_apply(scale=True)
    
    glass_mat = create_texture_material("Glass_Mat", None, color=[0.9, 0.95, 0.95], roughness=0.1)
    # Give glass transparency
    glass_mat.blend_method = 'BLEND'
    glass_mat.node_tree.nodes['Principled BSDF'].inputs['Transmission Weight'].default_value = 0.95
    glass_mat.node_tree.nodes['Principled BSDF'].inputs['Alpha'].default_value = 0.4
    
    glass.data.materials.append(glass_mat)
    parts.append(glass)

    # 5. Hanging Odds TV Screens (2 screens above Cashier Counter)
    screentex_mat = create_texture_material("TV_Odds_Mat", texture_path, roughness=0.3)
    black_plastic_mat = create_texture_material("TV_Case_Mat", None, color=[0.05, 0.05, 0.05], roughness=0.7)

    for idx, sx in enumerate([1.0, 2.5]):
        # Screen frame
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=(sx, 1.5, 2.3))
        tv = bpy.context.active_object
        tv.name = f"TV_Hanging_{idx}"
        tv.scale = (1.0, 0.15, 0.6)
        bpy.ops.object.transform_apply(scale=True)
        tv.data.materials.append(black_plastic_mat)
        parts.append(tv)
        
        # Display surface showing photo odds
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=(sx, 1.42, 2.3))
        display = bpy.context.active_object
        display.name = f"TV_Display_{idx}"
        display.scale = (0.94, 0.01, 0.54)
        bpy.ops.object.transform_apply(scale=True)
        display.data.materials.append(screentex_mat)
        
        # UV Smart Project display
        bpy.context.view_layer.objects.active = display
        bpy.ops.object.mode_set(mode='EDIT')
        bpy.ops.mesh.select_all(action='SELECT')
        bpy.ops.uv.smart_project()
        bpy.ops.object.mode_set(mode='OBJECT')
        parts.append(display)

        # Hanging pole
        bpy.ops.mesh.primitive_cylinder_add(radius=0.03, depth=0.7, location=(sx, 1.5, 2.75))
        pole = bpy.context.active_object
        pole.data.materials.append(black_plastic_mat)
        parts.append(pole)

    # 6. Grid of 6 betting screens on the West (left) Wall
    for row in range(2):
        for col in range(3):
            wx = -3.92
            wy = -1.2 + col * 1.1
            wz = 1.3 + row * 0.8
            
            # Frame
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=(wx, wy, wz))
            tv_l = bpy.context.active_object
            tv_l.scale = (0.15, 0.9, 0.6)
            bpy.ops.object.transform_apply(scale=True)
            tv_l.data.materials.append(black_plastic_mat)
            parts.append(tv_l)
            
            # Screen face (uses photos)
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=(wx + 0.08, wy, wz))
            display_l = bpy.context.active_object
            display_l.scale = (0.01, 0.84, 0.54)
            bpy.ops.object.transform_apply(scale=True)
            display_l.data.materials.append(screentex_mat)
            parts.append(display_l)

    # 7. Customer Seating & Seating Tables
    wood_mat = create_texture_material("Table_Wood_Mat", None, color=[0.75, 0.6, 0.45], roughness=0.6)
    table_positions = [(-2.0, 1.5), (-2.0, -1.0), (0.0, -2.0)]
    
    for idx, tpos in enumerate(table_positions):
        # Table top
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=(tpos[0], tpos[1], 0.75))
        tt = bpy.context.active_object
        tt.scale = (0.8, 0.8, 0.04)
        bpy.ops.object.transform_apply(scale=True)
        tt.data.materials.append(wood_mat)
        parts.append(tt)

        # Central base leg
        bpy.ops.mesh.primitive_cylinder_add(radius=0.04, depth=0.7, location=(tpos[0], tpos[1], 0.38))
        leg = bpy.context.active_object
        leg.data.materials.append(black_plastic_mat)
        parts.append(leg)
        
        # Base plate
        bpy.ops.mesh.primitive_cylinder_add(radius=0.25, depth=0.03, location=(tpos[0], tpos[1], 0.015))
        bplate = bpy.context.active_object
        bplate.data.materials.append(black_plastic_mat)
        parts.append(bplate)

        # Chairs around each table (2 chairs per table, red backs, black seats)
        red_seat_mat = create_texture_material("Chair_Red_Mat", None, color=[0.8, 0.05, 0.05], roughness=0.6)
        black_seat_mat = create_texture_material("Chair_Black_Mat", None, color=[0.1, 0.1, 0.1], roughness=0.8)
        
        for c_idx, c_offset in enumerate([-0.6, 0.6]):
            cx = tpos[0] + c_offset
            cy = tpos[1]
            
            # Chair seat
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=(cx, cy, 0.45))
            cseat = bpy.context.active_object
            cseat.scale = (0.38, 0.38, 0.06)
            bpy.ops.object.transform_apply(scale=True)
            cseat.data.materials.append(black_seat_mat)
            parts.append(cseat)

            # Chair red backrest
            back_x = cx + (0.17 if c_offset < 0 else -0.17)
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=(back_x, cy, 0.8))
            cback = bpy.context.active_object
            cback.scale = (0.04, 0.38, 0.4)
            bpy.ops.object.transform_apply(scale=True)
            cback.data.materials.append(red_seat_mat)
            parts.append(cback)

            # Metal legs
            for lx, ly in [(-0.14, -0.14), (0.14, -0.14), (-0.14, 0.14), (0.14, 0.14)]:
                bpy.ops.mesh.primitive_cylinder_add(radius=0.015, depth=0.42, location=(cx + lx, cy + ly, 0.21))
                cleg = bpy.context.active_object
                cleg.data.materials.append(black_plastic_mat)
                parts.append(cleg)

    # 8. Join all parts
    bpy.ops.object.select_all(action='DESELECT')
    main_shop = parts[0]
    main_shop.select_set(True)
    for p in parts[1:]:
        p.select_set(True)
    bpy.context.view_layer.objects.active = main_shop
    bpy.ops.object.join()
    main_shop.name = "Superbet_Shop"

    set_viewport_to_material_shading()
    try:
        bpy.ops.file.pack_all()
    except Exception as e:
        print(f"Warning: Could not pack textures: {e}")

    # 9. Save and export
    os.makedirs(output_dir, exist_ok=True)
    export_path_glb = os.path.join(output_dir, "superbet_shop.glb")
    export_path_blend = os.path.join(output_dir, "superbet_shop.blend")
    
    bpy.ops.export_scene.gltf(filepath=export_path_glb, export_format='GLB')
    bpy.ops.wm.save_as_mainfile(filepath=export_path_blend)
    print(f"[ASSET GENERATOR - SUPERBET] Superbet Shop exported successfully to {export_path_glb} and saved to {export_path_blend}")

def main():
    import sys
    argv = sys.argv
    if "--" in argv:
        args_to_parse = argv[argv.index("--") + 1:]
    else:
        args_to_parse = []

    parser = argparse.ArgumentParser(description="Superbet Shop asset generator")
    parser.add_argument("--output-dir", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/_data/blender_generated/superbet_shop", help="Target output directory")
    parser.add_argument("--superbet-texture", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/images/superbet-interior.jpg", help="Path to Superbet texture image")
    
    args = parser.parse_args(args_to_parse)
    generate_superbet_shop(args.superbet_texture, args.output_dir)

if __name__ == "__main__":
    main()
