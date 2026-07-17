# crack-pi-server — working notes

Small FastAPI + htmx + pico.css app. Almost all of it lives in two files:
`src/crack_server/app.py` (routes + inline HTML rendering) and
`src/crack_server/paths.py` (all filesystem access). `static/app.css` /
`static/app.js` hold the few bits of real CSS/JS (linked from `_render_base`).

## The server is always running — use it

A docker container runs this server live at all times, reachable at
`http://localhost:9847` from the host. `main.py` starts uvicorn with
`reload=True`, so saving a `.py` file under `src/crack_server/` is picked up
in about a second — **no rebuild or restart needed**. This makes curl the
fastest way to verify a change:

```bash
# create a task (title only — server derives the id, see below)
curl -s -X POST http://localhost:9847/api/tasks -d "title=My Task"

# list tasks / view a task page
curl -s http://localhost:9847/api/tasks
curl -s http://localhost:9847/tasks/<task_id>

# add a prompt (name is optional; blank -> auto-assigned)
curl -s -X POST http://localhost:9847/api/tasks/<task_id>/prompts -d "content=hello"
curl -s -X POST http://localhost:9847/api/tasks/<task_id>/prompts -d "name=notes.md&content=hello"

# edit / delete
curl -s -X PUT http://localhost:9847/api/tasks/<task_id>/prompts/prompt.md -d "content=updated"
curl -s -X DELETE http://localhost:9847/api/tasks/<task_id>/prompts/prompt.md
curl -s -X DELETE http://localhost:9847/api/tasks/<task_id>
```

Clean up any task directories you create while testing (`DELETE /api/tasks/<id>`
or `rm -rf .pi/crack/tasks/<id>`) — don't leave scratch tasks behind in
`.pi/crack/tasks/`.

### Testing the `pi` CLI itself

The "Regenerate Title" button shells out to the `pi` CLI
(`_run_pi_title_generation` in `app.py`), which is only installed *inside*
`crack-dev` — it won't be on the host `PATH`. Before debugging a
regenerate-title failure in the app, confirm `pi` itself works by running it
directly in the container:

```bash
docker exec crack-dev /bin/bash -exc "pi --version"

# same non-interactive form the endpoint uses (model, no session/tools, print+exit)
docker exec crack-dev /bin/bash -exc "pi --model google/gemma-4-31b-it -p --no-session --no-tools 'Say hello in 3 words'"
```

Note `pi` has no `run` subcommand and no `--prompt-file` flag — that mismatch
was the cause of the original "regenerate title does nothing" bug. The prompt
text goes in as a plain positional argument, not a file. Since the app's own
server process already runs inside `crack-dev` (see above), `_run_pi_title_generation`
calls `pi` with a plain `subprocess.run(...)`, no `docker exec` wrapper needed —
`docker exec` is only for you, testing from the host shell.

The endpoint logs everything needed to diagnose a failure without re-running
anything by hand: the full prompt, the `+`-prefixed command line
(`shlex.join`'d, matching bash `-x` style), the configured timeout
(`PI_TIMEOUT_SECONDS`), the elapsed wall time, and a summary of the output.
These go through `logging.getLogger("uvicorn.error")` (the only logger
uvicorn attaches a handler to by default) and show up in `docker logs
crack-dev`.

## Storage layout

- `.pi/crack/tasks/<task_id>/*.md` — prompt files, globbed fresh from disk on
  every request (no caching/DB, so editing a file on disk is immediately
  visible through the UI).
- `.pi/crack/tasks/<task_id>/info.json` — `{created_at, modified_at, title}`.
  There is exactly one title per task (shown in the page header); prompt rows
  do **not** have their own titles. "Regenerate Title"
  (`POST /api/tasks/<id>/regenerate-title`) fills a draft into the title
  input from the combined content of all prompt files — it does not save
  until the Save button is clicked.
- **Task id format is fixed**: `<ms_epoch_timestamp>_<slugified_title>`,
  generated once in `paths.generate_task_id()` at creation time and never
  changed afterward (renaming a task only updates `info.json["title"]`, not
  the directory name/id).
- Prompt filenames: `prompt.md`, `prompt2.md`, ... `prompt9.md` is the
  auto-assigned sequence (`paths.next_prompt_filename`) used whenever a
  caller submits a blank name; custom `*.md` names are also allowed.

## The htmx contract — read this before touching routes

Every route in `app.py` falls into one of two buckets, and mixing them up is
the single easiest way to silently break a button:

1. **Pure JSON API** (`GET /api/tasks`, `GET /api/tasks/{id}/info`,
   `GET /api/tasks/{id}/prompts[...]`) — not called from any HTML form/hx-*
   attribute, safe to return a `dict`.
2. **htmx-driven fragment routes** (basically everything else, especially
   any `POST`/`PUT`/`DELETE` wired to `hx-post`/`hx-put`/`hx-delete` in the
   rendered HTML) — these **must**:
   - accept `Form(...)` fields, never a Pydantic `BaseModel` JSON body.
     Browsers/htmx submit plain HTML forms as
     `application/x-www-form-urlencoded`; a JSON-body endpoint 422s on that
     with a confusing pydantic error, which is exactly what made every
     save/edit button silently fail before this file's last cleanup.
   - return an `HTMLResponse` fragment that matches what the triggering
     element's `hx-target`/`hx-swap` expects — never a JSON `dict`. If you
     return JSON here, htmx will happily swap the literal JSON text into the
     DOM in place of whatever it was supposed to update.
   - for delete endpoints paired with `hx-swap="outerHTML"`, return
     `HTMLResponse("")` — an empty fragment is what makes the element
     disappear.

When adding a new interactive element, grep `app.py` for an existing
`hx-target`/`hx-swap` pair that matches what you want and copy its endpoint's
shape (Form in, matching-fragment out) rather than inventing a new pattern.

## Misc gotchas

- `.venv/`, `__pycache__/`, `.context/` are vendored/generated — don't search
  them, don't hand-edit anything inside them.
- The prompt list and each row are always rendered from disk on every
  request (`_render_prompts_section` / `_render_prompt_row` in `app.py`) —
  there's no in-memory state to go stale, but it also means don't assume a
  row exists just because you saw it in an earlier response.
- The `.pi/extensions/crack_pi/index.ts` pi-agent extension (`/crack ...`
  commands) only *lists/opens* tasks in a browser — it never creates or
  writes task data, so task creation only ever happens through the web UI's
  `POST /api/tasks`.


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
src/crack_server/app.py ← __future__, fastapi, crack_server, shlex
src/crack_server/main.py ← uvicorn
src/crack_server/paths.py ← __future__
```

## versions (installed direct deps)
```
fastapi@0.139.2
python-multipart@0.0.32
uvicorn@0.51.0
```

## .

### pyproject.toml
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

### README.md
```
h1 crack-pi-server
h1 from repository root
code-fence bash
code-fence plain
```

## src

### src/crack_server/app.py
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

### src/crack_server/main.py
```
def main() → None  :8-11
```

### src/crack_server/paths.py
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

### src/crack_server/static/app.css
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
