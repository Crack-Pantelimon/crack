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

def generate_terasa_obor(texture_path, output_dir):
    print(f"[ASSET GENERATOR - OBOR] Generating Terasa Obor using texture: {texture_path}")
    clear_scene()

    parts = []

    # 1. Floor (Dark asphalt pavement)
    bpy.ops.mesh.primitive_plane_add(size=8.0, location=(0, 0, 0))
    floor = bpy.context.active_object
    floor.name = "Floor"
    asphalt_mat = create_texture_material("Asphalt_Mat", None, color=[0.15, 0.15, 0.15], roughness=0.95)
    floor.data.materials.append(asphalt_mat)
    parts.append(floor)

    # 2. Main Canopy Structure (Black metal frame + white gabled roof sheets)
    black_metal_mat = create_texture_material("Black_Metal_Mat", None, color=[0.05, 0.05, 0.05], metallic=0.7, roughness=0.4)
    roof_sheet_mat = create_texture_material("Roof_Sheet_Mat", None, color=[0.88, 0.88, 0.86], roughness=0.5)

    # 4 Vertical Pillars
    pillar_coords = [(-3.0, -3.0), (3.0, -3.0), (-3.0, 3.0), (3.0, 3.0)]
    for idx, (px, py) in enumerate(pillar_coords):
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=(px, py, 1.75))
        pillar = bpy.context.active_object
        pillar.name = f"Pillar_{idx}"
        pillar.scale = (0.2, 0.2, 3.5)
        bpy.ops.object.transform_apply(scale=True)
        pillar.data.materials.append(black_metal_mat)
        parts.append(pillar)

    # Horizontal Roof Beams
    beam_positions = [
        # Y direction beams
        (-3.0, 0.0, 3.5, 0.1, 6.0, 0.1),
        (3.0, 0.0, 3.5, 0.1, 6.0, 0.1),
        # X direction beams
        (0.0, -3.0, 3.5, 6.0, 0.1, 0.1),
        (0.0, 3.0, 3.5, 6.0, 0.1, 0.1),
        (0.0, 0.0, 3.5, 6.0, 0.1, 0.1)
    ]
    for idx, (bx, by, bz, sx, sy, sz) in enumerate(beam_positions):
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=(bx, by, bz))
        beam = bpy.context.active_object
        beam.scale = (sx, sy, sz)
        bpy.ops.object.transform_apply(scale=True)
        beam.data.materials.append(black_metal_mat)
        parts.append(beam)

    # Gabled Roof Sheets (left and right sloped panels)
    # Left Slope
    bpy.ops.mesh.primitive_plane_add(size=1.0, location=(-1.6, 0.0, 3.9), rotation=(0, math.radians(15), 0))
    rs_left = bpy.context.active_object
    rs_left.scale = (3.4, 6.2, 0.01)
    bpy.ops.object.transform_apply(scale=True)
    rs_left.data.materials.append(roof_sheet_mat)
    parts.append(rs_left)

    # Right Slope
    bpy.ops.mesh.primitive_plane_add(size=1.0, location=(1.6, 0.0, 3.9), rotation=(0, math.radians(-15), 0))
    rs_right = bpy.context.active_object
    rs_right.scale = (3.4, 6.2, 0.01)
    bpy.ops.object.transform_apply(scale=True)
    rs_right.data.materials.append(roof_sheet_mat)
    parts.append(rs_right)

    # 3. Red "TERASA OBOR" Sign Board (Center Front)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0.0, -2.0, 0.45))
    sign = bpy.context.active_object
    sign.name = "Obor_Sign"
    sign.scale = (1.6, 0.4, 0.9)
    bpy.ops.object.transform_apply(scale=True)
    
    red_mat = create_texture_material("Obor_Red_Mat", None, color=[0.85, 0.05, 0.05], roughness=0.6)
    sign.data.materials.append(red_mat)
    parts.append(sign)

    # White 3D Text: "TERASA"
    white_txt_mat = create_texture_material("Text_White_Mat", None, color=[1.0, 1.0, 1.0], roughness=0.5)
    bpy.ops.object.text_add(location=(-0.65, -2.22, 0.52))
    txt_terasa = bpy.context.active_object
    txt_terasa.name = "Text_Terasa"
    txt_terasa.data.body = "TERASA"
    txt_terasa.data.extrude = 0.05
    txt_terasa.scale = (0.22, 0.22, 0.22)
    txt_terasa.rotation_euler = (math.radians(90), 0, 0)
    txt_terasa.data.materials.append(white_txt_mat)
    parts.append(txt_terasa)

    # White 3D Text: "OBOR"
    bpy.ops.object.text_add(location=(-0.45, -2.22, 0.15))
    txt_obor = bpy.context.active_object
    txt_obor.name = "Text_Obor"
    txt_obor.data.body = "OBOR"
    txt_obor.data.extrude = 0.05
    txt_obor.scale = (0.25, 0.25, 0.25)
    txt_obor.rotation_euler = (math.radians(90), 0, 0)
    txt_obor.data.materials.append(white_txt_mat)
    parts.append(txt_obor)

    # 4. Grill Cart (Left side)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(-2.2, -1.0, 0.55))
    grill = bpy.context.active_object
    grill.name = "Grill_Cart"
    grill.scale = (1.5, 0.8, 1.1)
    bpy.ops.object.transform_apply(scale=True)
    
    metal_gray_mat = create_texture_material("Metal_Gray_Mat", None, color=[0.6, 0.6, 0.6], metallic=0.9, roughness=0.3)
    grill.data.materials.append(metal_gray_mat)
    parts.append(grill)

    # Small roof for grill cart
    bpy.ops.mesh.primitive_plane_add(size=1.0, location=(-2.2, -1.0, 1.15), rotation=(math.radians(10), 0, 0))
    grill_roof = bpy.context.active_object
    grill_roof.scale = (1.6, 0.9, 0.01)
    grill_roof.data.materials.append(metal_gray_mat)
    parts.append(grill_roof)

    # 5. Round Standing Beer Tables (3)
    beer_table_mat = create_texture_material("Beer_Table_Red_Mat", None, color=[0.85, 0.05, 0.05], roughness=0.6)
    table_coords = [(1.8, 1.8), (1.8, -1.2), (-1.8, 2.0)]
    for idx, (tx, ty) in enumerate(table_coords):
        # Round tabletop
        bpy.ops.mesh.primitive_cylinder_add(radius=0.45, depth=0.04, location=(tx, ty, 1.1))
        t_top = bpy.context.active_object
        t_top.data.materials.append(beer_table_mat)
        parts.append(t_top)

        # Central support pillar
        bpy.ops.mesh.primitive_cylinder_add(radius=0.06, depth=1.08, location=(tx, ty, 0.54))
        t_pole = bpy.context.active_object
        t_pole.data.materials.append(beer_table_mat)
        parts.append(t_pole)

        # Base circle ring
        bpy.ops.mesh.primitive_cylinder_add(radius=0.35, depth=0.03, location=(tx, ty, 0.015))
        t_base = bpy.context.active_object
        t_base.data.materials.append(beer_table_mat)
        parts.append(t_base)

    # 6. Red Feather Flags (3) with photo texture
    flag_mat = create_texture_material("Skol_Flag_Mat", texture_path, roughness=0.6)
    flag_coords = [(-3.3, 0.0), (3.3, 0.0), (-3.3, -2.5)]
    
    for idx, (fx, fy) in enumerate(flag_coords):
        # Pole
        bpy.ops.mesh.primitive_cylinder_add(radius=0.02, depth=3.0, location=(fx, fy, 1.5))
        fpole = bpy.context.active_object
        fpole.data.materials.append(black_metal_mat)
        parts.append(fpole)

        # Curved Flag geometry (represented as a vertical curved plane)
        # We model it as a vertical box panel and apply the texture
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=(fx + 0.25, fy, 2.0))
        flag = bpy.context.active_object
        flag.scale = (0.5, 0.02, 1.8)
        bpy.ops.object.transform_apply(scale=True)
        flag.data.materials.append(flag_mat)
        
        # UV Smart project
        bpy.context.view_layer.objects.active = flag
        bpy.ops.object.mode_set(mode='EDIT')
        bpy.ops.mesh.select_all(action='SELECT')
        bpy.ops.uv.smart_project()
        bpy.ops.object.mode_set(mode='OBJECT')
        parts.append(flag)

    # 7. Join all parts
    bpy.ops.object.select_all(action='DESELECT')
    main_shop = parts[0]
    main_shop.select_set(True)
    for p in parts[1:]:
        p.select_set(True)
    bpy.context.view_layer.objects.active = main_shop
    bpy.ops.object.join()
    main_shop.name = "Terasa_Obor"

    set_viewport_to_material_shading()
    try:
        bpy.ops.file.pack_all()
    except Exception as e:
        print(f"Warning: Could not pack textures: {e}")

    # 8. Save and export
    os.makedirs(output_dir, exist_ok=True)
    export_path_glb = os.path.join(output_dir, "terasa_obor.glb")
    export_path_blend = os.path.join(output_dir, "terasa_obor.blend")
    
    bpy.ops.export_scene.gltf(filepath=export_path_glb, export_format='GLB')
    bpy.ops.wm.save_as_mainfile(filepath=export_path_blend)
    print(f"[ASSET GENERATOR - OBOR] Terasa Obor exported successfully to {export_path_glb} and saved to {export_path_blend}")

def main():
    import sys
    argv = sys.argv
    if "--" in argv:
        args_to_parse = argv[argv.index("--") + 1:]
    else:
        args_to_parse = []

    parser = argparse.ArgumentParser(description="Terasa Obor asset generator")
    parser.add_argument("--output-dir", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/_data/blender_generated/terasa_obor", help="Target output directory")
    parser.add_argument("--obor-texture", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/images/terasa-obor-1-scaled.webp", help="Path to Obor texture image")
    
    args = parser.parse_args(args_to_parse)
    generate_terasa_obor(args.obor-texture if hasattr(args, 'obor-texture') else args.obor_texture, args.output_dir)

if __name__ == "__main__":
    main()
