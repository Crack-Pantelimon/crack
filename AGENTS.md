The game is in folder `crack_demo/demo_resolution_selector_web_bevy`.

Base rust packages are under `rust_pkg`. 

Data/asset generation and pre-procesing is in `_data`.

## Auto-generated signatures
<!-- Updated by gen-context.js -->
# Code signatures

## SigMap commands

| When | Command |
|------|---------|
| Before answering a question about code | `sigmap ask "<your question>"` |
| To rank files by topic | `sigmap --query "<topic>"` |
| After changing config or source dirs | `sigmap validate` |
| To verify an AI answer is grounded | `sigmap judge --response <file>` |

Always run `sigmap ask` (or `sigmap --query`) before searching for files relevant to a task.

## deps
```
_data/3d_data_v2/_blend_build_map.py ← mathutils, bmesh, bpy, numpy
_data/3d_data_v2/_blend_render_postprocess.py ← __future__, mathutils, bpy
_data/3d_data_v2/_blend_render_topdown.py ← __future__, mathutils, bpy
.pi/crack/server/src/crack_server/app.py ← __future__, fastapi, crack_server, shlex
.pi/crack/server/src/crack_server/main.py ← uvicorn
.pi/crack/server/src/crack_server/paths.py ← __future__
_data/3d_data_v2/yolo_v8_obb_sat.py ← __future__, cv2, numpy
```

## changes (last 5 commits — 3 minutes ago)
```
.pi/crack/server/src/crack_server/app.py      +_esc  +_format_time  +_render_base  +_render_task_card
.pi/crack/server/src/crack_server/main.py     +main
.pi/crack/server/src/crack_server/paths.py    +project_root  +tasks_dir  +task_dir  +validate_prompt_filename
```

## .pi

### .pi/crack/server/pyproject.toml
```
table [project]
table [project.scripts]
table [build-system]
table [tool.hatch.build.targets.wheel]
table [tool.hatch.build.targets.wheel.sources]
key name
key version
key description
key readme
key requires-python
key dependencies
key crack-server
key build-backend
```

### .pi/crack/server/README.md
```
h1 crack-pi-server
h1 from repository root
code-fence bash
code-fence plain
```

### .pi/crack/server/src/crack_server/app.py
```
def index() → HTMLResponse  :214-246
def api_delete_prompt(task_id: str, filename: str) → HTMLResponse  :371-379  # Returns an empty fragment so htmx's outerHTML swap removes t
def api_regenerate_task_title(task_id: str) → HTMLResponse  :422-447  # Regenerate the task title from the combined content of its p
def task_page(task_id: str) → HTMLResponse  :451-482
def task_prompts_list(task_id: str) → HTMLResponse  :486-492  # Return the prompt list HTML fragment for htmx (initial load 
GET /  →  index()  :214-246
POST /api/tasks  →  api_create_task()  :250-259
DELETE /api/tasks/{task_id}  →  api_delete_task()  :263-282
GET /api/tasks  →  api_tasks()  :286-288
GET /api/tasks/{task_id}/info  →  api_get_task_info()  :292-297
PUT /api/tasks/{task_id}/info  →  api_update_task_info()  :301-311
GET /api/tasks/{task_id}/prompts  →  api_list_prompts()  :315-320
GET /api/tasks/{task_id}/prompts/{filename}  →  api_get_prompt()  :324-331
POST /api/tasks/{task_id}/prompts  →  api_create_prompt()  :335-356
PUT /api/tasks/{task_id}/prompts/{filename}  →  api_update_prompt()  :360-367
DELETE /api/tasks/{task_id}/prompts/{filename}  →  api_delete_prompt()  :371-379
POST /api/tasks/{task_id}/regenerate-title  →  api_regenerate_task_title()  :422-447
GET /tasks/{task_id}  →  task_page()  :451-482
GET /tasks/{task_id}/prompts-list  →  task_prompts_list()  :486-492
GET /tasks/{task_id}/prompt-row/{filename}  →  prompt_row()  :496-502
```

### .pi/crack/server/src/crack_server/main.py
```
def main() → None  :8-11
```

### .pi/crack/server/src/crack_server/paths.py
```
def project_root() → Path  :16-18
def tasks_dir(root: Path | None) → Path  :21-22
def task_dir(task_id: str, root: Path | None) → Path  :25-28
def validate_prompt_filename(name: str) → str  :31-35
def list_task_ids(root: Path | None) → list[str]  :38-42
def list_prompt_files(task_id: str, root: Path | None) → list[dict[str, str | int]]  :45-63  # Glob *
def read_prompt(task_id: str, filename: str, root: Path | None) → str  :66-71
def write_prompt(task_id: str, filename: str, content: str, root: Path | None) → None  :74-79
def delete_prompt(task_id: str, filename: str, root: Path | None) → None  :82-87
def info_path(task_id: str, root: Path | None) → Path  :90-91
def read_info(task_id: str, root: Path | None) → dict  :94-101
def write_info(task_id: str, info: dict, root: Path | None) → None  :104-110
def slugify_title(title: str) → str  :113-116  # Replace runs of non-alphanumeric characters with '_', stripp
def generate_task_id(title: str) → str  :119-121  # Task id format: <ms_epoch_timestamp>_<slugified_title>
def create_task(task_id: str, title: str | None, root: Path | None) → dict  :124-139  # Create a new task directory with info
def next_prompt_filename(task_id: str, root: Path | None) → str | None  :142-151  # Return the next available prompt filename (prompt
```

### .pi/crack/server/src/crack_server/static/app.css
```
.prompt-row
.prompt-row
.prompt-row
.title-row
.title-input
.htmx-indicator
.htmx-request
.htmx-request
```

## _data

### _data/3d_data_v2/_blend_build_map.py
```
def clear_scene  :28-30
def weld_terrain_mesh  :33-48
def measure_terrain_bbox  :51-81
def latlon_to_xy  :84-99
def build_terrain_bvh  :102-131
def raycast_hit  :134-135
def raycast_height  :148-151
def resolve_heights  :154-178
def get_or_create_collection  :181-186
def create_road_object  :189-192
def resolve_corner_heights  :208-212
def build_collider_mesh  :215-221
def create_car_object  :299-302
def build_fill_material  :371-372
def cut_car_from_terrain  :589-595
def log  :774-775
def process_item  :778-904
def main  :907-936
```

### _data/3d_data_v2/_blend_render_postprocess.py
```
def pick_render_engine() → str  :30-38
def convert_materials_to_emission() → None  :41-66  # Flatten every textured material to an unlit emission of its 
def make_cage_material() → bpy.types.Material  :69-98  # A translucent red-orange tint for the car wrappers: emission
def show_car_wrappers_as_cage() → None  :101-116  # Tint every object in the 'cars' collection translucent red s
def compute_mesh_bbox(objects) → dict | None  :119-144
def setup_world_black(scene: bpy.types.Scene) → None  :147-153
def render_blend(blend_path: str) → bool  :156-211
def main() → None  :214-230
```

### _data/3d_data_v2/_blend_render_topdown.py
```
def enable_gpu_rendering() → list[str]  :25-55  # Enable GPU compute devices for Blender rendering
def pick_render_engine() → str  :58-66
def ensure_gpu_rendering() → None  :69-74
def clear_scene() → None  :77-96
def convert_materials_to_emission() → None  :99-123
def compute_mesh_bbox() → dict | None  :126-154
def resolve_resolution(tile: dict) → tuple[int, int]  :157-161
def setup_render_settings(scene: bpy.types.Scene, *, width: int, height: int) → None  :164-187
def render_tile(tile: dict) → bool  :190-253
def main() → None  :256-287
```

### _data/3d_data_v2/yolo_v8_obb_sat.py
```
def load_net  :29-33
def detect_cars  :43-48
```

## _docker

### _docker/_cont_start.sh
```
export CRACK_PI_PROJECT_ROOT
```

### _docker/build.sh
```
export IMG_NAME
```

### _docker/run.sh
```
export IMG_NAME
```

## crack_demo

### crack_demo/game_logic/src/worker/osm_impl.rs
```
pub async fn fetch_osm_data(args: FetchArgs) → anyhow::Result<OsmDataResult>  :14-104
```

## rust_pkg

### rust_pkg/net_crackpipe/src/chat/global_chat.rs
```
pub struct GlobalChatRoomType  :6-6
pub struct GlobalChatPresence  :16-20
pub enum GlobalChatMessageContent  :24-36
pub enum GlobalChatBootstrapQuery  :40-43
pub enum MatchHandshakeType  :46-51
impl GlobalChatRoomType  :8-14
```

### rust_pkg/net_crackpipe/src/global_matchmaker.rs
```
pub struct GlobalMatchmaker  :39-48
pub struct BootstrapNodeInfo  :98-104
impl GlobalMatchmakerInner  :65-89
  pub async fn shutdown(&mut self) → Result<()>  :66-66
impl GlobalMatchmaker  :91-95
impl GlobalMatchmaker  :106-247
  pub async fn sleep(&self, duration: Duration)  :107-107
  pub async fn shutdown(&self) → Result<()>  :110-110
  pub fn user_secrets(&self) → std::sync::Arc<UserIdentity...  :122-122
  pub fn own_node_identity(&self) → NodeIdentity  :125-125
  pub fn user(&self) → UserIdentity  :132-132
  pub async fn global_chat_controller(&self) → Option<ChatController<Globa...  :136-136
  pub async fn bs_global_chat_controller(&self) → Option<ChatController<Globa...  :139-139
  pub async fn display_debug_info(&self) → Result<String>  :142-142
```
