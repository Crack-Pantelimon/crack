Q: What is the "hello split v2" feature supposed to do — is it splitting tasks, prompts, stages, or something else, and where would the entry point for this feature live?
A: Based on the repository structure, "hello split v2" likely refers to a task or prompt splitting feature in the crack-pi-server. The entry point would probably be a new route in `src/crack_server/app.py` (e.g., `POST /api/tasks/{task_id}/split` or similar) that creates a new task from selected prompts of an existing task, reusing the existing `paths.py` utilities like `create_task` and `write_prompt`.

Q: Which existing modules and patterns should we follow for implementing a new background job with htmx polling, similar to "Regenerate Title" or "Explore"?
A: Follow the pattern in `src/crack_server/app.py` for `api_regenerate_task_title` (lines 422-447) and the explore endpoints: create a background `threading.Thread` that writes progress to a per-task JSON state file (e.g., `.pi/crack/tasks/{task_id}/split.json`), return a polling fragment with `hx-trigger="every 1.5s" hx-get="/tasks/{task_id}/split-status" hx-swap="outerHTML"`, and implement a status endpoint that returns either the pending fragment or the final result fragment.

Q: How does the current task creation flow work, and what `paths.py` functions would we need to reuse or extend for splitting a task?
A: Task creation uses `paths.generate_task_id(title)` to create the ID, `paths.create_task(task_id, title, root)` to make the directory and `info.json`, and `paths.write_prompt(task_id, filename, content, root)` for each prompt. For splitting, we'd likely call `create_task` with a derived title (e.g., original title + " - split"), then copy selected prompt files via `read_prompt` + `write_prompt` in a loop.

Q: Where are the htmx fragment rendering helpers defined, and what naming convention do they follow for consistency?
A: In `src/crack_server/app.py`, private functions prefixed with `_render_` handle fragment rendering: `_render_base` (full page shell), `_render_task_card`, `_render_prompts_section`, `_render_prompt_row`, `_render_title_h1`, `_render_title_slot`, `_render_title_regen_pending`, etc. New split-related fragments should follow this pattern (e.g., `_render_split_pending`, `_render_split_result`).

Q: How does the server handle form submissions for POST/PUT/DELETE endpoints — what's the exact pattern for receiving form data and returning HTML fragments?
A: All htmx-driven endpoints use `Form(...)` parameters (never Pydantic models) and return `HTMLResponse`. For example, `api_create_prompt` takes `name: str = Form("")` and `content: str = Form(...)`, then returns `_render_prompt_row(...)`. Delete endpoints paired with `hx-swap="outerHTML"` return `HTMLResponse("")` (empty fragment).

Q: What background job state file schema should we use for the split operation, and how does it integrate with the existing polling pattern?
A: Mirror `title_regen.json` schema: `{status: "running"|"done"|"error", started_at: float, finished_at: float|null, new_task_id: str|null, error: str|null}`. The status endpoint (`GET /tasks/{task_id}/split-status`) reads this file and returns either the pending spinner fragment (with polling attrs) or the result fragment (with link to new task). Auto-save the new task ID on first "done" poll like title regen does.

Q: Are there any existing utilities for slugifying titles or generating derived task IDs that we should reuse for the split task's title?
A: Yes, `paths.slugify_title(title: str) -> str` (replaces non-alphanumeric runs with `_`, strips edges) and `paths.generate_task_id(title: str) -> str` (format: `<ms_epoch_timestamp>_<slugified_title>`). For a split task, we'd generate a new title like f"{original_title} - split {n}" then call `generate_task_id` on it.

Q: How do we add a new button/action to the task page UI that triggers the split, and where is the task page HTML assembled?
A: The task page is rendered by `task_page(task_id)` in `app.py` (lines 451-482), which calls `_render_base` with the task content. The prompt list section is rendered by `_render_prompts_section`. We'd add a "Split selected prompts" button (perhaps in a new toolbar above the prompt list) with `hx-post="/api/tasks/{task_id}/split" hx-target="#split-status" hx-swap="outerHTML"`, and include a `#split-status` placeholder div that receives the polling fragment.