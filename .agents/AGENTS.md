# Antigravity Rules for GTA Vice City: Pantelimon (Crack Project)

This file contains guidelines and context rules loaded automatically by Antigravity whenever working in this repository.

---

## 1. Development Servers & Building
- **Do not restart `trunk serve` / `start_game_web.sh` repeatedly**: Trunk has a built-in file watcher and hot-reloads/recompiles automatically when any Rust files change. Keep it running in the background.
- **Local Asset Server (`1973`)**: The game client streams 3D map meshes dynamically over HTTP from port `1973`. Always keep `python3 scripts/local_server.py` running to serve the `_data/` folder.
- **Path normalization**: Be aware that the Bevy client may request paths with double leading slashes (e.g. `//3d_data_v2/data_out/...`). `local_server.py` handles this by stripping leading slashes before routing.
- **sccache Support**: Compilation scripts (`start_game_native.sh` and `start_game_web.sh`) conditionally use `sccache` if installed on the system to cache library builds. Propose installing `sccache` to users facing long rebuild times.

---

## 2. Virtual Machine (Host/Guest) Context
- The development sandbox runs inside a CachyOS Guest VM.
- To play the game with full GPU acceleration, the user accesses it from their physical Host browser.
- **Firewall settings**: UFW is active in the VM. Ports `8080` (Trunk) and `1973` (local server) must be open. The user should run `sudo ufw allow 8080/tcp` and `sudo ufw allow 1973/tcp` inside the VM.
- **Host access URL**: The guest VM IP is `192.168.122.237`. The host browser can navigate to `http://192.168.122.237:8080` (or `http://localhost:8080` if port-forwarding is active).

---

## 3. Map Coordinates & Coordinate Shifts
- The `3d_data_v2` map was generated from Google Earth ECEF coordinates flat-projected to local ENU (East, North, Up) coordinates relative to reference point `Lat: 44.445522`, `Lon: 26.142436` (Cora Pantelimon).
- **Coordinate system mapping in Bevy**: Blender's GLTF exporter automatically converts Blender Z-up to GLTF Y-up. The resulting mapping parsed from Parquet is:
  - Bevy X (East) = `x_min`
  - Bevy Y (Height) = `z_min`
  - Bevy Z (-North) = `-y_max`
- **Missions Config Translation**: The coordinates in `missions_config.json` use the old offset from `3d_data`. The new `3d_data_v2` map is shifted. The runtime translation to match the new map is:
  - `New_X = X - 797.55`
  - `New_Y_height = Y_height - 20637.90`
  - `New_Z_bevy = -Z_north - 3310.80`
  This translation is automatically applied in `load_missions_config` at load time.


