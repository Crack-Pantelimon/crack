#!/bin/bash
set -e

# Make sure target directory exists
mkdir -p _data

echo "Downloading 3D map data recursively from https://pantelimon.alt-f4.ro/3d_data/..."
echo "This will skip already existing files and fetch the remaining assets."

# Wget options:
# -r: recursive download
# -np: no-parent (prevent ascending to parent directories)
# -nH: no host directory (do not create pantelimon.alt-f4.ro folder)
# --cut-dirs=0: preserve folder structure under 3d_data/
# -N: timestamping (retrieve only files newer than local or missing ones)
# -R "index.html*": reject auto-generated server index pages
# -P _data: output directory prefix
wget -r -np -nH --cut-dirs=0 -N -R "index.html*" -P _data https://pantelimon.alt-f4.ro/3d_data/

echo "Downloading 3D map data v2 recursively from https://pantelimon.alt-f4.ro/3d_data_v2/..."
wget -r -np -nH --cut-dirs=0 -N -R "index.html*" -P _data https://pantelimon.alt-f4.ro/3d_data_v2/

echo "3D data download completed successfully!"
