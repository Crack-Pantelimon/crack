import bpy
import math
import os
import argparse
import subprocess
import sys

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

def make_bus_textures(output_dir):
    """
    Programmatically draws the orthographic front, side, and back textures for the 335 bus
    by executing Pillow in the sandboxed python virtual environment via subprocess.
    """
    textures_dir = os.path.join(output_dir, "textures")
    os.makedirs(textures_dir, exist_ok=True)
    
    print("[ASSET GENERATOR - BUS] Invoking python venv subprocess to draw bus textures...")
    
    script_code = f"""
import os
from PIL import Image, ImageDraw, ImageFont

textures_dir = "{textures_dir}"
os.makedirs(textures_dir, exist_ok=True)

font_path = "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf"
if not os.path.exists(font_path):
    font_path = None

# ==========================================
# 1. FRONT TEXTURE (512x512)
# ==========================================
img_front = Image.new("RGBA", (512, 512), (245, 245, 242, 255))
draw = ImageDraw.Draw(img_front)

# Windshield / LED Screen
draw.rectangle([15, 30, 497, 240], fill=(15, 15, 15, 255))

if font_path:
    f_led = ImageFont.truetype(font_path, 28)
    f_led_small = ImageFont.truetype(font_path, 20)
    f_plate = ImageFont.truetype(font_path, 22)
    f_ratb = ImageFont.truetype(font_path, 18)
else:
    f_led = f_led_small = f_plate = f_ratb = ImageFont.load_default()
    
# Destination text
draw.text((60, 50), "335", fill=(255, 130, 0, 255), font=f_led)
draw.text((150, 50), "COMPLEX BANEASA", fill=(255, 130, 0, 255), font=f_led)
draw.text((150, 85), "BARAJUL DUNARII", fill=(255, 130, 0, 255), font=f_led_small)
draw.text((50, 180), "Aer Conditionat", fill=(255, 255, 255, 255), font=f_led_small)

# Headlights
draw.ellipse([30, 360, 90, 400], fill=(255, 250, 200, 255), outline=(100, 100, 100, 255), width=3)
draw.ellipse([422, 360, 482, 400], fill=(255, 250, 200, 255), outline=(100, 100, 100, 255), width=3)

# Mercedes star
draw.ellipse([236, 330, 276, 370], fill=(200, 200, 200, 255), outline=(120, 120, 120, 255), width=3)
draw.line([256, 350, 256, 333], fill=(120, 120, 120, 255), width=3)
draw.line([256, 350, 241, 361], fill=(120, 120, 120, 255), width=3)
draw.line([256, 350, 271, 361], fill=(120, 120, 120, 255), width=3)

# License Plate
draw.rectangle([190, 430, 322, 470], fill=(255, 255, 255, 255), outline=(50, 50, 50, 255), width=2)
draw.text((200, 435), "B 97 YXX", fill=(0, 0, 0, 255), font=f_plate)

draw.text((120, 340), "ratb", fill=(0, 0, 0, 255), font=f_ratb)
draw.text((120, 360), "4951", fill=(0, 0, 0, 255), font=f_ratb)

# Bumper
draw.rectangle([0, 480, 512, 512], fill=(60, 60, 60, 255))
img_front.save(os.path.join(textures_dir, "bus_front.png"))

# ==========================================
# 2. SIDE TEXTURE (1024x256)
# ==========================================
img_side = Image.new("RGBA", (1024, 256), (245, 245, 242, 255))
draw_side = ImageDraw.Draw(img_side)
draw_side.rectangle([0, 25, 1024, 115], fill=(20, 20, 20, 255))

for dx in [120, 500, 880]:
    draw_side.rectangle([dx, 25, dx + 80, 230], fill=(245, 245, 242, 255), outline=(80, 80, 80, 255), width=3)
    draw_side.line([dx + 40, 25, dx + 40, 230], fill=(80, 80, 80, 255), width=2)
    draw_side.rectangle([dx + 8, 35, dx + 32, 115], fill=(30, 30, 30, 255))
    draw_side.rectangle([dx + 48, 35, dx + 72, 115], fill=(30, 30, 30, 255))
    draw_side.rectangle([dx + 8, 130, dx + 32, 210], fill=(30, 30, 30, 255))
    draw_side.rectangle([dx + 48, 130, dx + 72, 210], fill=(30, 30, 30, 255))
    
draw_side.rectangle([470, 190, 490, 210], fill=(0, 100, 200, 255))
draw_side.rectangle([590, 190, 610, 210], fill=(0, 100, 200, 255))
draw_side.text((250, 45), "Aer Conditionat", fill=(255, 255, 255, 255), font=f_ratb)
draw_side.text((320, 150), "ratb", fill=(0, 0, 0, 255), font=f_ratb)
draw_side.text((320, 175), "4951", fill=(0, 0, 0, 255), font=f_ratb)
draw_side.rectangle([0, 240, 1024, 256], fill=(50, 50, 50, 255))
img_side.save(os.path.join(textures_dir, "bus_side.png"))

# ==========================================
# 3. BACK TEXTURE (512x512)
# ==========================================
img_back = Image.new("RGBA", (512, 512), (245, 245, 242, 255))
draw_back = ImageDraw.Draw(img_back)
draw_back.rectangle([30, 40, 482, 200], fill=(20, 20, 20, 255))
draw_back.rectangle([20, 400, 60, 440], fill=(240, 30, 30, 255), outline=(50, 50, 50, 255), width=2)
draw_back.rectangle([452, 400, 492, 440], fill=(240, 30, 30, 255), outline=(50, 50, 50, 255), width=2)
draw_back.rectangle([20, 440, 60, 460], fill=(255, 140, 0, 255))
draw_back.rectangle([452, 440, 492, 460], fill=(255, 140, 0, 255))
draw_back.rectangle([190, 410, 322, 450], fill=(255, 255, 255, 255), outline=(50, 50, 50, 255), width=2)
draw_back.text((200, 415), "B 97 YXX", fill=(0, 0, 0, 255), font=f_plate)
draw_back.rectangle([0, 480, 512, 512], fill=(60, 60, 60, 255))
img_back.save(os.path.join(textures_dir, "bus_back.png"))
"""
    subprocess.run(["/tmp/venv/bin/python", "-c", script_code], check=True)
    
    return {
        "front": os.path.join(textures_dir, "bus_front.png"),
        "side": os.path.join(textures_dir, "bus_side.png"),
        "back": os.path.join(textures_dir, "bus_back.png")
    }

def generate_bus(texture_path, output_dir):
    """
    Models the Mercedes Citaro 335 Bus in low-poly, creates custom orthographic textures,
    and maps them mathematically to the front, sides, and back of the bus body.
    """
    print(f"[ASSET GENERATOR - BUS] Generating 335 Bus using texture: {texture_path}")
    clear_scene()

    parts = []

    # 1. Main Bus body (White Mercedes Citaro box)
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0, 0, 1.5))
    bus_body = bpy.context.active_object
    bus_body.name = "Bus_Body"
    bus_body.scale = (2.5, 12.0, 2.8)
    bpy.ops.object.transform_apply(scale=True)
    parts.append(bus_body)

    # 2. Make modular orthographic textures using Pillow via venv python
    tex_files = make_bus_textures(output_dir)

    # 3. Create materials for Slots
    white_paint_mat = create_texture_material("Bus_Paint_Mat", None, color=[0.96, 0.96, 0.94], roughness=0.3)
    side_mat = create_texture_material("Bus_Side_Mat", tex_files["side"], roughness=0.3)
    front_mat = create_texture_material("Bus_Front_Mat", tex_files["front"], roughness=0.3)
    back_mat = create_texture_material("Bus_Back_Mat", tex_files["back"], roughness=0.3)

    # Add materials to slot list
    bus_body.data.materials.append(white_paint_mat) # Slot 0
    bus_body.data.materials.append(side_mat)        # Slot 1
    bus_body.data.materials.append(front_mat)       # Slot 2
    bus_body.data.materials.append(back_mat)        # Slot 3

    # 4. Map material slots to specific faces based on normals
    for face in bus_body.data.polygons:
        normal = face.normal
        if normal.y > 0.9:     # Front face (Y+)
            face.material_index = 2
        elif normal.y < -0.9:  # Back face (Y-)
            face.material_index = 3
        elif abs(normal.x) > 0.9: # Side faces (X+ and X-)
            face.material_index = 1
        else:                  # Top / Bottom
            face.material_index = 0

    # 5. Calculate UV coordinates mathematically for clean wrapping
    if not bus_body.data.uv_layers:
        bus_body.data.uv_layers.new()
    uv_layer = bus_body.data.uv_layers.active.data

    for face in bus_body.data.polygons:
        loop_indices = face.loop_indices
        normal = face.normal
        
        if abs(normal.x) > 0.9: # Side faces (X+ or X-)
            # Map Y (length) to U, Z (height) to V
            # U goes from 0 (at Y=-6.0) to 1 (at Y=6.0)
            # V goes from 0 (at Z=0.1) to 1 (at Z=2.9)
            for li in loop_indices:
                v_idx = bus_body.data.loops[li].vertex_index
                v = bus_body.data.vertices[v_idx]
                u = (v.co.y + 6.0) / 12.0
                v_uv = (v.co.z - 0.1) / 2.8
                # Flip U on the left side to keep text running forward-to-back on both sides
                if normal.x < 0:
                    u = 1.0 - u
                uv_layer[li].uv = (u, v_uv)
                
        elif normal.y > 0.9: # Front face (Y+)
            # Map X (width) to U, Z (height) to V
            for li in loop_indices:
                v_idx = bus_body.data.loops[li].vertex_index
                v = bus_body.data.vertices[v_idx]
                u = (v.co.x + 1.25) / 2.5
                v_uv = (v.co.z - 0.1) / 2.8
                uv_layer[li].uv = (u, v_uv)
                
        elif normal.y < -0.9: # Back face (Y-)
            # Map X to U, Z to V
            for li in loop_indices:
                v_idx = bus_body.data.loops[li].vertex_index
                v = bus_body.data.vertices[v_idx]
                u = 1.0 - (v.co.x + 1.25) / 2.5
                v_uv = (v.co.z - 0.1) / 2.8
                uv_layer[li].uv = (u, v_uv)
                
        else: # Top / Bottom faces (Z+ or Z-)
            # Simple plane mapping
            for li in loop_indices:
                v_idx = bus_body.data.loops[li].vertex_index
                v = bus_body.data.vertices[v_idx]
                u = (v.co.x + 1.25) / 2.5
                v_uv = (v.co.y + 6.0) / 12.0
                uv_layer[li].uv = (u, v_uv)

    # 6. Add wheels (4 dark rubber cylinders)
    wheel_mat = create_texture_material("Wheel_Mat", None, color=[0.08, 0.08, 0.08], roughness=0.9)
    wheel_positions = [
        (-1.25, 4.0, 0.5),  # Front Left
        (1.25, 4.0, 0.5),   # Front Right
        (-1.25, -4.0, 0.5), # Rear Left
        (1.25, -4.0, 0.5)   # Rear Right
    ]

    for idx, pos in enumerate(wheel_positions):
        bpy.ops.mesh.primitive_cylinder_add(
            radius=0.5, 
            depth=0.3, 
            location=pos,
            rotation=(0, math.radians(90), 0)
        )
        wheel = bpy.context.active_object
        wheel.name = f"Wheel_{idx}"
        wheel.data.materials.append(wheel_mat)
        parts.append(wheel)

    # 7. Join all bus parts
    bpy.ops.object.select_all(action='DESELECT')
    main_bus = parts[0]
    main_bus.select_set(True)
    for p in parts[1:]:
        p.select_set(True)
    bpy.context.view_layer.objects.active = main_bus
    bpy.ops.object.join()
    main_bus.name = "Bus_335"

    # Set viewport shading to MATERIAL and pack textures
    set_viewport_to_material_shading()
    try:
        bpy.ops.file.pack_all()
    except Exception as e:
        print(f"Warning: Could not pack textures: {e}")

    # 8. Export GLB and save .blend project file
    os.makedirs(output_dir, exist_ok=True)
    export_path_glb = os.path.join(output_dir, "bus_335.glb")
    export_path_blend = os.path.join(output_dir, "bus_335.blend")
    
    bpy.ops.export_scene.gltf(filepath=export_path_glb, export_format='GLB')
    bpy.ops.wm.save_as_mainfile(filepath=export_path_blend)
    print(f"[ASSET GENERATOR - BUS] 335 Bus exported successfully to {export_path_glb} and saved to {export_path_blend}")


def main():
    import sys
    argv = sys.argv
    if "--" in argv:
        args_to_parse = argv[argv.index("--") + 1:]
    else:
        args_to_parse = []

    parser = argparse.ArgumentParser(description="Bus 335 asset generator")
    parser.add_argument("--output-dir", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/_data/blender_generated/bus_335", help="Target output directory")
    parser.add_argument("--bus-texture", type=str, default="/home/vasile/.gemini/antigravity/scratch/crack/images/8170086299_2a8157c6bc_z.jpg", help="Path to bus texture image")
    
    args = parser.parse_args(args_to_parse)
    generate_bus(args.bus_texture, args.output_dir)

if __name__ == "__main__":
    main()
