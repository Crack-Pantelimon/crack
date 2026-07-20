#!/usr/bin/env python3
"""
Calculate total size of files in data_cache directory in KB and print.
"""

import os
from pathlib import Path

def main():
    script_dir = Path(__file__).parent
    cache_dir = script_dir / "data_cache"
    
    if not cache_dir.is_dir():
        print(f"Error: {cache_dir} does not exist or is not a directory")
        return 1
    
    total_size = 0
    for dirpath, dirnames, filenames in os.walk(cache_dir):
        for f in filenames:
            fp = os.path.join(dirpath, f)
            if os.path.isfile(fp):
                total_size += os.path.getsize(fp)
    
    total_kb = total_size / 1024.0
    print(f"Total size: {total_kb:.2f} KB")
    return 0

if __name__ == "__main__":
    exit(main())