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

def generate_bus(texture_path, output_dir):
    """
    Models the low-poly Mercedes 335 Bus and exports it to the specified output directory.
    """
    print(f"[ASSET GENERATOR] Generating 335 Bus using texture: {texture_path}")
    clear_scene()

    # 1. Bus body (box)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0, 0, 1.5))
    bus_body = bpy.context.active_object
    bus_body.name = "Bus_Body"
    bus_body.scale = (2.5, 12.0, 3.0)
    bpy.ops.object.transform_apply(scale=True)

    # 2. Apply texture material
    bus_mat = create_texture_material("Bus_Mat", texture_path, metallic=0.2, roughness=0.4)
    bus_body.data.materials.append(bus_mat)

    # 3. UV Smart Project
    bpy.context.view_layer.objects.active = bus_body
    bus_body.select_set(True)
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=66.0, island_margin=0.02)
    bpy.ops.object.mode_set(mode='OBJECT')

    # 4. Add wheels (4 cylinders)
    wheel_mat = create_texture_material("Wheel_Mat", None, color=[0.05, 0.05, 0.05], roughness=0.9)
    wheels = []
    wheel_positions = [
        (-1.25, 4.0, 0.5),  # Front Left
        (1.25, 4.0, 0.5),   # Front Right
        (-1.25, -4.0, 0.5), # Rear Left
        (1.25, -4.0, 0.5)   # Rear Right
    ]

    for idx, pos in enumerate(wheel_positions):
        # Rotate 90 degrees around Y axis so they roll forward along Y axis
        bpy.ops.mesh.primitive_cylinder_add(
            radius=0.5, 
            depth=0.3, 
            location=pos,
            rotation=(0, math.radians(90), 0)
        )
        wheel = bpy.context.active_object
        wheel.name = f"Wheel_{idx}"
        wheel.data.materials.append(wheel_mat)
        wheels.append(wheel)

    # 5. Join all bus parts
    bpy.ops.object.select_all(action='DESELECT')
    bus_body.select_set(True)
    for wheel in wheels:
        wheel.select_set(True)
    bpy.context.view_layer.objects.active = bus_body
    bpy.ops.object.join()
    bus_body.name = "Bus_335"

    # 6. Export GLB
    os.makedirs(output_dir, exist_ok=True)
    export_path = os.path.join(output_dir, "bus_335.glb")
    bpy.ops.export_scene.gltf(filepath=export_path, export_format='GLB')
    print(f"[ASSET GENERATOR] 335 Bus exported successfully to {export_path}")


def generate_kebab_shop(texture_path, output_dir):
    """
    Models the low-poly Kebab Shop room interior and exports it to the specified output directory.
    """
    print(f"[ASSET GENERATOR] Generating Kebab Shop using texture: {texture_path}")
    clear_scene()

    parts = []

    # 1. Floor
    bpy.ops.mesh.primitive_plane_add(size=8.0, location=(0, 0, 0))
    floor = bpy.context.active_object
    floor.name = "Floor"
    floor_mat = create_texture_material("Floor_Mat", None, color=[0.35, 0.22, 0.15], roughness=0.8)
    floor.data.materials.append(floor_mat)
    parts.append(floor)

    # 2. Back Wall (North)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0, 4.0, 1.5))
    back_wall = bpy.context.active_object
    back_wall.name = "Back_Wall"
    back_wall.scale = (8.0, 0.1, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    parts.append(back_wall)

    # 3. Left Wall (West)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(-4.0, 0, 1.5))
    left_wall = bpy.context.active_object
    left_wall.name = "Left_Wall"
    left_wall.scale = (0.1, 8.0, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    parts.append(left_wall)

    # 4. Right Wall (East)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(4.0, 0, 1.5))
    right_wall = bpy.context.active_object
    right_wall.name = "Right_Wall"
    right_wall.scale = (0.1, 8.0, 3.0)
    bpy.ops.object.transform_apply(scale=True)
    parts.append(right_wall)

    # 5. Apply photo texture to back wall
    kebab_mat = create_texture_material("Kebab_Mat", texture_path, roughness=0.6)
    back_wall.data.materials.append(kebab_mat)

    # 6. UV Project back wall
    bpy.context.view_layer.objects.active = back_wall
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project()
    bpy.ops.object.mode_set(mode='OBJECT')

    # 7. Set standard wall color for side walls
    wall_mat_plain = create_texture_material("Wall_Mat_Plain", None, color=[0.85, 0.85, 0.8], roughness=0.9)
    left_wall.data.materials.append(wall_mat_plain)
    right_wall.data.materials.append(wall_mat_plain)

    # 8. Serving Counter
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0, 2.0, 0.55))
    counter = bpy.context.active_object
    counter.name = "Serving_Counter"
    counter.scale = (4.0, 1.0, 1.1)
    bpy.ops.object.transform_apply(scale=True)
    counter.data.materials.append(kebab_mat) # Wrap photo texture on counter too
    parts.append(counter)

    # 9. Kebab Rotisseries (2 cylinders)
    rotisserie_mat = create_texture_material("Metal_Mat", None, color=[0.75, 0.75, 0.75], metallic=1.0, roughness=0.2)
    for idx, x_pos in enumerate([-1.0, 1.0]):
        bpy.ops.mesh.primitive_cylinder_add(radius=0.2, depth=1.2, location=(x_pos, 3.2, 1.3))
        spit = bpy.context.active_object
        spit.name = f"Rotisserie_{idx}"
        spit.data.materials.append(rotisserie_mat)
        parts.append(spit)

    # 10. Customer Tables (2) and Chairs (4)
    wood_mat = create_texture_material("Wood_Mat", None, color=[0.45, 0.3, 0.2], roughness=0.7)
    table_positions = [(-2.0, -2.0), (2.0, -2.0)]
    for t_idx, t_pos in enumerate(table_positions):
        # Table top
        bpy.ops.mesh.primitive_cube_add(size=1.0, location=(t_pos[0], t_pos[1], 0.75))
        tt = bpy.context.active_object
        tt.scale = (1.2, 0.8, 0.05)
        bpy.ops.object.transform_apply(scale=True)
        tt.data.materials.append(wood_mat)
        parts.append(tt)
        
        # Table legs
        for x_off, y_off in [(-0.5, -0.3), (0.5, -0.3), (-0.5, 0.3), (0.5, 0.3)]:
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=(t_pos[0] + x_off, t_pos[1] + y_off, 0.35))
            leg = bpy.context.active_object
            leg.scale = (0.05, 0.05, 0.7)
            bpy.ops.object.transform_apply(scale=True)
            leg.data.materials.append(wood_mat)
            parts.append(leg)

        # 2 Chairs per table
        for c_idx, y_off in enumerate([-0.7, 0.7]):
            chair_center = (t_pos[0], t_pos[1] + y_off)
            # Chair seat
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=(chair_center[0], chair_center[1], 0.45))
            seat = bpy.context.active_object
            seat.scale = (0.4, 0.4, 0.05)
            bpy.ops.object.transform_apply(scale=True)
            seat.data.materials.append(wood_mat)
            parts.append(seat)
            
            # Chair backrest
            back_y = chair_center[1] + (0.18 if y_off < 0 else -0.18)
            bpy.ops.mesh.primitive_cube_add(size=1.0, location=(chair_center[0], back_y, 0.75))
            back = bpy.context.active_object
            back.scale = (0.4, 0.03, 0.6)
            bpy.ops.object.transform_apply(scale=True)
            back.data.materials.append(wood_mat)
            parts.append(back)
            
            # Chair legs
            for cx_off, cy_off in [(-0.15, -0.15), (0.15, -0.15), (-0.15, 0.15), (0.15, 0.15)]:
                bpy.ops.mesh.primitive_cube_add(size=1.0, location=(chair_center[0] + cx_off, chair_center[1] + cy_off, 0.22))
                c_leg = bpy.context.active_object
                c_leg.scale = (0.03, 0.03, 0.44)
                bpy.ops.object.transform_apply(scale=True)
                c_leg.data.materials.append(wood_mat)
                parts.append(c_leg)

    # 11. Join all kebab shop parts
    bpy.ops.object.select_all(action='DESELECT')
    main_obj = parts[0]
    main_obj.select_set(True)
    for p in parts[1:]:
        p.select_set(True)
    bpy.context.view_layer.objects.active = main_obj
    bpy.ops.object.join()
    main_obj.name = "Kebab_Shop"

    # 12. Export GLB
    os.makedirs(output_dir, exist_ok=True)
    export_path = os.path.join(output_dir, "kebab_shop.glb")
    bpy.ops.export_scene.gltf(filepath=export_path, export_format='GLB')
    print(f"[ASSET GENERATOR] Kebab Shop exported successfully to {export_path}")


def main():
    # Since this script runs inside Blender's python, we parse args after '--'
    import sys
    argv = sys.argv
    if "--" in argv:
        args_to_parse = argv[argv.index("--") + 1:]
    else:
        args_to_parse = []

    parser = argparse.ArgumentParser(description="Modular 3D asset and texture generator for GTA Vice City: Pantelimon")
    parser.add_argument("--output-dir", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/_data/blender_generated", help="Target output directory")
    parser.add_argument("--bus-texture", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/images/8170086299_2a8157c6bc_z.jpg", help="Path to bus texture image")
    parser.add_argument("--kebab-texture", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/images/IMG-20170701-WA0024.webp", help="Path to kebab shop texture image")
    parser.add_argument("--model", type=str, choices=["all", "bus", "kebab"], default="all", help="Which model to build")
    
    args = parser.parse_args(args_to_parse)

    if args.model in ["all", "bus"]:
        generate_bus(args.bus_texture, args.output_dir)
        
    if args.model in ["all", "kebab"]:
        generate_kebab_shop(args.kebab_texture, args.output_dir)

if __name__ == "__main__":
    main()
