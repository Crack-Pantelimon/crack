import os
import sys
import json
import bpy
import mathutils

def clear_scene():
    # Deselect all
    bpy.ops.object.select_all(action='DESELECT')
    # Select all objects in the scene
    for obj in bpy.data.objects:
        obj.select_set(True)
    bpy.ops.object.delete(use_global=False)
    
    # Clear orphan data
    for block in bpy.data.meshes:
        if block.users == 0:
            bpy.data.meshes.remove(block)
    for block in bpy.data.armatures:
        if block.users == 0:
            bpy.data.armatures.remove(block)

def get_armature_object():
    for obj in bpy.data.objects:
        if obj.type == 'ARMATURE':
            return obj
    return None

def build_joints_list(armature):
    joints = []
    # Add the Armature object itself as the root joint (coccis_entity)
    joints.append({
        'name': armature.name,
        'pos': mathutils.Vector((0, 0, 0)),
        'parent': None
    })
    
    # Find all root bones (bones with no parent bone)
    root_bones = [b for b in armature.data.bones if b.parent is None]
    root_bones.sort(key=lambda b: b.name)
    
    def traverse(bone):
        joints.append({
            'name': bone.name,
            'pos': bone.matrix_local.translation.copy(),
            'parent': bone.parent.name if bone.parent else armature.name
        })
        # Traverse children bones
        children = sorted(list(bone.children), key=lambda b: b.name)
        for child in children:
            traverse(child)
            
    for rb in root_bones:
        traverse(rb)
        
    # Filter joints to match Rust's is_valid_joint:
    # let is_valid_joint = name_str.starts_with("bone_") || name_str == "Armature";
    # If no bones start with "bone_", include all bones (e.g. for reference model)
    has_bone_prefix = any(b.name.startswith("bone_") for b in armature.data.bones)
    filtered = []
    for j in joints:
        if has_bone_prefix:
            if j['name'].startswith('bone_') or j['name'] == armature.name:
                filtered.append(j)
        else:
            filtered.append(j)
    return filtered


def classify_skeleton(joints, armature_name):
    labels = {}
    if not joints:
        return labels, None, None, None, None, None, None

    # coccis is joints[0]
    coccis_name = joints[0]['name']
    labels[coccis_name] = 'Midgroin'

    # head has max height (Z in Blender, since Z is up)
    head_idx = 0
    max_z = joints[0]['pos'].z
    for idx, joint in enumerate(joints):
        if joint['pos'].z > max_z:
            max_z = joint['pos'].z
            head_idx = idx
            
    head_name = joints[head_idx]['name']
    labels[head_name] = 'Head'

    # find_parent_of helper
    def find_parent_of(name):
        for j in joints:
            if j['name'] == name:
                return j['parent']
        return None

    # spine path from head to coccis
    spine_path = []
    current = head_name
    while current != coccis_name and current is not None:
        spine_path.append(current)
        current = find_parent_of(current)
    spine_path.append(coccis_name)

    # neck is parent of head
    neck_name = None
    head_parent = joints[head_idx]['parent']
    if head_parent and head_parent != coccis_name:
        labels[head_parent] = 'Neck'
        neck_name = head_parent

    # spine nodes
    for node in spine_path:
        if node != head_name and node != neck_name and node != coccis_name:
            labels[node] = 'Spine'

    # joints center x
    joints_min_x = min(j['pos'].x for j in joints)
    joints_max_x = max(j['pos'].x for j in joints)
    joints_center_x = (joints_min_x + joints_max_x) / 2.0

    def is_left(pos):
        return pos.x > joints_center_x

    def is_right(pos):
        return pos.x < joints_center_x

    # left and right heel (min Z coordinate)
    left_heel_name = None
    left_min_z = float('inf')
    right_heel_name = None
    right_min_z = float('inf')

    for joint in joints:
        if joint['name'] in [armature_name, 'root']:
            continue
        pos = joint['pos']
        if is_left(pos) and pos.z < left_min_z:
            left_min_z = pos.z
            left_heel_name = joint['name']
        if is_right(pos) and pos.z < right_min_z:
            right_min_z = pos.z
            right_heel_name = joint['name']

    # left and right hand tip (max dist from center X)
    left_hand_tip_name = None
    left_max_dist = -float('inf')
    right_hand_tip_name = None
    right_max_dist = -float('inf')

    for joint in joints:
        if joint['name'] in [armature_name, 'root']:
            continue
        pos = joint['pos']
        dist = abs(pos.x - joints_center_x)
        if is_left(pos) and dist > left_max_dist:
            left_max_dist = dist
            left_hand_tip_name = joint['name']
        if is_right(pos) and dist > right_max_dist:
            right_max_dist = dist
            right_hand_tip_name = joint['name']

    # classify limb path helper
    def classify_limb_path(tip_name, spine_path, root_name, limb_main_label, limb_shoulder_label, limb_hand_label):
        if not tip_name:
            return None
        
        path = []
        current = tip_name
        while current not in spine_path and current is not None:
            path.append(current)
            current = find_parent_of(current)
            
        if len(path) < 2:
            labels[tip_name] = limb_hand_label
            return None

        segments = []
        for i in range(len(path)):
            node = path[i]
            parent = find_parent_of(node) if i == len(path) - 1 else path[i + 1]
            if parent:
                pos_node = next(j['pos'] for j in joints if j['name'] == node)
                pos_parent = next(j['pos'] for j in joints if j['name'] == parent)
                length = (pos_node - pos_parent).length
                segments.append((i, node, parent, length))

        # sort by length descending
        segments.sort(key=lambda x: x[3], reverse=True)

        if len(segments) >= 2:
            idxs = [segments[0][0], segments[1][0]]
            idxs.sort()
            idx1, idx2 = idxs[0], idxs[1]
        else:
            idx1, idx2 = 0, len(path) - 1

        wrist_node = path[idx1]
        elbow_node = path[idx2]
        shoulder_node = find_parent_of(path[idx2]) if idx2 == len(path) - 1 else path[idx2 + 1]
        if not shoulder_node:
            shoulder_node = path[idx2]

        for i in range(len(path)):
            node = path[i]
            if i < idx1:
                labels[node] = limb_hand_label
            elif i >= idx1 and i <= idx2:
                labels[node] = limb_main_label
            else:
                labels[node] = limb_shoulder_label

        return shoulder_node, elbow_node, wrist_node

    classify_limb_path(left_hand_tip_name, spine_path, coccis_name, 'LeftArm', 'LeftShoulder', 'LeftHand')
    classify_limb_path(right_hand_tip_name, spine_path, coccis_name, 'RightArm', 'RightShoulder', 'RightHand')
    classify_limb_path(left_heel_name, spine_path, coccis_name, 'LeftLeg', 'Midgroin', 'LeftFoot')
    classify_limb_path(right_heel_name, spine_path, coccis_name, 'RightLeg', 'Midgroin', 'RightFoot')

    # Convert mapping from bone_name -> label to label -> bone_name(s)
    label_to_bones = {}
    for bone_name, label in labels.items():
        if label not in label_to_bones:
            label_to_bones[label] = []
        label_to_bones[label].append(bone_name)

    json_mapping = {}
    all_labels = [
        'Head', 'Neck', 'Spine', 'Midgroin', 
        'LeftShoulder', 'RightShoulder', 'LeftArm', 'RightArm', 'LeftHand', 'RightHand', 
        'LeftLeg', 'RightLeg', 'LeftFoot', 'RightFoot'
    ]
    for lbl in all_labels:
        bones = label_to_bones.get(lbl, [])
        if len(bones) == 0:
            json_mapping[lbl] = None
        elif len(bones) == 1:
            json_mapping[lbl] = bones[0]
        else:
            json_mapping[lbl] = bones

    return json_mapping

def apply_transforms_safe(armature):
    # Find all child meshes
    child_meshes = []
    for obj in bpy.data.objects:
        if obj.type == 'MESH' and obj.parent == armature:
            child_meshes.append(obj)
            
    # 1. Unparent child meshes keeping transform
    if child_meshes:
        bpy.ops.object.select_all(action='DESELECT')
        for mesh in child_meshes:
            mesh.select_set(True)
        bpy.context.view_layer.objects.active = child_meshes[0]
        bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
        
    # 2. Select armature and meshes to apply transforms
    bpy.ops.object.select_all(action='DESELECT')
    armature.select_set(True)
    for mesh in child_meshes:
        mesh.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    
    # 3. Parent meshes back to armature keeping transform
    for mesh in child_meshes:
        bpy.ops.object.select_all(action='DESELECT')
        mesh.select_set(True)
        armature.select_set(True)
        bpy.context.view_layer.objects.active = armature
        bpy.ops.object.parent_set(type='OBJECT', keep_transform=True)

def rotate_180_degrees_up(armature):
    print("Rotating model 180 degrees around Z-axis...")
    R_180 = mathutils.Quaternion((0, 0, 1), 3.141592653589793)
    armature.location = R_180 @ armature.location
    if armature.rotation_mode == 'QUATERNION':
        armature.rotation_quaternion = R_180 @ armature.rotation_quaternion
    elif armature.rotation_mode == 'AXIS_ANGLE':
        q = mathutils.Quaternion(armature.rotation_axis_angle[1:], armature.rotation_axis_angle[0])
        q_rot = R_180 @ q
        axis, angle = q_rot.to_axis_angle()
        armature.rotation_axis_angle = (angle, axis[0], axis[1], axis[2])
    else:
        q = armature.rotation_euler.to_quaternion()
        q_rot = R_180 @ q
        armature.rotation_euler = q_rot.to_euler(armature.rotation_mode)
        
    bpy.context.view_layer.update()
    apply_transforms_safe(armature)

def align_head_above_cog(armature, head_bone_name):
    head_bone = armature.data.bones.get(head_bone_name)
    if not head_bone:
        print("Warning: head bone not found in armature for alignment")
        return
        
    has_bone_prefix = any(b.name.startswith("bone_") for b in armature.data.bones)
    if has_bone_prefix:
        bone_positions = [b.matrix_local.translation for b in armature.data.bones if b.name.startswith("bone_")]
    else:
        bone_positions = [b.matrix_local.translation for b in armature.data.bones if b.name != 'root']
        
    if len(bone_positions) > 0:
        C = sum(bone_positions, mathutils.Vector((0, 0, 0))) / len(bone_positions)
    else:
        C = mathutils.Vector((0, 0, 0))
        
    H = head_bone.matrix_local.translation
    
    C_world = armature.matrix_world @ C
    H_world = armature.matrix_world @ H
    
    V = H_world - C_world
    R = V.rotation_difference(mathutils.Vector((0, 0, 1)))
    
    print(f"Head world: {H_world}, Center of Gravity world: {C_world}")
    print(f"Aligning head to be directly above center of gravity (rotation: {R.to_euler()})")
    
    empty = bpy.data.objects.new("Temp_Pivot", None)
    bpy.context.scene.collection.objects.link(empty)
    empty.location = C_world
    
    bpy.ops.object.select_all(action='DESELECT')
    armature.select_set(True)
    bpy.context.view_layer.objects.active = empty
    bpy.ops.object.parent_set(type='OBJECT', keep_transform=True)
    
    empty.rotation_mode = 'QUATERNION'
    empty.rotation_quaternion = R @ empty.rotation_quaternion
    bpy.context.view_layer.update()
    
    bpy.ops.object.select_all(action='DESELECT')
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.parent_clear(type='CLEAR_KEEP_TRANSFORM')
    
    bpy.data.objects.remove(empty, do_unlink=True)
    
    apply_transforms_safe(armature)

def position_model_on_ground_and_center(armature):
    print("Moving model to sit on Z=0 and center COG at X=0, Y=0...")
    lowest_z = float('inf')
    for obj in bpy.data.objects:
        if obj.type == 'MESH':
            mesh = obj.data
            for vertex in mesh.vertices:
                world_pos = obj.matrix_world @ vertex.co
                if world_pos.z < lowest_z:
                    lowest_z = world_pos.z
    if lowest_z == float('inf'):
        lowest_z = 0.0
        
    has_bone_prefix = any(b.name.startswith("bone_") for b in armature.data.bones)
    if has_bone_prefix:
        bone_positions = [b.matrix_local.translation for b in armature.data.bones if b.name.startswith("bone_")]
    else:
        bone_positions = [b.matrix_local.translation for b in armature.data.bones if b.name != 'root']
        
    if len(bone_positions) > 0:
        C = sum(bone_positions, mathutils.Vector((0, 0, 0))) / len(bone_positions)
    else:
        C = mathutils.Vector((0, 0, 0))
        
    C_world = armature.matrix_world @ C
    
    translation = mathutils.Vector((-C_world.x, -C_world.y, -lowest_z/2.0))
    print(f"Lowest vertex Z: {lowest_z}")
    print(f"Center of gravity world: {C_world}")
    print(f"Applying translation: {translation}")
    
    armature.location += translation
    bpy.context.view_layer.update()
    
    apply_transforms_safe(armature)

def scale_model_2x(armature):
    print("Scaling model 2x...")
    armature.scale *= 2.0
    bpy.context.view_layer.update()
    apply_transforms_safe(armature)


def stage_1_rotate_model(armature):
    # Ensure all Mesh objects are parented to the Armature initially
    for obj in bpy.data.objects:
        if obj.type == 'MESH' and obj.parent != armature:
            print(f"Parenting mesh {obj.name} to armature {armature.name}...")
            obj.parent = armature
            obj.matrix_parent_inverse = armature.matrix_world.inverted()
            
    # 1. 180 degree initial rotation
    rotate_180_degrees_up(armature)
    
    # 2. Detect bones as normal
    joints = build_joints_list(armature)
    bone_mapping = classify_skeleton(joints, armature.name)
    
    # 3. Realign it such that the head is above center of gravity
    head_bone_name = bone_mapping.get('Head')
    if head_bone_name:
        align_head_above_cog(armature, head_bone_name)
    else:
        print("Warning: no Head bone classified for rotation alignment")
        
    # 4. Scale 2x
    scale_model_2x(armature)

    # 5. Move model so lowest point is at Z=0 and COG is at (0,0)
    position_model_on_ground_and_center(armature)
    
    # 6. Re-detect bones one final time to return the finalized mapping reflecting scaled positions
    final_joints = build_joints_list(armature)
    final_bone_mapping = classify_skeleton(final_joints, armature.name)
    return final_bone_mapping

def main():
    # Parse arguments after '--'
    try:
        args_idx = sys.argv.index('--')
        args = sys.argv[args_idx + 1:]
    except ValueError:
        print("Error: script arguments missing. Use '--' followed by ref_glb, input_glb, out_dir")
        sys.exit(1)
        
    if len(args) < 3:
        print(f"Error: expected 3 arguments, got {len(args)}")
        sys.exit(1)
        
    ref_glb_path = args[0]
    input_glb_path = args[1]
    out_dir = args[2]
    
    os.makedirs(out_dir, exist_ok=True)
    
    input_basename = os.path.basename(input_glb_path)
    input_name_no_ext = os.path.splitext(input_basename)[0]
    
    # Define JSON output file paths
    flag_bones_json_path = os.path.join(out_dir, f"{input_name_no_ext}_flag_bones.json")
    ref_bones_json_path = os.path.join(out_dir, f"{input_name_no_ext}_reference_bones.json")
    output_glb_path = os.path.join(out_dir, input_basename)
    
    # 1. Process Input GLB: Rotate, Identify, Align, Ground, Scale & Save
    clear_scene()
    print(f"Importing input model: {input_glb_path}")
    bpy.ops.import_scene.gltf(filepath=input_glb_path)
    
    armature = get_armature_object()
    if not armature:
        print("Error: No armature found in input model")
        sys.exit(1)
        
    # Execute stage_1_rotate_model pipeline
    input_bone_mapping = stage_1_rotate_model(armature)
    
    # Save input bone mapping to JSON
    with open(flag_bones_json_path, 'w') as f:
        json.dump(input_bone_mapping, f, indent=2)
    print(f"Saved input bones JSON to {flag_bones_json_path}")
        
    # Export aligned model
    print(f"Exporting aligned model to: {output_glb_path}")
    # Deselect all, then select only Armature and Meshes to export
    bpy.ops.object.select_all(action='DESELECT')
    for obj in bpy.data.objects:
        if obj.type in ['ARMATURE', 'MESH']:
            obj.select_set(True)
    bpy.ops.export_scene.gltf(filepath=output_glb_path, use_selection=True)
    print("Export complete.")
    
    # 2. Process Reference GLB: Identify Bones
    clear_scene()
    print(f"Importing reference model: {ref_glb_path}")
    bpy.ops.import_scene.gltf(filepath=ref_glb_path)
    
    ref_armature = get_armature_object()
    if not ref_armature:
        print("Error: No armature found in reference model")
        sys.exit(1)
        
    print("Classifying reference model bones...")
    ref_joints = build_joints_list(ref_armature)
    ref_bone_mapping = classify_skeleton(ref_joints, ref_armature.name)
    
    # Save reference bone mapping to JSON
    with open(ref_bones_json_path, 'w') as f:
        json.dump(ref_bone_mapping, f, indent=2)
    print(f"Saved reference bones JSON to {ref_bones_json_path}")
    
    clear_scene()
    print("Done!")

if __name__ == '__main__':
    main()
