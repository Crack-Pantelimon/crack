
○
Install Blender and MCP Server Setup

ID: 1784450256833_blender_mcp • Created: 2026-07-19 08:37 • Modified: 2026-07-19 08:42

← All tasks
Prompt files
prompt.md
639 bytes • 2026-07-19 08:42
Install blender inside the container (you are root, just find the blender deb for latest version 5.1.* and install that)

when you have the correct download location , add that info to /workspace/_docker/Dockerfile so we have it in next containers also. 

Then install and set up using the next http port down, the blender mcp server . Configure it in the root dir /workspace the same as the other mcp servers . Blender will be running headless in the container, and the mcp should be able to control it . Verify that the mcp works by creating a blender file, replacing the basic cube with a sphere, and saving as /workspace/tmp/test.blend
Add Prompt
Explore
Plan
Plan Review Implementation Implementation Review Finished
Type	Path / command	Size
think	
I will act as a planning agent during a read-only exploration phase.
	out 463 · 17.3s
text	
I'll explore the Docker/MCP setup and Blender-related paths so the plan is gr…
	out 157 · 17.3s
bash	

cd /workspace && sigmap ask "How are MCP servers configured in Docker and where is blender or blender-mcp set up?" && sigmap --query "docker mcp blender dockerfile cont_start"

output

	in 205 / out 1.8k · 0.0s
Type	Path / command	Size
think	Reading the context file, Docker files, and MCP configurations.	out 63 · 1.3s
read	/workspace/.context/query-context.md
output

	in 48 / out 416 · 0.0s
read	/workspace/_docker/Dockerfile
output

	in 41 / out 206 · 0.0s
read	/workspace/_docker/_cont_start.sh
output

	in 45 / out 891 · 0.0s
Type	Path / command	Size
read	/workspace/_docker/run.sh
output

	in 37 / out 478 · 0.0s
Type	Path / command	Size
read	/workspace/_docker/build.sh
output

	in 39 / out 98 · 0.0s
Type	Path / command	Size
think	
Cursor shell: cd /workspace && rg -n -i "mcp|blender|9876|9847|port" _docker/…
	out 255 · 3.3s
bash	

cd /workspace && rg -n -i "mcp|blender|9876|9847|port" _docker/ .pi/ .cursor/ mcp.json .mcp.json 2>/dev/null; ls -la /workspace/.pi/ 2>/dev/null; ls -la /workspace/ | head -50; fd -t f -i 'mcp' /workspace --max-depth 3 2>/dev/null | head -40; ls -la /workspace/.cursor/ 2>/dev/null; ls /workspace/tmp 2>/dev/null; which blender 2>/dev/null; ls /opt/blender* 2>/dev/null; ls /root/.config/blender 2>/dev/null

output

	in 439 / out 24.0k · 0.0s
Type	Path / command	Size
think	
The ripgrep output was too large. I will narrow the search.
	out 559 · 4.0s
read	/workspace/.mcp.json
output

	in 32 / out 519 · 0.0s
bash	

cd /workspace && ls -la .mcp.json _docker/README.md 2>/dev/null; ls -la /opt/ 2>/dev/null | head -30; ls -la /root/.config/blender/ 2>/dev/null; ls /root/.config/mcp/ 2>/dev/null; which blender; blender --version 2>&1 | head -5; ls /workspace/tmp 2>/dev/null; ls /workspace/.pi/crack/tasks/1784450256833_blender_mcp/ 2>/dev/null; head -n 80 /workspace/.pi/crack/tasks/1784450256833_blender_mcp/explore.json 2>/dev/null; ls /workspace/.agents/skills/take-screenshot-using-browser/ 2>/dev/null

output

	in 521 / out 16.8k · 0.0s
Type	Path / command	Size
think	
Cursor find: _docker/Dockerfile _docker/README.md _docker/tcp_forward.py _doc…
	out 1.0k · 6.6s
text	
Next I'll inspect the blender-mcp install layout and how existing MCP servers…
	out 139 · 6.6s
bash	

cd /workspace && ls -la /opt/blender/ /opt/blender-5.1.2-linux-x64/ 2>/dev/null | head -40; ls -la /usr/local/bin/blender; readlink -f /usr/local/bin/blender; ls -la /root/.config/blender/5.1/scripts/addons/ 2>/dev/null; find /root -name '*blender*mcp*' 2>/dev/null | head -20; find /opt /root -name 'blender_mcp*' 2>/dev/null | head -20; ls /workspace/tmp 2>/dev/null; dpkg -l | rg -i 'xvfb|x11|wayland' | head -20; which xvfb-run; blender --help 2>&1 | rg -i 'background|window|display|x11|wayland' | head -30

output

	in 541 / out 6.7k · 0.0s
read	/workspace/.pi/crack…/1784450256833_blender_mcp/prompt.md
output

	in 74 / out 639 · 0.0s
Type	Path / command	Size
read	/workspace/.pi/crack…/explore/explore_summary.md
output

	in 91 / out 1.3k · 0.0s
Type	Path / command	Size
think	
Cursor shell: cd /workspace && ls /root/blender-mcp 2>/dev/null; ls /tmp/blen…
	out 255 · 0.7s
bash	

cd /workspace && ls /root/blender-mcp 2>/dev/null; ls /tmp/blender* 2>/dev/null; find /root /opt /workspace -maxdepth 3 -iname '*blender*mcp*' 2>/dev/null; pip show blender-mcp 2>/dev/null; uv pip show blender-mcp 2>/dev/null; npm ls -g blender-mcp 2>/dev/null; rg -n "9876|blender.mcp|blender_mcp|BLENDER" /root/.config/blender/5.1/scripts/addons/ 2>/dev/null | head -40

output

	in 403 / out 4.0k · 0.0s
Type	Path / command	Size
think	
Reading the blender-mcp README and main.py to understand the architecture. A …
	out 418 · 4.5s
read	/tmp/blender-mcp/README.md
output

	in 38 / out 663 · 0.0s
read	/tmp/blender-mcp/pyproject.toml
output

	in 43 / out 531 · 0.0s
read	/tmp/blender-mcp/main.py
output

	in 36 / out 171 · 0.0s
Type	Path / command	Size
think	
Cursor grep: ../../../../tmp/blender-mcp/src/blender_mcp/server.py ../../../.…
	out 254 · 2.6s
read	/workspace/_docker/Dockerfile.base
output

	in 46 / out 669 · 0.0s
Type	Path / command	Size
think	
Cursor shell: cd /workspace && blender --help 2>&1 | rg -i 'window-system|bac…
	out 255 · 0.6s
bash	

cd /workspace && blender --help 2>&1 | rg -i 'window-system|backend|gpu|opengl|egl|wayland|x11' | head -40; ls /tmp/blender-mcp/src/; rg -n "def |port|host|9876|socket" /tmp/blender-mcp/src -g '*.py' | head -60; curl -sI "https://download.blender.org/release/Blender5.1/" 2>&1 | head -15; curl -sL "https://download.blender.org/release/Blender5.1/" 2>&1 | rg -i '5\.1\.[0-9].*(linux|deb|tar)' | head -30

output

	in 441 / out 7.1k · 0.0s
Type	Path / command	Size
think	
Official Blender does not ship with this file.
	out 1.0k · 6.9s
read	/root/.config/blender/5.1/scripts/addons/blender_mcp.py
output

	in 67 / out 870 · 0.0s
read	/root/.config/blender/5.1/scripts/addons/blender_mcp.py
output

	in 67 / out 797 · 0.0s
Type	Path / command	Size
think	Cursor grep: ../../../../tmp/blender-mcp/addon.py	out 50 · 1.7s
read	/tmp/blender-mcp/README.md
output

	in 38 / out 757 · 0.0s
Type	Path / command	Size
think	
Cursor shell: cd /workspace && which blender-mcp uvx; blender-mcp --help 2>&1…
	out 255 · 0.6s
bash	

cd /workspace && which blender-mcp uvx; blender-mcp --help 2>&1 | head -20; dpkg -l xvfb 2>&1 | tail -3; rg -n "execute_code|save|primitive|sphere|cube" /tmp/blender-mcp/src/blender_mcp/server.py | head -40; ls /workspace/.pi/crack/harness/mcp-http/ 2>/dev/null; ss -tlnp 2>/dev/null | rg '9876|9929|9930' || netstat -tlnp 2>/dev/null | rg '9876|9929|9930' || true

output

	in 398 / out 1.2k · 0.0s
Type	Path / command	Size
think	
I understand the boundary constraints. I will verify the callable tool surfac…
	out 1.3k · 23.0s
bash	

cd /workspace && blender --help 2>&1 | rg -i 'addon|python|factory|enable' | head -30; rg -n "execute_code|create_|get_scene|delete" /tmp/blender-mcp/src/blender_mcp/server.py | head -50; ls /opt/blender/5.1/scripts/addons/blender_mcp.py; cat /root/.config/blender/5.1/config/userpref.blend 2>/dev/null | head -c 20; ls /root/.config/blender/5.1/config/ 2>/dev/null; # quick test: can blender start under xvfb with X11?
TIMEOUT=15; timeout 12 env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE GDK_BACKEND=x11 QT_QPA_PLATFORM=xcb SDL_VIDEODRIVER=x11 xvfb-run -a blender --python-expr "import bpy; print('BG', bpy.app.background); print('OK', bpy.app.version_string)" 2>&1 | tail -40

output

	in 711 / out 2.6k · 0.0s
Type	Path / command	Size
think	
Blender crashes on Wayland because XDG_RUNTIME_DIR is invalid.
	out 386 · 62.5s
bash	

cd /workspace && # Try various ways to force X11 for Blender 5.1
mkdir -p /tmp/runtime-root && chmod 700 /tmp/runtime-root
for combo in \
  'XDG_RUNTIME_DIR=/tmp/runtime-root WAYLAND_DISPLAY= GDK_BACKEND=x11' \
  'XDG_RUNTIME_DIR=/tmp/runtime-root WAYLAND_DISPLAY= BLENDER_SYSTEM_SCRIPTS= SDL_VIDEODRIVER=x11' \
  'XDG_RUNTIME_DIR=/tmp/runtime-root WAYLAND_DISPLAY=xdg-none' \
  'XDG_RUNTIME_DIR=/tmp/runtime-root __GLX_VENDOR_LIBRARY_NAME=mesa' ; do
  echo "=== TRY: $combo ==="
  timeout 8 env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 $combo xvfb-run -a blender --python-expr "import bpy; print('OK', bpy.app.version_string, 'bg', bpy.app.background)" 2>&1 | tail -8
done
# Also check if there's an env var documented
rg -l 'Wayland|GHOST_SYSTEM|window.system' /opt/blender/5.1/ 2>/dev/null | head -5
# Check blender source hints via strings
strings /opt/blender/blender 2>/dev/null | rg -i 'WAYLAND|force.x11|GHOST_BACKEND|window.system' | head -30

output

	in 1.0k / out 383 · 0.0s
Type	Path / command	Size
think	
The command appears to be hanging. Blender may be frozen rather than crashing…
	out 97 · 21.1s
read	/root/.cursor/projec…/terminals/31716.txt
output

	in 80 / out 1.7k · 0.0s
Type	Path / command	Size
think	
Cursor shell: cd /workspace && strings /opt/blender/blender 2>/dev/null | rg …
	out 255 · 16.1s
bash	

cd /workspace && strings /opt/blender/blender 2>/dev/null | rg -i 'WAYLAND|force.x11|GHOST_BACKEND|window.system|X11 backend|Use Wayland' | head -40; # try blender-softwaregl and also with DISPLAY set explicitly via xvfb-run -e
timeout 15 bash -c 'Xvfb :99 -screen 0 1280x1024x24 >/tmp/xvfb.log 2>&1 & XV=$!; sleep 1; export DISPLAY=:99 XDG_RUNTIME_DIR=/tmp/runtime-root; unset WAYLAND_DISPLAY; export XDG_SESSION_TYPE=x11 GDK_BACKEND=x11; blender --python-expr "import bpy; print(\"OK\", bpy.app.version_string, \"bg\", bpy.app.background)" 2>&1 | tail -20; kill $XV 2>/dev/null'

output

	in 619 / out 1.6k · 0.0s
Type	Path / command	Size
think	
Wayland failed to initialize. The system fell back to X11.
	out 454 · 10.6s
bash	

cd /workspace && mkdir -p /tmp/runtime-root && chmod 700 /tmp/runtime-root
# Try softwaregl + longer timeout, capture full stderr
timeout 25 bash -c '
  Xvfb :98 -screen 0 1024x768x24 >/tmp/xvfb98.log 2>&1 & XV=$!
  sleep 1
  export DISPLAY=:98
  export XDG_RUNTIME_DIR=/tmp/runtime-root
  unset WAYLAND_DISPLAY
  export XDG_SESSION_TYPE=x11
  export LIBGL_ALWAYS_SOFTWARE=1
  /opt/blender/blender-softwaregl --python-expr "import bpy; print(\"OK\", bpy.app.version_string, \"bg\", bpy.app.background)" >/tmp/bl_out.txt 2>/tmp/bl_err.txt
  echo EXIT:$?
  kill $XV 2>/dev/null
'
echo "--- stdout ---"; cat /tmp/bl_out.txt 2>/dev/null | tail -20
echo "--- stderr ---"; cat /tmp/bl_err.txt 2>/dev/null | tail -40
echo "--- xvfb ---"; cat /tmp/xvfb98.log 2>/dev/null | tail -10
# Also check _ghost_backend env
strings /opt/blender/blender | rg -i '_ghost_backend|GHOST_BACKEND|BLENDER.*BACKEND' | head -20
ls /opt/blender/lib/ | head -20
dpkg -l | rg -i 'mesa|libgl|libegl|libosmesa' | head -20

output

	in 1.1k / out 10.2k · 0.0s
Type	Path / command	Size
think	
blender-softwaregl exited with code 1. The stdout contained only a memory lea…
	out 617 · 9.3s
bash	

cd /workspace && mkdir -p /tmp/runtime-root && chmod 700 /tmp/runtime-root
timeout 40 bash -c '
  Xvfb :97 -screen 0 1024x768x24 >/tmp/xvfb97.log 2>&1 & XV=$!
  sleep 1
  export DISPLAY=:97 XDG_RUNTIME_DIR=/tmp/runtime-root XDG_SESSION_TYPE=x11 LIBGL_ALWAYS_SOFTWARE=1
  unset WAYLAND_DISPLAY
  # force opengl backend
  /opt/blender/blender --gpu-backend opengl --python-expr "import bpy; print(\"OK\", bpy.app.version_string, \"bg\", bpy.app.background); import sys; sys.exit(0)" > /tmp/bl2_out.txt 2> /tmp/bl2_err.txt
  echo EXIT:$?
  kill $XV 2>/dev/null
' || echo TIMEOUT
echo "=== OUT (all) ==="; wc -c /tmp/bl2_out.txt /tmp/bl2_err.txt; head -c 4000 /tmp/bl2_err.txt; echo; echo "=== OUT ==="; head -c 2000 /tmp/bl2_out.txt; echo
# grep for OK / error / Wayland / GHOST
rg -n "OK|Error|Wayland|GHOST|Cannot|failed|OpenGL|Vulkan" /tmp/bl2_err.txt /tmp/bl2_out.txt 2>/dev/null | head -40

output

	in 942 / out 430 · 0.0s
Type	Path / command	Size
think	
Xvfb display :97 is now active.
	out 2.0k · 15.9s
text	
Checking one more detail on how `execute_code` saves files, then I'll finaliz…
	out 98 · 15.9s
read	/tmp/blender-mcp/src/blender_mcp/server.py
output

	in 54 / out 817 · 0.0s
Type	Path / command	Size
read	/workspace/_docker/tcp_forward.py
output

	in 45 / out 838 · 0.0s
Type	Path / command	Size
think	
Cursor shell: cd /workspace && # Confirm port math and whether 9929 is free; …
	out 255 · 0.7s
bash	

cd /workspace && # Confirm port math and whether 9929 is free; note download URL used previously
ls -la /tmp/blender-5.1.2-linux-x64.tar.xz 2>/dev/null; curl -sI "https://download.blender.org/release/Blender5.1/blender-5.1.2-linux-x64.tar.xz" | head -8; ss -tlnp 2>/dev/null | rg ':9929|:9876|:993' || true; which blender-mcp; python3 -c "import blender_mcp; print(blender_mcp.__file__)" 2>/dev/null || true

output

	in 440 / out 422 · 0.0s
Type	Path / command	Size
think	
Enough information is available. No further clarifications are needed. The de…
	out 190 · 37.0s
text	
## Hypotheses (what “done” means)
	out 4.3k · 37.0s
user prompt · round 1 — cool, continue

Type	Path / command	Size
think	
The user confirmed readiness to proceed after the initial planning signal.
	out 385 · 27.5s
text	
I'll pull the plan-stage template and re-check the key files so the final imp…
	out 125 · 27.5s
read	/workspace/_docker/Dockerfile
output

	in 41 / out 206 · 0.0s
Type	Path / command	Size
read	/workspace/_docker/_cont_start.sh
output

	in 45 / out 891 · 0.0s
read	/workspace/_docker/run.sh
output

	in 37 / out 478 · 0.0s
Type	Path / command	Size
read	/workspace/.mcp.json
output

	in 32 / out 519 · 0.0s
Type	Path / command	Size
think	Reading the final_plan.md template to structure the plan.	out 57 · 5.9s
read	prompt_templates/plan/final_plan.md
output

	in 47 / out 751 · 0.0s
Type	Path / command	Size
think	I will write the final implementation plan following the exact structure.	out 73 · 38.6s
text	
# Plan
	out 10.7k · 38.6s
user prompt · round 1 — Stop calling tools now. Based on what you have gathered so far, write your Lay of the land, then …

Type	Path / command	Size
think	
I will stop calling tools and draft the "Lay of the land" section. Then I wil…
	out 210 · 27.0s
text	
## Lay of the land
	out 2.1k · 27.0s
user prompt · round 1 — continue writing the plan draft

Type	Path / command	Size
think	
The user wants me to continue writing the plan draft. I already emitted READY…
	out 333 · 19.8s
text	
# Plan
	out 10.9k · 19.8s
user prompt · round 1 — Stop calling tools now. Based on what you have gathered so far, write your Lay of the land, then …

Type	Path / command	Size
think	
The user is asking me to stop calling tools and provide my final output - eit…
	out 283 · 3.3s
text	
I've already gathered sufficient information and provided the Lay of the Land…
	out 125 · 3.3s
Send a message to resume the agent
