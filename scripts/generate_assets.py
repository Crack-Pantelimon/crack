import os
import sys
import json
import argparse
import subprocess

MANIFEST_PATH = "/home/vasile/.gemini/antigravity/scratch/crack/_data/blender_generated/manifest.json"
REPO_ROOT = "/home/vasile/.gemini/antigravity/scratch/crack"

def load_manifest():
    if not os.path.exists(MANIFEST_PATH):
        raise FileNotFoundError(f"Manifest not found at {MANIFEST_PATH}")
    with open(MANIFEST_PATH, "r") as f:
        return json.load(f)

def run_generator(asset):
    name = asset["name"]
    script_rel = asset["script"]
    image_rel = asset["input_image"]
    glb_rel = asset["glb_path"]
    
    script_path = os.path.join(REPO_ROOT, script_rel)
    image_path = os.path.join(REPO_ROOT, image_rel)
    output_dir = os.path.dirname(os.path.join(REPO_ROOT, glb_rel))
    
    print(f"\n======================================================================")
    print(f"[GATEWAY] Building Asset: {name} (Version {asset.get('version', '1.0.0')})")
    print(f"[GATEWAY] Description: {asset.get('description', '')}")
    print(f"[GATEWAY] Running script: {script_rel}")
    print(f"[GATEWAY] Using input texture: {image_rel}")
    print(f"[GATEWAY] Destination directory: {os.path.relpath(output_dir, REPO_ROOT)}")
    print(f"======================================================================")
    
    if not os.path.exists(script_path):
        print(f"Error: Generator script not found at {script_path}")
        return False
        
    if not os.path.exists(image_path):
        print(f"Error: Input texture image not found at {image_path}")
        return False
        
    # Map asset names to their script-specific texture argument flags
    texture_flags = {
        "bus_335": "--bus-texture",
        "kebab_shop": "--kebab-texture",
        "superbet_shop": "--superbet-texture",
        "terasa_obor": "--obor-texture"
    }
    texture_flag = texture_flags.get(name, "--texture")
    
    cmd = [
        "blender",
        "--background",
        "--python", script_path,
        "--",
        "--output-dir", output_dir,
        texture_flag, image_path
    ]
    
    try:
        subprocess.run(cmd, check=True)
        print(f"[GATEWAY] Asset {name} built successfully!")
        return True
    except subprocess.CalledProcessError as e:
        print(f"[GATEWAY] Error: Blender failed to build asset {name}: {e}")
        return False

def main():
    parser = argparse.ArgumentParser(description="Blender asset generation gateway manager.")
    parser.add_argument("--model", type=str, default="all", help="Which asset to build (defaults to 'all')")
    args = parser.parse_args()
    
    try:
        manifest = load_manifest()
    except Exception as e:
        print(f"Error loading manifest: {e}")
        sys.exit(1)
        
    assets = manifest.get("assets", [])
    if not assets:
        print("No assets registered in manifest.")
        return
        
    to_build = []
    if args.model == "all":
        to_build = assets
    else:
        # Find matching asset in manifest
        match = [a for a in assets if a["name"] == args.model]
        if not match:
            print(f"Error: No asset named '{args.model}' registered in the manifest.")
            print(f"Available assets: {', '.join([a['name'] for a in assets])}")
            sys.exit(1)
        to_build = match

    success_count = 0
    for asset in to_build:
        if run_generator(asset):
            success_count += 1
            
    print(f"\n======================================================================")
    print(f"[GATEWAY] Generation completed. Successfully built {success_count}/{len(to_build)} assets.")
    print(f"======================================================================")
    
    if success_count < len(to_build):
        sys.exit(1)

if __name__ == "__main__":
    main()
