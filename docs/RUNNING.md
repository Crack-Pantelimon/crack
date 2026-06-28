# Running GTA Vice City: Pantelimon

This guide outlines how to compile, host, and run the GTA Vice City: Pantelimon Bevy client on both native and browser platforms.

## Prerequisites

Ensure you have Rust, Cargo, and Trunk installed.
Also install the required Linux graphics/audio dependencies (e.g. for CachyOS / Arch Linux):
```bash
sudo pacman -S libx11 pkgconf alsa-lib libxcursor libxrandr libxi mesa vulkan-intel
```

---

## 1. Syncing 3D Map Data Assets

The 3D map data and Parquet tiles (about 2GB) must be downloaded from the asset server. Run the downloader script in the root directory:
```bash
./download_data.sh
```
This script recursively downloads the standard assets and the `3d_data_v2` textures, saving them under the gitignored `_data/` directory. (It automatically skips already downloaded files).

---

## 2. Launching the game

### Method A: Web Browser Mode (Trunk Serve)

This runs the client as a WebAssembly application in your browser.

1. **Start the local asset server**:
   The Bevy client fetches map models and textures over local HTTP. Start the Python server from the root directory to host the downloaded assets:
   ```bash
   python3 scripts/local_server.py
   ```
   This server listens on port `1973` with wildcard CORS headers enabled so the browser client can request assets.

2. **Start the Trunk server**:
   In a separate terminal, navigate to the Bevy client directory and start the Trunk server:
   ```bash
   cd crack_demo/demo_resolution_selector_web_bevy
   trunk serve
   ```

3. **Play in Browser**:
   Open **`http://localhost:8080`** in your web browser. Progress is saved automatically using browser-native HTML5 LocalStorage.

---

### Method B: Native Desktop Mode

This runs the client as a native desktop binary.

1. **Start the local asset server**:
   The native client still streams map files over localhost HTTP. Start the asset server from the root directory:
   ```bash
   python3 scripts/local_server.py
   ```

2. **Run the game**:
   Run the launcher script from the root directory:
   ```bash
   ./start_game_native.sh
   ```
   Progress is persisted locally in the `post3.db` SQLite database file.
