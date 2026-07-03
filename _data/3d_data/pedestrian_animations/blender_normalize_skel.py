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

def identify_key_bones(armature, bone_mapping):
    # Helper to resolve bone list
    def resolve_list(label):
        val = bone_mapping.get(label)
        if not val:
            return []
        if isinstance(val, list):
            return val
        return [val]

    key_bones = {}

    # Neck
    neck_list = resolve_list('Neck')
    if neck_list:
        key_bones['neck'] = neck_list[0]
    else:
        key_bones['neck'] = None

    # Legs & Arms
    for side in ['Left', 'Right']:
        # Legs
        foot_label_bones = resolve_list(f'{side}Foot')
        leg_label_bones = resolve_list(f'{side}Leg')
        midgroin_label_bones = resolve_list('Midgroin')

        hip_bone = None
        knee_bone = None
        foot_bone = None

        # Find chain Hip -> Knee -> Foot
        for f in leg_label_bones:
            bone_f = armature.data.bones.get(f)
            if bone_f and bone_f.parent and bone_f.parent.name in leg_label_bones:
                k = bone_f.parent.name
                bone_k = armature.data.bones.get(k)
                if bone_k and bone_k.parent and bone_k.parent.name in midgroin_label_bones:
                    h = bone_k.parent.name
                    # Make sure hip is not the armature name
                    if h != armature.name:
                        hip_bone = h
                        knee_bone = k
                        foot_bone = f
                        break
        
        key_bones[f'{side.lower()}_hip'] = hip_bone
        key_bones[f'{side.lower()}_knee'] = knee_bone
        key_bones[f'{side.lower()}_foot'] = foot_bone

        # Find child bone (toe/physical foot)
        child_bone = None
        if foot_bone:
            for f_child in foot_label_bones:
                bone_fc = armature.data.bones.get(f_child)
                if bone_fc and bone_fc.parent and bone_fc.parent.name == foot_bone:
                    child_bone = f_child
                    break
        key_bones[f'{side.lower()}_foot_child'] = child_bone

        # Arms
        hand_label_bones = resolve_list(f'{side}Hand')
        arm_label_bones = resolve_list(f'{side}Arm')
        shoulder_label_bones = resolve_list(f'{side}Shoulder')

        arm_bone = None
        forearm_bone = None
        wrist_bone = None

        # Find chain Arm -> Forearm -> Wrist
        for w in arm_label_bones:
            bone_w = armature.data.bones.get(w)
            if bone_w and bone_w.parent and bone_w.parent.name in arm_label_bones:
                f_arm = bone_w.parent.name
                bone_f_arm = armature.data.bones.get(f_arm)
                if bone_f_arm and bone_f_arm.parent and bone_f_arm.parent.name in shoulder_label_bones:
                    a = bone_f_arm.parent.name
                    if a != armature.name:
                        arm_bone = a
                        forearm_bone = f_arm
                        wrist_bone = w
                        break

        key_bones[f'{side.lower()}_arm'] = arm_bone
        key_bones[f'{side.lower()}_forearm'] = forearm_bone
        key_bones[f'{side.lower()}_wrist'] = wrist_bone

    return key_bones

def apply_pose_to_skin_and_armature(armature):
    print("Applying pose to meshes/skin and armature rest pose...")
    if bpy.context.object and bpy.context.object.mode != 'OBJECT':
        bpy.ops.object.mode_set(mode='OBJECT')
        
    depsgraph = bpy.context.evaluated_depsgraph_get()
    
    # 1. Extract evaluated mesh data for all child meshes to permanently bake pose deformation into vertices
    child_meshes = []
    for obj in bpy.data.objects:
        if obj.type == 'MESH' and (obj.parent == armature or any(m.type == 'ARMATURE' and m.object == armature for m in obj.modifiers)):
            child_meshes.append(obj)
            
    for mesh_obj in child_meshes:
        mesh_eval = mesh_obj.evaluated_get(depsgraph)
        new_mesh_data = bpy.data.meshes.new_from_object(mesh_eval)
        
        old_data = mesh_obj.data
        mesh_obj.data = new_mesh_data
        if old_data.users == 0:
            bpy.data.meshes.remove(old_data)
            
        for mod in list(mesh_obj.modifiers):
            if mod.type == 'ARMATURE':
                mesh_obj.modifiers.remove(mod)
                
    # 2. Apply pose as rest pose on armature
    bpy.ops.object.select_all(action='DESELECT')
    armature.select_set(True)
    bpy.context.view_layer.objects.active = armature
    bpy.ops.object.mode_set(mode='POSE')
    bpy.ops.pose.armature_apply()
    bpy.ops.object.mode_set(mode='OBJECT')
    
    # 3. Add new Armature modifier back to child meshes pointing to armature
    for mesh_obj in child_meshes:
        mod = mesh_obj.modifiers.new(name="Armature", type='ARMATURE')
        mod.object = armature
        print(f"Added new armature modifier to mesh {mesh_obj.name}")

def get_segment_direction(armature, bone_name, next_bone_name=None, is_pose=False):
    if is_pose:
        pb = armature.pose.bones.get(bone_name)
        if not pb:
            return None
        if next_bone_name:
            pb_next = armature.pose.bones.get(next_bone_name)
            if pb_next:
                dir_vec = pb_next.head - pb.head
                if dir_vec.length > 1e-6:
                    return dir_vec.normalized()
        dir_vec = pb.tail - pb.head
        if dir_vec.length > 1e-6:
            return dir_vec.normalized()
        return mathutils.Vector((0, 1, 0))
    else:
        b = armature.data.bones.get(bone_name)
        if not b:
            return None
        if next_bone_name:
            b_next = armature.data.bones.get(next_bone_name)
            if b_next:
                dir_vec = b_next.head_local - b.head_local
                if dir_vec.length > 1e-6:
                    return dir_vec.normalized()
        dir_vec = b.tail_local - b.head_local
        if dir_vec.length > 1e-6:
            return dir_vec.normalized()
        return mathutils.Vector((0, 1, 0))

def print_bone_debug_data(title, target_armature, target_key_bones, ref_debug_info):
    print(f"\n=================== Bone Alignment Debug: {title} ===================")
    
    # Define joint chains (current_bone, next_bone, label)
    debug_chains = [
        ('left_arm', 'left_forearm', 'Left Arm (Upper: Shoulder->Elbow)'),
        ('left_forearm', 'left_wrist', 'Left Forearm (Elbow->Wrist)'),
        ('left_wrist', None, 'Left Wrist (Wrist->HandTip)'),
        ('right_arm', 'right_forearm', 'Right Arm (Upper: Shoulder->Elbow)'),
        ('right_forearm', 'right_wrist', 'Right Forearm (Elbow->Wrist)'),
        ('right_wrist', None, 'Right Wrist (Wrist->HandTip)'),
        ('left_hip', 'left_knee', 'Left Thigh (Hip->Knee)'),
        ('left_knee', 'left_foot_child', 'Left Calf (Knee->Ankle)'),
        ('left_foot_child', None, 'Left Foot (Ankle->Toes)'),
        ('right_hip', 'right_knee', 'Right Thigh (Hip->Knee)'),
        ('right_knee', 'right_foot_child', 'Right Calf (Knee->Ankle)'),
        ('right_foot_child', None, 'Right Foot (Ankle->Toes)'),
        ('neck', None, 'Neck (Neck->Head)')
    ]
    
    def angle_between_vectors(v1, v2):
        import math
        dot = max(-1.0, min(1.0, v1.dot(v2)))
        return math.acos(dot) * 180 / math.pi
        
    for k_curr, k_next, label in debug_chains:
        t_name = target_key_bones.get(k_curr)
        t_next = target_key_bones.get(k_next) if k_next else None
        
        ref_info = ref_debug_info.get(k_curr)
        
        t_dir_str = "N/A"
        r_dir_str = "N/A"
        diff_angle = "N/A"
        
        if ref_info:
            r_dir = ref_info['dir']
            r_dir_str = f"({r_dir.x:.4f}, {r_dir.y:.4f}, {r_dir.z:.4f})"
            
        if t_name:
            t_dir = get_segment_direction(target_armature, t_name, t_next, is_pose=False)
            if t_dir:
                t_dir_str = f"({t_dir.x:.4f}, {t_dir.y:.4f}, {t_dir.z:.4f})"
                if ref_info and ref_info.get('dir'):
                    diff_angle = f"{angle_between_vectors(t_dir, ref_info['dir']):.1f}°"
        
        ref_bone_name = ref_info['name'] if ref_info else 'None'
        print(f"  {label} (Target: {t_name} | Ref: {ref_bone_name}):")
        print(f"    Target Segment Dir: {t_dir_str}")
        print(f"    Ref Segment Dir:    {r_dir_str}")
        if diff_angle != "N/A":
            print(f"    Direction Angle Difference: {diff_angle}")
    print("=====================================================================\n")

def stage_2_align_hands_feet_head(target_armature, target_bone_mapping, ref_key_bones, ref_segment_dirs):
    print("Executing Stage 2: Aligning hands, feet, head/neck orientations using segment vector matching...")
    
    target_key_bones = identify_key_bones(target_armature, target_bone_mapping)
    print(f"Target key bones identified: {target_key_bones}")
    
    # Categories to align (zipped in order: upper -> lower -> end)
    category_pairs = [
        # Left Arm: [UpperArm, LowerArm, Wrist]
        (
            [target_key_bones.get('left_arm'), target_key_bones.get('left_forearm'), target_key_bones.get('left_wrist')],
            [ref_key_bones.get('left_arm'), ref_key_bones.get('left_forearm'), ref_key_bones.get('left_wrist')]
        ),
        # Right Arm: [UpperArm, LowerArm, Wrist]
        (
            [target_key_bones.get('right_arm'), target_key_bones.get('right_forearm'), target_key_bones.get('right_wrist')],
            [ref_key_bones.get('right_arm'), ref_key_bones.get('right_forearm'), ref_key_bones.get('right_wrist')]
        ),
        # Left Leg: [Hip, Knee, Ankle, Toes]
        (
            [target_key_bones.get('left_hip'), target_key_bones.get('left_knee'), target_key_bones.get('left_foot'), target_key_bones.get('left_foot_child')],
            [ref_key_bones.get('left_hip'), ref_key_bones.get('left_knee'), ref_key_bones.get('left_foot'), 'ball_l']
        ),
        # Right Leg: [Hip, Knee, Ankle, Toes]
        (
            [target_key_bones.get('right_hip'), target_key_bones.get('right_knee'), target_key_bones.get('right_foot'), target_key_bones.get('right_foot_child')],
            [ref_key_bones.get('right_hip'), ref_key_bones.get('right_knee'), ref_key_bones.get('right_foot'), 'ball_r']
        ),
        # Neck: [Neck]
        (
            [target_key_bones.get('neck')],
            [ref_key_bones.get('neck')]
        )
    ]

    # Switch target armature to POSE mode
    bpy.context.view_layer.objects.active = target_armature
    bpy.ops.object.mode_set(mode='POSE')
    
    # Clear active pose transforms
    for pb in target_armature.pose.bones:
        pb.matrix_basis = mathutils.Matrix.Identity(4)
    bpy.context.view_layer.update()

    # Align bone chains top-down using segment vector matching (1 clean top-down pass)
    for _pass in range(1):
        for target_chain, ref_chain in category_pairs:
            t_chain = [b for b in target_chain if b]
            r_chain = [b for b in ref_chain if b]
            
            for i in range(min(len(t_chain), len(r_chain))):
                t_name = t_chain[i]
                r_name = r_chain[i]
                
                v_ref = ref_segment_dirs.get(r_name)
                if not v_ref:
                    continue
                    
                t_next = t_chain[i+1] if i + 1 < len(t_chain) else None
                v_target = get_segment_direction(target_armature, t_name, t_next, is_pose=True)
                
                if not v_target:
                    continue
                    
                # Compute minimal rotation difference to align target segment to reference segment
                Q = v_target.rotation_difference(v_ref)
                
                pb = target_armature.pose.bones.get(t_name)
                if pb:
                    target_pb = pb
                    loc, rot, scale = target_pb.matrix.decompose()
                    q_rot = Q
                    new_rot = q_rot @ rot
                    target_pb.matrix = mathutils.Matrix.Translation(loc) @ new_rot.to_matrix().to_4x4() @ mathutils.Matrix.Scale(1.0, 4, (1,0,0))
                    bpy.context.view_layer.update()

    # Apply pose as rest pose to both skin meshes and armature
    apply_pose_to_skin_and_armature(target_armature)
    print("Stage 2 vector orientation alignment complete.")
    return target_bone_mapping

def print_global_joint_coordinates(title, target_armature, target_key_bones, ref_armature=None, ref_key_bones=None):
    print(f"\n=================== GLOBAL JOINT COORDINATES: {title} ===================")
    
    # Key joints to output: elbow, knee, heel (ankle), wrist for left and right
    joint_specs = [
        ('Left Elbow', 'left_forearm', 'lowerarm_l'),
        ('Right Elbow', 'right_forearm', 'lowerarm_r'),
        ('Left Wrist', 'left_wrist', 'hand_l'),
        ('Right Wrist', 'right_wrist', 'hand_r'),
        ('Left Knee', 'left_knee', 'calf_l'),
        ('Right Knee', 'right_knee', 'calf_r'),
        ('Left Heel/Ankle', 'left_foot', 'foot_l'),
        ('Right Heel/Ankle', 'right_foot', 'foot_r'),
    ]
    
    for label, target_key, ref_key in joint_specs:
        t_bone_name = target_key_bones.get(target_key)
        t_pos_str = "N/A"
        if t_bone_name:
            # Check pose bone first, then fallback to rest bone
            pb = target_armature.pose.bones.get(t_bone_name) if target_armature.pose else None
            if pb:
                w_pos = target_armature.matrix_world @ pb.head
            else:
                b = target_armature.data.bones.get(t_bone_name)
                w_pos = target_armature.matrix_world @ b.head_local if b else None
            if w_pos:
                t_pos_str = f"({w_pos.x:+.4f}, {w_pos.y:+.4f}, {w_pos.z:+.4f})"
                
        r_pos_str = ""
        try:
            if ref_armature and hasattr(ref_armature, "matrix_world"):
                r_bone_name = ref_key_bones.get(ref_key) if ref_key_bones else ref_key
                rb = ref_armature.data.bones.get(r_bone_name) if r_bone_name else None
                if rb:
                    r_w_pos = ref_armature.matrix_world @ rb.head_local
                    r_pos_str = f" | Ref ({r_bone_name}): ({r_w_pos.x:+.4f}, {r_w_pos.y:+.4f}, {r_w_pos.z:+.4f})"
        except Exception:
            pass
                
        print(f"  {label:<16} (Target: {t_bone_name or 'N/A'}): {t_pos_str}{r_pos_str}")
    print("=========================================================================\n")


def render_stage2_preview(output_jpg_path):
    output_x_jpg_path = output_jpg_path.replace(".jpg", "_x.jpg")
    print(f"Rendering Stage 2 256x256 preview images to:\n  Front: {output_jpg_path}\n  Side:  {output_x_jpg_path}")
    
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_WORKBENCH'
    scene.render.resolution_x = 256
    scene.render.resolution_y = 256
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = 'JPEG'
    
    if hasattr(scene, 'display'):
        scene.display.shading.type = 'SOLID'
        scene.display.shading.light = 'STUDIO'
        
    # Camera 1: Front view (Y = -3.2, Z = 1.0 looking towards +Y)
    cam_front = bpy.data.objects.new('CamFront', bpy.data.cameras.new('CamFront'))
    scene.collection.objects.link(cam_front)
    cam_front.location = mathutils.Vector((0.0, -3.2, 1.0))
    cam_front.rotation_euler = mathutils.Euler((1.5708, 0, 0), 'XYZ')
    
    scene.camera = cam_front
    scene.render.filepath = output_jpg_path
    bpy.ops.render.render(write_still=True)
    
    # Camera 2: Side view from model's right side (X = -3.2, Y = 0, Z = 1.0 looking towards +X)
    cam_side = bpy.data.objects.new('CamSide', bpy.data.cameras.new('CamSide'))
    scene.collection.objects.link(cam_side)
    cam_side.location = mathutils.Vector((-3.2, 0.0, 1.0))
    cam_side.rotation_euler = mathutils.Euler((1.5708, 0, -1.5708), 'XYZ')
    
    scene.camera = cam_side
    scene.render.filepath = output_x_jpg_path
    bpy.ops.render.render(write_still=True)
    
    print("Stage 2 preview renders complete.")

def stage_3_apply_animations(target_armature, target_bone_mapping, ref_key_bones, ref_actions):
    print("Executing Stage 3: Applying and retargeting animations...")
    
    # Identify key bones in target model
    target_key_bones = identify_key_bones(target_armature, target_bone_mapping)
    print(f"Target key bones identified: {target_key_bones}")
    
    # Regular expression to parse bone name and property path
    import re
    pattern = re.compile(r'^pose\.bones\["([^"]+)"\]\.(.+)$')
    
    target_actions = []
    
    for ref_action in ref_actions:
        print(f"Retargeting animation: {ref_action.name}")
        # Duplicate action
        target_action = ref_action.copy()
        target_action.name = f"{ref_action.name}_retargeted"
        
        # Iterate over layers, strips, channelbags in target_action
        for layer in target_action.layers:
            for strip in layer.strips:
                for cb in strip.channelbags:
                    fcurves_to_remove = []
                    
                    for fc in cb.fcurves:
                        match = pattern.match(fc.data_path)
                        if match:
                            ref_bone_name = match.group(1)
                            prop = match.group(2)
                            
                            # Check if ref_bone_name maps to any of our key bones
                            found_key = None
                            for key, name in ref_key_bones.items():
                                if name == ref_bone_name:
                                    found_key = key
                                    break
                                    
                            if found_key and prop in ['rotation_quaternion', 'rotation_euler', 'rotation_axis_angle']:
                                t_bone_name = target_key_bones.get(found_key)
                                if t_bone_name:
                                    fc.data_path = f'pose.bones["{t_bone_name}"].{prop}'
                                else:
                                    fcurves_to_remove.append(fc)
                            else:
                                fcurves_to_remove.append(fc)
                        else:
                            fcurves_to_remove.append(fc)
                            
                    for fc in fcurves_to_remove:
                        cb.fcurves.remove(fc)
            
        # Push retargeted action to target armature's NLA tracks
        if not target_armature.animation_data:
            target_armature.animation_data_create()
            
        track = target_armature.animation_data.nla_tracks.new()
        track.name = target_action.name
        
        # Create strip
        start_frame = target_action.frame_range[0]
        track.strips.new(name=target_action.name, start=int(start_frame), action=target_action)
        target_actions.append(target_action)
        
    print(f"Retargeted {len(target_actions)} animations.")
    
    # Remove original non-retargeted actions and rename retargeted actions
    all_actions = list(bpy.data.actions)
    for act in all_actions:
        if not act.name.endswith("_retargeted"):
            bpy.data.actions.remove(act, do_unlink=True)
            
    for act in list(bpy.data.actions):
        if act.name.endswith("_retargeted"):
            clean_name = act.name[:-11]  # strip '_retargeted'
            act.name = clean_name
            
    if target_armature.animation_data:
        for track in target_armature.animation_data.nla_tracks:
            if track.name.endswith("_retargeted"):
                track.name = track.name[:-11]
            for strip in track.strips:
                if strip.name.endswith("_retargeted"):
                    strip.name = strip.name[:-11]
                    
    print("Action cleanup complete: original actions removed, retargeted actions renamed.")


def render_reference_animations(ref_glb_path, anim_names):
    ref_dir = os.path.join(os.path.dirname(os.path.abspath(ref_glb_path)), "ref")
    os.makedirs(ref_dir, exist_ok=True)
    ref_basename = os.path.splitext(os.path.basename(ref_glb_path))[0]
    
    needed_files = []
    for anim in anim_names:
        needed_files.append(os.path.join(ref_dir, f"{ref_basename}_{anim}_front.jpg"))
        needed_files.append(os.path.join(ref_dir, f"{ref_basename}_{anim}_side.jpg"))
        
    if all(os.path.exists(f) for f in needed_files):
        print(f"All reference animation renders for {ref_basename} already exist in {ref_dir}. Skipping reference render.")
        return
        
    print(f"Rendering reference animation preview images to {ref_dir}...")
    clear_scene()
    bpy.ops.import_scene.gltf(filepath=ref_glb_path)
    ref_armature = get_armature_object()
    if not ref_armature:
        print("Error: No armature found in reference model for anim rendering")
        return
        
    if not ref_armature.animation_data:
        ref_armature.animation_data_create()
        
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_WORKBENCH'
    scene.render.resolution_x = 256
    scene.render.resolution_y = 256
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = 'JPEG'
    
    if hasattr(scene, 'display'):
        scene.display.shading.type = 'SOLID'
        scene.display.shading.light = 'STUDIO'
        
    cam_front = bpy.data.objects.new('RefCamFront', bpy.data.cameras.new('RefCamFront'))
    scene.collection.objects.link(cam_front)
    cam_front.location = mathutils.Vector((0.0, -3.2, 1.0))
    cam_front.rotation_euler = mathutils.Euler((1.5708, 0, 0), 'XYZ')
    
    cam_side = bpy.data.objects.new('RefCamSide', bpy.data.cameras.new('RefCamSide'))
    scene.collection.objects.link(cam_side)
    cam_side.location = mathutils.Vector((-3.2, 0.0, 1.0))
    cam_side.rotation_euler = mathutils.Euler((1.5708, 0, -1.5708), 'XYZ')
    
    for anim_name in anim_names:
        action = bpy.data.actions.get(anim_name)
        if not action:
            print(f"Warning: Reference action '{anim_name}' not found.")
            continue
            
        ref_armature.animation_data.action = action
        scene.frame_set(1)
        bpy.context.view_layer.update()
        
        front_path = os.path.join(ref_dir, f"{ref_basename}_{anim_name}_front.jpg")
        scene.camera = cam_front
        scene.render.filepath = front_path
        bpy.ops.render.render(write_still=True)
        
        side_path = os.path.join(ref_dir, f"{ref_basename}_{anim_name}_side.jpg")
        scene.camera = cam_side
        scene.render.filepath = side_path
        bpy.ops.render.render(write_still=True)
        print(f"Saved reference renders for {anim_name} to {ref_dir}")
        
    print("Reference animation rendering complete.")


def render_and_debug_target_animations(target_armature, target_key_bones, out_dir, target_basename, anim_names):
    print("\n=================== TARGET ANIMATION RENDERS & DEBUG ===================")
    scene = bpy.context.scene
    scene.render.engine = 'BLENDER_WORKBENCH'
    scene.render.resolution_x = 256
    scene.render.resolution_y = 256
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = 'JPEG'
    
    if hasattr(scene, 'display'):
        scene.display.shading.type = 'SOLID'
        scene.display.shading.light = 'STUDIO'
        
    cam_front = bpy.data.objects.new('TargetCamFront', bpy.data.cameras.new('TargetCamFront'))
    scene.collection.objects.link(cam_front)
    cam_front.location = mathutils.Vector((0.0, -3.2, 1.0))
    cam_front.rotation_euler = mathutils.Euler((1.5708, 0, 0), 'XYZ')
    
    cam_side = bpy.data.objects.new('TargetCamSide', bpy.data.cameras.new('TargetCamSide'))
    scene.collection.objects.link(cam_side)
    cam_side.location = mathutils.Vector((-3.2, 0.0, 1.0))
    cam_side.rotation_euler = mathutils.Euler((1.5708, 0, -1.5708), 'XYZ')
    
    if not target_armature.animation_data:
        target_armature.animation_data_create()
        
    for anim_name in anim_names:
        action = bpy.data.actions.get(anim_name)
        if not action:
            print(f"Warning: Target action '{anim_name}' not found.")
            continue
            
        target_armature.animation_data.action = action
        scene.frame_set(1)
        bpy.context.view_layer.update()
        
        print(f"\n--- Body Part Orientations for Target Animation: {anim_name} ---")
        keys_to_debug = [
            'left_arm', 'left_forearm', 'left_wrist',
            'right_arm', 'right_forearm', 'right_wrist',
            'left_hip', 'left_knee', 'left_foot',
            'right_hip', 'right_knee', 'right_foot',
            'neck'
        ]
        for key in keys_to_debug:
            b_name = target_key_bones.get(key)
            if b_name:
                pb = target_armature.pose.bones.get(b_name)
                if pb:
                    w_head = target_armature.matrix_world @ pb.head
                    w_tail = target_armature.matrix_world @ pb.tail
                    dir_vec = (w_tail - w_head).normalized()
                    q_rot = pb.matrix.to_quaternion()
                    print(f"  {key:<14} ({b_name:<8}): Head=({w_head.x:+.3f}, {w_head.y:+.3f}, {w_head.z:+.3f}) | Dir=({dir_vec.x:+.3f}, {dir_vec.y:+.3f}, {dir_vec.z:+.3f}) | Quat=({q_rot.w:+.3f}, {q_rot.x:+.3f}, {q_rot.y:+.3f}, {q_rot.z:+.3f})")
                    
        front_path = os.path.join(out_dir, f"{target_basename}_{anim_name}_front.jpg")
        scene.camera = cam_front
        scene.render.filepath = front_path
        bpy.ops.render.render(write_still=True)
        
        side_path = os.path.join(out_dir, f"{target_basename}_{anim_name}_side.jpg")
        scene.camera = cam_side
        scene.render.filepath = side_path
        bpy.ops.render.render(write_still=True)
        print(f"Saved target renders for {anim_name}: {front_path} and {side_path}")
        
    print("=========================================================================\n")

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
    
    # Define Stage-specific GLB output paths
    output_stage_1_path = os.path.join(out_dir, f"{input_name_no_ext}_stage_1.glb")
    output_stage_2_path = os.path.join(out_dir, f"{input_name_no_ext}_stage_2.glb")
    output_stage_3_path = os.path.join(out_dir, f"{input_name_no_ext}_stage_3.glb")
    
    # 1. Process Reference GLB: Identify Bones & Animations
    clear_scene()
    print(f"Importing reference model: {ref_glb_path}")
    bpy.ops.import_scene.gltf(filepath=ref_glb_path)
    
    ref_armature = get_armature_object()
    if not ref_armature:
        print("Error: No armature found in reference model")
        sys.exit(1)
        
    # Apply reference transforms to make bone matrices absolute to origin
    apply_transforms_safe(ref_armature)
        
    print("Classifying reference model bones...")
    ref_joints = build_joints_list(ref_armature)
    ref_bone_mapping = classify_skeleton(ref_joints, ref_armature.name)
    
    # Save reference bone mapping to JSON
    with open(ref_bones_json_path, 'w') as f:
        json.dump(ref_bone_mapping, f, indent=2)
    print(f"Saved reference bones JSON to {ref_bones_json_path}")
    
    # Identify reference key bones and segment directions
    ref_key_bones = identify_key_bones(ref_armature, ref_bone_mapping)
    ref_key_matrices = {}
    ref_debug_info = {}
    ref_segment_dirs = {}
    
    # Categories for capturing reference segment directions
    ref_categories = [
        [ref_key_bones.get('left_arm'), ref_key_bones.get('left_forearm'), ref_key_bones.get('left_wrist')],
        [ref_key_bones.get('right_arm'), ref_key_bones.get('right_forearm'), ref_key_bones.get('right_wrist')],
        [ref_key_bones.get('left_hip'), ref_key_bones.get('left_knee'), ref_key_bones.get('left_foot'), 'ball_l'],
        [ref_key_bones.get('right_hip'), ref_key_bones.get('right_knee'), ref_key_bones.get('right_foot'), 'ball_r'],
        [ref_key_bones.get('neck'), 'Head']
    ]
    
    for chain in ref_categories:
        c_chain = [b for b in chain if b]
        for i in range(len(c_chain)):
            b_name = c_chain[i]
            b_next = c_chain[i+1] if i + 1 < len(c_chain) else None
            v_dir = get_segment_direction(ref_armature, b_name, b_next, is_pose=False)
            if v_dir:
                ref_segment_dirs[b_name] = v_dir
                
    for key, b_name in ref_key_bones.items():
        if b_name:
            bone = ref_armature.data.bones.get(b_name)
            if bone:
                ref_key_matrices[key] = bone.matrix_local.copy()
                r_dir = ref_segment_dirs.get(b_name) or (bone.tail_local - bone.head_local).normalized()
                ref_debug_info[key] = {
                    'name': b_name,
                    'dir': r_dir,
                    'rot': bone.matrix_local.to_euler()
                }
                
    # Capture and protect animations (Actions) from the reference model
    ref_actions = list(bpy.data.actions)
    for action in ref_actions:
        action.use_fake_user = True
    print(f"Protected {len(ref_actions)} reference actions.")
    
    # Render reference animation preview images if they do not exist
    render_reference_animations(ref_glb_path, ['Idle_Loop', 'Crouch_Idle_Loop', 'Jog_Fwd_Loop', 'Walk_Loop'])
    
    # 2. Process Input GLB: Stage 1 (Rotate, Ground, Scale)
    clear_scene()
    print(f"Importing input model: {input_glb_path}")
    bpy.ops.import_scene.gltf(filepath=input_glb_path)
    
    target_armature = get_armature_object()
    if not target_armature:
        print("Error: No armature found in input model")
        sys.exit(1)
        
    # Execute stage_1_rotate_model pipeline
    target_bone_mapping = stage_1_rotate_model(target_armature)
    
    # Save target bone mapping to JSON
    with open(flag_bones_json_path, 'w') as f:
        json.dump(target_bone_mapping, f, indent=2)
    print(f"Saved target bones JSON to {flag_bones_json_path}")
    
    # Identify target key bones
    target_key_bones = identify_key_bones(target_armature, target_bone_mapping)
        
    # Export Stage 1 GLB
    print(f"Exporting Stage 1 model to: {output_stage_1_path}")
    bpy.ops.object.select_all(action='DESELECT')
    for obj in bpy.data.objects:
        if obj.type in ['ARMATURE', 'MESH']:
            obj.select_set(True)
    bpy.ops.export_scene.gltf(filepath=output_stage_1_path, use_selection=True)
    print("Stage 1 export complete.")
    
    # Print debug data BEFORE Stage 2 alignment
    print_bone_debug_data("BEFORE STAGE 2 ALIGNMENT", target_armature, target_key_bones, ref_debug_info)
    print_global_joint_coordinates("BEFORE STAGE 2 ALIGNMENT", target_armature, target_key_bones, ref_armature, ref_key_bones)
    
    # 3. Process Stage 2 (Align hand/foot/neck orientations)
    target_bone_mapping = stage_2_align_hands_feet_head(target_armature, target_bone_mapping, ref_key_bones, ref_segment_dirs)
    
    # Identify target key bones after alignment
    target_key_bones_after = identify_key_bones(target_armature, target_bone_mapping)
    
    # Print debug data AFTER Stage 2 alignment
    print_bone_debug_data("AFTER STAGE 2 ALIGNMENT", target_armature, target_key_bones_after, ref_debug_info)
    print_global_joint_coordinates("AFTER STAGE 2 ALIGNMENT", target_armature, target_key_bones_after, ref_armature, ref_key_bones)
    
    # Export Stage 2 GLB
    print(f"Exporting Stage 2 model to: {output_stage_2_path}")
    bpy.ops.object.select_all(action='DESELECT')
    for obj in bpy.data.objects:
        if obj.type in ['ARMATURE', 'MESH']:
            obj.select_set(True)
    bpy.ops.export_scene.gltf(filepath=output_stage_2_path, use_selection=True)
    print("Stage 2 export complete.")
    
    # Render Stage 2 preview image (256x256)
    output_stage_2_jpg_path = os.path.join(out_dir, f"{input_name_no_ext}_stage_2.jpg")
    render_stage2_preview(output_stage_2_jpg_path)
    
    # 4. Process Stage 3 (Apply reference animations to target)
    stage_3_apply_animations(target_armature, target_bone_mapping, ref_key_bones, ref_actions)
    
    # Export Stage 3 GLB
    print(f"Exporting Stage 3 model to: {output_stage_3_path}")
    bpy.ops.object.select_all(action='DESELECT')
    for obj in bpy.data.objects:
        if obj.type in ['ARMATURE', 'MESH']:
            obj.select_set(True)
    bpy.ops.export_scene.gltf(filepath=output_stage_3_path, use_selection=True)
    print("Stage 3 export complete.")
    
    # Render and debug target animations post-Stage 3
    render_and_debug_target_animations(target_armature, target_key_bones_after, out_dir, input_name_no_ext, ['Idle_Loop', 'Crouch_Idle_Loop', 'Jog_Fwd_Loop', 'Walk_Loop'])
    
    clear_scene()
    print("Done!")

if __name__ == '__main__':
    main()
