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
.pi/crack/server/src/crack_server/app.py ← __future__, fastapi, crack_server, shlex
.pi/crack/server/src/crack_server/main.py ← uvicorn
.pi/crack/server/src/crack_server/paths.py ← __future__
```

## changes (last 5 commits — 12 minutes ago)
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
def api_delete_task(task_id: str) → HTMLResponse  :801-820  # Delete a task directory
def api_tasks() → dict  :824-826
def api_get_task_info(task_id: str) → dict  :830-835
def api_list_prompts(task_id: str) → dict  :853-858
def api_get_prompt(task_id: str, filename: str) → dict  :862-869
GET /  →  index()  :752-784
POST /api/tasks  →  api_create_task()  :788-797
DELETE /api/tasks/{task_id}  →  api_delete_task()  :801-820
GET /api/tasks  →  api_tasks()  :824-826
GET /api/tasks/{task_id}/info  →  api_get_task_info()  :830-835
PUT /api/tasks/{task_id}/info  →  api_update_task_info()  :839-849
GET /api/tasks/{task_id}/prompts  →  api_list_prompts()  :853-858
GET /api/tasks/{task_id}/prompts/{filename}  →  api_get_prompt()  :862-869
POST /api/tasks/{task_id}/prompts  →  api_create_prompt()  :873-897
PUT /api/tasks/{task_id}/prompts/{filename}  →  api_update_prompt()  :901-921
DELETE /api/tasks/{task_id}/prompts/{filename}  →  api_delete_prompt()  :925-935
POST /api/tasks/{task_id}/regenerate-title  →  api_regenerate_task_title()  :939-947
GET /tasks/{task_id}/title-regen-status  →  title_regen_status()  :951-979
POST /api/tasks/{task_id}/explore  →  api_explore()  :983-993
GET /tasks/{task_id}/explore-status  →  explore_status()  :997-1003
GET /tasks/{task_id}  →  task_page()  :1007-1041
GET /tasks/{task_id}/prompts-list  →  task_prompts_list()  :1045-1051
GET /tasks/{task_id}/prompt-row/{filename}  →  prompt_row()  :1055-1061
```

### .pi/crack/server/src/crack_server/main.py
```
def main() → None  :8-11
```

### .pi/crack/server/src/crack_server/paths.py
```
def project_root() → Path  :18-20
def tasks_dir(root: Path | None) → Path  :23-24
def task_dir(task_id: str, root: Path | None) → Path  :27-30
def validate_prompt_filename(name: str) → str  :33-37
def list_task_ids(root: Path | None) → list[str]  :40-44
def list_prompt_files(task_id: str, root: Path | None) → list[dict[str, str | int]]  :47-65  # Glob *
def read_prompt(task_id: str, filename: str, root: Path | None) → str  :68-73
def write_prompt(task_id: str, filename: str, content: str, root: Path | None) → None  :76-81
def delete_prompt(task_id: str, filename: str, root: Path | None) → None  :84-89
def info_path(task_id: str, root: Path | None) → Path  :92-93
def read_info(task_id: str, root: Path | None) → dict  :96-103
def write_info(task_id: str, info: dict, root: Path | None) → None  :106-112
def title_regen_path(task_id: str, root: Path | None) → Path  :123-124
def read_title_regen_state(task_id: str, root: Path | None) → dict  :127-134
def write_title_regen_state(task_id: str, state: dict, root: Path | None) → None  :137-138
def explore_path(task_id: str, root: Path | None) → Path  :141-142
def read_explore_state(task_id: str, root: Path | None) → dict  :145-152
def write_explore_state(task_id: str, state: dict, root: Path | None) → None  :155-156
def read_all_prompts_joined(task_id: str, root: Path | None) → str  :159-167  # Read all prompt markdown files in a task and join them with 
def slugify_title(title: str) → str  :170-173  # Replace runs of non-alphanumeric characters with '_', stripp
def generate_task_id(title: str) → str  :176-178  # Task id format: <ms_epoch_timestamp>_<slugified_title>
def create_task(task_id: str, title: str | None, root: Path | None) → dict  :181-196  # Create a new task directory with info
def next_prompt_filename(task_id: str, root: Path | None) → str | None  :199-208  # Return the next available prompt filename (prompt
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

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/materials.rs
```
pub struct BillboardParams
pub struct AdditiveFxMaterial
pub struct BlendFxMaterial
pub enum FxKind
impl AdditiveFxMaterial
impl BlendFxMaterial
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/mod.rs
```
pub struct VisualFXPlugin
impl VisualFXPlugin
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/settings.rs
```
pub struct VfxSettings
impl VfxSettings
```

### crack_demo/demo_resolution_selector_web_bevy/src/plugins/visual_fx/ui.rs
```
pub fn vfx_controls_window(mut contexts: EguiContexts, mut ui_state: ResMut<UiState>, mut s: ResMut<VfxSettings>,)
```

### crack_demo/demo_resolution_selector_web_bevy/src/ui_egui.rs
```
pub struct UiEguiPlugin
pub struct UiState
impl UiEguiPlugin
impl UiState
impl UiState
  pub fn with_physics_debug() → Self
impl UiState
pub fn web_set_loading_status(_show: bool, _message: &str)
```

### crack_demo/demo_resolution_selector_web_bevy/src/utils/setup_debug_scene.rs
```
pub struct SetupDebugScenePlugin
pub struct DebugSceneGroundComponent
impl SetupDebugScenePlugin
```

### crack_demo/game_logic/src/worker/osm_impl.rs
```
pub async fn fetch_osm_data(args: FetchArgs) → anyhow::Result<OsmDataResult>
```

## rust_pkg

### rust_pkg/net_crackpipe/src/chat/global_chat.rs
```
pub struct GlobalChatRoomType
pub struct GlobalChatPresence
pub enum GlobalChatMessageContent
pub enum GlobalChatBootstrapQuery
pub enum MatchHandshakeType
impl GlobalChatRoomType
```

### rust_pkg/net_crackpipe/src/global_matchmaker.rs
```
pub struct GlobalMatchmaker
pub struct BootstrapNodeInfo
impl GlobalMatchmakerInner
  pub async fn shutdown(&mut self) → Result<()>
impl GlobalMatchmaker
impl GlobalMatchmaker
  pub async fn sleep(&self, duration: Duration)
  pub async fn shutdown(&self) → Result<()>
  pub fn user_secrets(&self) → std::sync::Arc<UserIdentity...
  pub fn own_node_identity(&self) → NodeIdentity
  pub fn user(&self) → UserIdentity
  pub async fn global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn bs_global_chat_controller(&self) → Option<ChatController<Globa...
  pub async fn display_debug_info(&self) → Result<String>
```
