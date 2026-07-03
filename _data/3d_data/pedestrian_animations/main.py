import os
import glob
import subprocess
import sys

def main():
    # Resolve relative paths starting from this script's directory
    base_dir = os.path.dirname(os.path.abspath(__file__))
    
    ref_glb = os.path.join(base_dir, "UAL1_Standard.glb")
    input_dir = os.path.join(base_dir, "..", "pedestrian_3d_gen", "3d_with_skeleton", "round1")
    out_dir = os.path.join(base_dir, "out2")
    blender_script = os.path.join(base_dir, "blender_normalize_skel.py")
    
    print(f"Base Directory: {base_dir}")
    print(f"Reference GLB: {ref_glb}")
    print(f"Input Directory: {input_dir}")
    print(f"Output Directory: {out_dir}")
    print(f"Blender Script: {blender_script}")
    
    if not os.path.exists(ref_glb):
        print(f"Error: Reference GLB not found at {ref_glb}")
        sys.exit(1)
        
    if not os.path.exists(input_dir):
        print(f"Error: Input directory not found at {input_dir}")
        sys.exit(1)
        
    if not os.path.exists(blender_script):
        print(f"Error: Blender script not found at {blender_script}")
        sys.exit(1)
        
    # Get all GLB files in input_dir
    glb_files = sorted(glob.glob(os.path.join(input_dir, "*.glb")))
    if not glb_files:
        print("No GLB files found in input directory.")
        sys.exit(0)
        
    print(f"Found {len(glb_files)} GLB files to process.")
    
    for idx, glb_file in enumerate(glb_files, 1):
        print("\n" + "="*80)
        print(f"Processing file {idx}/{len(glb_files)}: {os.path.basename(glb_file)}")
        print("="*80)
        
        # Build command: blender --background --python <script> -- <ref_glb> <input_glb> <out_dir>
        cmd = [
            "blender",
            "--background",
            "--python", blender_script,
            "--",
            ref_glb,
            glb_file,
            out_dir
        ]
        
        try:
            # Run the command and capture output
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            print(result.stdout)
        except subprocess.CalledProcessError as e:
            print(f"Error executing Blender on {glb_file}:")
            print(e.stderr)
            print(e.stdout)
            sys.exit(1)
            
    print("\nAll files processed successfully!")

if __name__ == '__main__':
    main()
