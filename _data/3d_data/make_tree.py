#!/usr/bin/env python3
import os
import json
import math
import sys
import pyarrow as pa
import pyarrow.parquet as pq


class BVHNode:
    def __init__(self, name, level, objects=None, children=None, aabb=None):
        self.name = name
        self.level = level
        self.objects = objects or []
        self.children = children or []
        self.aabb = aabb
        self.octant_path = []


def union_aabbs(aabbs):
    if not aabbs:
        return None
    return {
        "minx": min(box["minx"] for box in aabbs),
        "maxx": max(box["maxx"] for box in aabbs),
        "miny": min(box["miny"] for box in aabbs),
        "maxy": max(box["maxy"] for box in aabbs),
        "minz": min(box["minz"] for box in aabbs),
        "maxz": max(box["maxz"] for box in aabbs),
    }


def get_aabb_center(aabb):
    return (
        (aabb["minx"] + aabb["maxx"]) / 2.0,
        (aabb["miny"] + aabb["maxy"]) / 2.0,
        (aabb["minz"] + aabb["maxz"]) / 2.0,
    )


def distance(c1, c2):
    return math.sqrt((c1[0] - c2[0]) ** 2 + (c1[1] - c2[1]) ** 2 + (c1[2] - c2[2]) ** 2)


def update_aabbs(node):
    boxes = [obj["aabb"] for obj in node.objects]
    for child in node.children:
        update_aabbs(child)
        if child.aabb:
            boxes.append(child.aabb)
    node.aabb = union_aabbs(boxes)


def validate_tree(node, parent_aabb=None):
    # Assert AABB validity
    assert node.aabb is not None, f"Node {node.name} has null AABB"
    assert node.aabb["minx"] <= node.aabb["maxx"], (
        f"Node {node.name} invalid X bounds: {node.aabb['minx']} > {node.aabb['maxx']}"
    )
    assert node.aabb["miny"] <= node.aabb["maxy"], (
        f"Node {node.name} invalid Y bounds: {node.aabb['miny']} > {node.aabb['maxy']}"
    )
    assert node.aabb["minz"] <= node.aabb["maxz"], (
        f"Node {node.name} invalid Z bounds: {node.aabb['minz']} > {node.aabb['maxz']}"
    )

    # Assert containment in parent's AABB
    if parent_aabb is not None:
        eps = 1e-3  # Floating point epsilon
        assert node.aabb["minx"] >= parent_aabb["minx"] - eps, (
            f"Node {node.name} minx out of parent bounds"
        )
        assert node.aabb["maxx"] <= parent_aabb["maxx"] + eps, (
            f"Node {node.name} maxx out of parent bounds"
        )
        assert node.aabb["miny"] >= parent_aabb["miny"] - eps, (
            f"Node {node.name} miny out of parent bounds"
        )
        assert node.aabb["maxy"] <= parent_aabb["maxy"] + eps, (
            f"Node {node.name} maxy out of parent bounds"
        )
        assert node.aabb["minz"] >= parent_aabb["minz"] - eps, (
            f"Node {node.name} minz out of parent bounds"
        )
        assert node.aabb["maxz"] <= parent_aabb["maxz"] + eps, (
            f"Node {node.name} maxz out of parent bounds"
        )

    # Assert containment of all direct objects in node's AABB
    for obj in node.objects:
        obj_box = obj["aabb"]
        eps = 1e-3
        assert obj_box["minx"] >= node.aabb["minx"] - eps, (
            f"Object {obj['name']} minx out of node bounds"
        )
        assert obj_box["maxx"] <= node.aabb["maxx"] + eps, (
            f"Object {obj['name']} maxx out of node bounds"
        )
        assert obj_box["miny"] >= node.aabb["miny"] - eps, (
            f"Object {obj['name']} miny out of node bounds"
        )
        assert obj_box["maxy"] <= node.aabb["maxy"] + eps, (
            f"Object {obj['name']} maxy out of node bounds"
        )
        assert obj_box["minz"] >= node.aabb["minz"] - eps, (
            f"Object {obj['name']} minz out of node bounds"
        )
        assert obj_box["maxz"] <= node.aabb["maxz"] + eps, (
            f"Object {obj['name']} maxz out of node bounds"
        )

    num_nodes = 1
    num_meshes = len(node.objects)
    max_depth = 1

    for child in node.children:
        c_nodes, c_meshes, c_depth = validate_tree(child, parent_aabb=node.aabb)
        num_nodes += c_nodes
        num_meshes += c_meshes
        max_depth = max(max_depth, c_depth + 1)

    return num_nodes, num_meshes, max_depth


def main():
    script_dir = os.path.dirname(os.path.abspath(__file__))
    print(f"Running make_tree.py in directory: {script_dir}")

    # 1. Parse all _list.json files
    level_nodes = {0: {}, 1: {}, 2: {}, 3: {}, 4: {}}
    total_parsed_objects = 0
    total_skipped_objects = 0

    for i in [0, 1, 2, 3, 4]:
        list_path = os.path.join(script_dir, f"lod_0{i}", "_list.json")
        if not os.path.exists(list_path):
            print(f"Warning: {list_path} not found. Skipping level {i}.")
            continue

        with open(list_path, "r", encoding="utf-8") as f:
            data = json.load(f)

        for item in data:
            if item.get("vertex_count", 0) == 0:
                total_skipped_objects += 1
                continue

            name = item["name"]
            # Extract the base name (octant coordinates string representation)
            base_name = name.rsplit("_", 1)[0]

            if base_name not in level_nodes[i]:
                octant_path = [int(d) for d in base_name if d.isdigit()]
                level_nodes[i][base_name] = BVHNode(
                    name=base_name, level=i, objects=[], children=[], aabb=None
                )
                level_nodes[i][base_name].octant_path = octant_path

            # Filename path relative to the script directory
            relative_filename = f"lod_0{i}/{item['filename']}"

            level_nodes[i][base_name].objects.append(
                {
                    "name": name,
                    "filename": relative_filename,
                    "vertex_count": item["vertex_count"],
                    "aabb": item["aabb"],
                }
            )
            total_parsed_objects += 1

    print(
        f"Parsed {total_parsed_objects} valid objects, skipped {total_skipped_objects} empty geometry objects."
    )

    # 2. Build octree hierarchy links (0 -> 1 -> 2 -> 3 -> 4)
    for L in [0, 1, 2, 3]:
        for child_name, child_node in list(level_nodes[L].items()):
            parent_name = child_name[:-1]
            if not parent_name:
                continue

            if parent_name not in level_nodes[L + 1]:
                parent_octant_path = [int(d) for d in parent_name if d.isdigit()]
                parent_node = BVHNode(
                    name=parent_name, level=L + 1, objects=[], children=[], aabb=None
                )
                parent_node.octant_path = parent_octant_path
                level_nodes[L + 1][parent_name] = parent_node

            level_nodes[L + 1][parent_name].children.append(child_node)

    # 3. Update AABBs bottom-up from level 1 to level 4
    for node in level_nodes[4].values():
        update_aabbs(node)

    print(f"LOD level hierarchy built. Level 4 nodes: {len(level_nodes[4])}")

    # 4. Perform bottom-up clustering above level 4 to obtain a single root
    active_nodes = [node for node in level_nodes[4].values() if node.aabb is not None]

    if not active_nodes:
        print("Error: No nodes with valid bounding boxes found. Cannot build BVH tree.")
        sys.exit(1)

    virtual_id = 0
    while len(active_nodes) > 1:
        # Find the two closest nodes by AABB center distance
        centers = [get_aabb_center(node.aabb) for node in active_nodes]
        min_dist = float("inf")
        best_pair = (0, 1)

        n = len(active_nodes)
        for i in range(n):
            for j in range(i + 1, n):
                dist = distance(centers[i], centers[j])
                if dist < min_dist:
                    min_dist = dist
                    best_pair = (i, j)

        i, j = best_pair
        node_a = active_nodes[i]
        node_b = active_nodes[j]

        # Merge node_a and node_b
        parent_aabb = union_aabbs([node_a.aabb, node_b.aabb])
        parent_level = max(node_a.level, node_b.level) + 1
        parent_name = f"virtual_node_{virtual_id}"
        virtual_id += 1

        parent_node = BVHNode(
            name=parent_name,
            level=parent_level,
            objects=[],
            children=[node_a, node_b],
            aabb=parent_aabb,
        )

        # Determine common octant path prefix
        if hasattr(node_a, "octant_path") and hasattr(node_b, "octant_path"):
            common = []
            for x, y in zip(node_a.octant_path, node_b.octant_path):
                if x == y:
                    common.append(x)
                else:
                    break
            parent_node.octant_path = common
        else:
            parent_node.octant_path = []

        # Pop larger index first to preserve correct indexing
        active_nodes.pop(j)
        active_nodes.pop(i)
        active_nodes.append(parent_node)

    root = active_nodes[0]
    print(
        f"Top-level clustering completed. Root name: {root.name}, final tree levels: {root.level}"
    )

    # 5. Validate the completed tree
    print("Validating BVH tree invariants...")
    try:
        nodes_count, meshes_count, max_depth = validate_tree(root)
        print("Validation Successful!")
        print(
            f"Tree Stats: Total Nodes={nodes_count}, Total Meshes={meshes_count}, Max Depth={max_depth}"
        )
        if meshes_count != total_parsed_objects:
            print(
                f"Warning: Mesh count in tree ({meshes_count}) does not match parsed objects count ({total_parsed_objects})"
            )
    except AssertionError as e:
        print(f"Validation Failed: {e}")
        sys.exit(1)

    # 6. Serialize and write the flat Parquet files next to the script
    nodes_parquet_path = os.path.join(script_dir, "tree_nodes.parquet")
    children_parquet_path = os.path.join(script_dir, "tree_children.parquet")
    print(
        f"Writing Parquet outputs to:\n  {nodes_parquet_path}\n  {children_parquet_path}"
    )

    nodes_rows = []
    children_rows = []

    def collect_data(node):
        path_str = "".join(str(d) for d in getattr(node, "octant_path", []))
        nodes_rows.append(
            {
                "name": node.name,
                "type": "node",
                "level": node.level,
                "minx": node.aabb["minx"],
                "maxx": node.aabb["maxx"],
                "miny": node.aabb["miny"],
                "maxy": node.aabb["maxy"],
                "minz": node.aabb["minz"],
                "maxz": node.aabb["maxz"],
                "octant_path": path_str,
                "filename": None,
                "vertex_count": None,
            }
        )

        for obj in node.objects:
            nodes_rows.append(
                {
                    "name": obj["name"],
                    "type": "mesh",
                    "level": None,
                    "minx": obj["aabb"]["minx"],
                    "maxx": obj["aabb"]["maxx"],
                    "miny": obj["aabb"]["miny"],
                    "maxy": obj["aabb"]["maxy"],
                    "minz": obj["aabb"]["minz"],
                    "maxz": obj["aabb"]["maxz"],
                    "octant_path": None,
                    "filename": obj["filename"],
                    "vertex_count": obj["vertex_count"],
                }
            )
            children_rows.append({"parent_name": node.name, "child_name": obj["name"]})

        for child in node.children:
            children_rows.append({"parent_name": node.name, "child_name": child.name})
            collect_data(child)

    collect_data(root)

    # Write tree_nodes.parquet
    table_nodes = pa.Table.from_pylist(sorted(nodes_rows, key=lambda k: list(k.values())))
    pq.write_table(table_nodes, nodes_parquet_path)

    # Write tree_children.parquet
    table_children = pa.Table.from_pylist(sorted(children_rows, key=lambda k: list(k.values())))
    pq.write_table(table_children, children_parquet_path)

    # Remove old CSV files if they exist
    for filename in ["tree_nodes.csv", "tree_children.csv"]:
        path = os.path.join(script_dir, filename)
        if os.path.exists(path):
            try:
                os.remove(path)
                print(f"Removed old {filename}")
            except Exception as e:
                print(f"Warning: Could not remove old {filename}: {e}")

    print("make_tree.py finished successfully.")


if __name__ == "__main__":
    main()
