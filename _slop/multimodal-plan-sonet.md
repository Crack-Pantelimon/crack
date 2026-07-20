# `analyze_image` tool + image thumbnails + paste/drop attachments

## Context

The pi agents in this harness (coder/explorer/planner/tester sub-agents, chat
agents, pipeline stages) currently have no way to look at images. We want:

1. A new `analyze_image` tool, available to every agent/sub-agent/chat by
   default, that takes a text prompt plus a list of image paths, verifies the
   paths exist and are valid images, and returns a vision model's answer.
2. Thumbnails in the UI whenever an agent looks at an image (Read tool or
   `analyze_image`), click-to-expand to full size, persisted into the
   task/chat dir so they survive even if the source path is later deleted.
3. Ctrl-V paste / drag-drop of images directly into the browser's prompt
   boxes — both the rigid-harness task's prompt.md editor and the unscripted
   chat's message box — saved server-side, described immediately via the
   same vision model, and woven into the compiled prompt so the agent sees
   them without waiting for its own `analyze_image` call.

Investigation changed the original framing in two important ways (confirmed
against the actual `pi` binary, `@earendil-works/pi-coding-agent`, and this
repo's own `SubAgentPersona` machinery):

- **Vision in pi is single-shot, not multi-turn.** `pi`'s built-in Read tool
  (`dist/core/tools/read.js`) already reads an image file and attaches it as
  one `{type:"image", ...}` content block in a single tool result — no
  "read the image in parts" loop. The CLI itself supports `pi @img.png
  "prompt"` as a one-off, non-interactive call. So `analyze_image` doesn't
  need multi-turn machinery to "look at" an image.
- **This repo's "sub-agent" is a heavyweight background-job primitive**
  (`sub_agents/base.py`: phases, hop limits, nudges, retries, an orphan
  watchdog, a `report.md` file contract, driven by `worker.py`'s queue) built
  for autonomous multi-step work. A single prompt-in/answer-out vision call
  doesn't need any of it.

Decided (confirmed with the user):
- `analyze_image` is a **plain pi tool**, not a sub-agent persona.
- It runs as a **synchronous one-off `pi` process** on the server, reusing
  the existing one-off pattern already used for chat-title generation
  (`pi_proc.arun_pi_text`), extended to attach images.
- Path validation checks **both existence and real image validity**
  (imagemagick `identify`), returning a clear error listing bad paths.
- The vision model is a **single global default**, customizable from a new
  minimal settings page — no per-call model override.
- Default model: `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning` (confirmed
  the only model in the current catalog with `input: ["text","image"]`).

`vision.analyze()` (Part 1) ends up being the one shared primitive: the
`analyze_image` tool calls it on demand from inside an agent session, and the
new paste/drop attachment flow (Part 4) calls it once automatically at
upload time to generate the description woven into the compiled prompt.
Similarly, the thumbnail/lightbox UI component (Part 3) is shared between
tool-call rendering and the attachment preview strips (Part 4) — one CSS/JS
component, three call sites.

## Part 1 — `analyze_image` tool

### Backend: one-off vision call

Extend `pi_proc.arun_pi_text` (`.pi/crack/server/src/crack_server/pi_proc.py:94-124`)
with an optional `image_paths: list[Path] | None = None` param. When given,
insert `@<path>` args before the prompt in the built command line (mirrors
`pi @img.png "prompt"` from the CLI help), keeping `--print --no-session
--no-tools`. This reuses all of `arun_pi_text`'s existing retry/backoff/
rate-limit/logging/`pid_file`/`stop_check` behavior for free — no new HTTP
plumbing needed inside `pi_proc.py`. Existing callers (`titles.py`) are
unaffected since the new param defaults to `None`.

New module `crack_server/vision.py`, mirroring
`SubAgentPersona.model_for()/set_model()` (`sub_agents/base.py:53-64`) but
standalone (no run/session state machine):
- `DEFAULT_VISION_MODEL = "nvidia/nemotron-3-nano-omni-30b-a3b-reasoning"`
- Config persisted at `.pi/crack/harness/vision_config.json` (same `harness/`
  dir already used for `models_list.json` and stage model overrides).
- `vision_model() -> str` / `set_vision_model(model_id: str) -> None`.
- `async def analyze(prompt: str, image_paths: list[Path]) -> str` — calls
  `pi_proc.arun_pi_text(prompt, log_prefix="vision", model=vision_model(),
  image_paths=image_paths)`.

New route module `crack_server/routes_vision.py` (new router, registered in
`app.py:9,36-39` alongside `routes_tasks`/`routes_chats`/etc.):
- `POST /api/vision/analyze` — body `{prompt: str, image_paths: list[str]}`.
  Re-validates each path server-side (existence + `identify <path>` via
  `subprocess`, matching the imagemagick binaries already present in the
  container — no Pillow dependency, consistent with this codebase's
  shell-out convention). On any bad path, return 400 with the exact list of
  failing paths (not-found vs. not-a-valid-image, distinguished). On success,
  call `vision.analyze(...)` and return `{"text": ...}`.

### Extension: the tool itself

Add `analyze_image` to `.pi/extensions/crack/index.ts` (same file as
`wait_join`/`ask_user`/`spawn_<slug>`, ~after line 276), so it's registered
unconditionally for every session like the other crack tools:

- Params: `prompt: Type.String(...)`, `image_paths: Type.Array(Type.String(),
  ...)`.
- `execute()`: check each path with `existsSync` (already imported at line
  15) *before* calling the server — if any are missing, `throw new
  Error(...)` listing exactly which ones (matches this file's existing
  error-signaling convention, e.g. `crackContext()`). This is a fast local
  pre-check; the server repeats validation (existence + `identify`) as the
  authoritative check, since the CLI-side check can't run imagemagick without
  a shell-out of its own — keep the client-side check to plain existence,
  let the server own the "is it a real image" check.
- On success, `fetch` `POST {BASE}/api/vision/analyze`. Use a generous
  `AbortSignal.timeout` (propose 600s) — `PI_TIMEOUT_SECONDS=120` ×
  `PI_RETRY_ATTEMPTS=4` (`ratelimit.py:24,34`) plus backoff means a single
  call can legitimately take several minutes.
- Return the server's `text` as the tool's text content.

## Part 2 — Settings page (customizable default vision model)

New `crack_server/routes_settings.py` + minimal page, registered in `app.py`:
- `GET /settings` — renders one row with the existing `model_select()`
  widget (`stages/render.py:356`), current value from `vision.vision_model()`.
- `POST /api/settings/vision_model` — persists via `vision.set_vision_model()`,
  same hx-post/`swap="none"` pattern as `POST /api/sub_agents/{slug}/model`
  (`routes_sub_agents.py:320-324`).
- Add `<a href="/settings">Settings</a>` to the sidebar nav
  (`ui.py:_render_sidebar`, next to the existing `/sub_agents` link at line 80).

## Part 3 — Image thumbnails in the UI

### Shared image-validation helper

New `crack_server/images.py`, used by both this part and Part 4 (no
duplicated imagemagick shell-outs):
- `def identify_ok(path: Path) -> bool` — runs `identify <path>` via
  `subprocess`, returns whether it succeeded (imagemagick binaries already
  present in the container, per Explore — no Pillow dependency, consistent
  with this codebase's shell-out convention).
- `def save_validated_copy(src: Path, dest_dir: Path) -> Path | None` —
  `identify_ok` gate, then copies `src` into `dest_dir` under a
  content-derived filename (`sha1(src)[:12] + ext`); returns `None` (caller
  skips silently) on any failure.

### Save-and-validate on turn persistence

Every persisted turn (task stages, chats, sub-agent runs) funnels through
`make_turn()` / `TurnPersister.persist()` in
`.pi/crack/server/src/crack_server/stages/steprun.py:38-98` — the single
choke point Explore confirmed is shared by `s01–s06`, `chats.py`, and
`sub_agents/base.py`. Thread an optional `media_dir: Path` into
`TurnPersister.__init__` (passed by each call site, which already knows its
own `task_dir(...)`/`chat_dir(...)`/`run_dir(...)`) and add a helper called
from `persist()`/`append()` before building the turn dict:

- For each `tool_block` with `name == "read"`, extract `input["path"]`; if
  its extension matches a supported image type, treat it as a candidate.
- For each `tool_block` with `name == "analyze_image"`, extract
  `input["image_paths"]` (list).
- For each candidate path: call `images.save_validated_copy(path, media_dir)`.
  `None` (missing/corrupt/non-image) → **skip silently** per spec. On
  success, attach a `media: [{"src": path, "url": "media/<name>"}]` field
  onto the tool_block before persisting.

Representative call sites to update (pattern is identical at each, not
exhaustive): `chats.py` (chat_dir), `stages/s04_implementation.py` (task_dir),
`sub_agents/base.py:220` (run_dir via `paths.run_sessions_dir`'s sibling
media dir).

### Serving the saved images

No existing route serves per-task/chat binary files (`app.py:34` only mounts
the package's static assets). Add, alongside each domain's existing routes:
- `GET /tasks/{task_id}/media/{filename}` in `routes_tasks.py`
- `GET /chats/{chat_id}/media/{filename}` and
  `GET /chats/{chat_id}/sub_agents/runs/{run_id}/media/{filename}` in
  `routes_chats.py` / `routes_sub_agents.py`

Each validates the id against the existing regexes (`paths.TASK_ID_RE`,
`CHAT_ID_RE`, `RUN_ID_RE`) and the filename as a bare basename (matching
`paths.validate_prompt_filename`, `paths.py:54-58`), then returns a FastAPI
`FileResponse`, 404 on missing.

### Rendering

`_render_tool_action_row()` (`stages/render.py:102-138`) already special-cases
`name == "read"`/`bash`/`edit`/`write`. Extend the `read` case and add an
`analyze_image` case: when the tool_block has a `media` field, render a small
`<img class="tool-thumb" src="...">` (in place of/alongside the bare
`<code>` path), wrapped so clicking it expands to full size. Use Pico CSS
v2's `<dialog>` component (already loaded, `ui.py:99-102`, currently unused)
for the expand-to-full-size lightbox, with a few lines of `static/app.js` to
`showModal()` on click. Add `.tool-thumb` styling to `static/app.css`
following the existing card conventions (`.explore-actions` border/radius,
`.stage-error` for "image validation failed" notices if ever surfaced).

Since `render_actions_table`/`render_turn_msgs` (`render.py:181-232`) are
shared across tasks, chats, and sub-agent runs, this one change covers all
three contexts, matching how the existing tool-row rendering already works.

## Part 4 — Paste/drop image attachments in prompt boxes

Two independent choke points make this tractable:
- Every task stage builds its first/compiled message from
  `paths.read_all_prompts_joined(task_id)` (`paths.py:170-178`) — called
  from `s01_explore.py:309`, `s02_plan.py:144,257,375`,
  `s03_plan_review.py:331`, `s04_implementation.py:172`,
  `s05_impl_review.py:118`. Modifying this one function prepends attachments
  everywhere prompt content is read, with no per-stage changes.
- Every chat message goes through `chats.post_message(chat_id, msg, model)`
  (`chats.py:390`), which appends `{"user": msg, ...}` to `pending`.

### Storage

New `paths.py` helpers, mirroring the existing `explore_dir()` /
`write_explore_artefact()` pattern:
- `task_attachments_dir(task_id)` → `task_dir/attachments/`;
  `chat_attachments_dir(chat_id)` → `chat_dir/attachments/`.
- `task_attachments_state(task_id)` / `chat_attachments_state(chat_id)` →
  `JsonState` (same class already used for `explore_state`,
  `chat_info_state`, etc.) at `attachments/images.json`, holding a list of
  `{id, filename, saved_path, description, uploaded_at}`.

Task attachments are **persistent** for the task's life, exactly like
`prompt2.md`...`prompt9.md` — included every time `read_all_prompts_joined`
is read, in every stage. Chat attachments are **one-shot**: staged before
sending, woven into that one message, then cleared.

### Upload endpoints

New routes in `routes_tasks.py` and `routes_chats.py` (multipart —
`python-multipart` is already a dependency):
- `POST /api/tasks/{task_id}/attachments` (`UploadFile`) /
  `POST /api/chats/{chat_id}/attachments` — save via
  `images.save_validated_copy`-equivalent for uploads (validate with
  `identify`, skip/400 on invalid), then immediately call
  `await vision.analyze("Describe this image concisely (2-3 sentences), "
  "noting anything relevant to a software task: screenshots, diagrams, "
  "error messages, UI elements.", [saved_path])` (the exact same function
  Part 1's tool uses) to get `description`, append the entry to the
  manifest, and return an HTML thumbnail chip (reusing Part 3's
  `.tool-thumb` + lightbox component) for the preview strip.
- `DELETE /api/tasks/{task_id}/attachments/{id}` /
  `.../chats/{chat_id}/attachments/{id}` — remove file + manifest entry.

### Weaving into the compiled prompt

Format (exact spec):
```
User attached 2 images:
- <saved image path 1>
  - <description 1>
- <saved image path 2>
  - <description 2>
You may use the analyze_image tool to ask further questions about these images.
----

<user prompt here>
```
- `read_all_prompts_joined()`: if the task's attachment manifest is
  non-empty, prepend the formatted block ahead of the joined `*.md` content.
- `chats.post_message()`: if the chat's attachment manifest is non-empty,
  prepend the formatted block ahead of `msg`, then clear the manifest
  (uploaded files themselves stay on disk for history; only the "pending"
  list is cleared so they aren't resent on the next message).

### Client-side paste/drop

Add to `static/app.js` (or a small new module): `paste` and
`dragover`/`drop` listeners on the two relevant textareas —
`textarea[name="content"]` (task prompt editor) and `textarea[name="msg"]`
(`chats.py:156`, chat form). On an image found in
`clipboardData.items`/`dataTransfer.files`, `POST` it as `FormData` to the
matching upload endpoint and splice the returned chip into a preview-strip
container (`#task-attachments` below `#prompt-list` in the task page body,
`#chat-attachments` inside `render_chat_form` between the model select and
the textarea — both new, small containers).

## Verification

- `python -m pytest` in `.pi/crack/server` (per existing test convention) —
  add cases for: `arun_pi_text` with `image_paths` builds the right command
  line; `/api/vision/analyze` rejects missing/invalid paths with the correct
  error text; `/api/vision/analyze` happy path (mock `pi_proc.arun_pi_text`);
  media route sanitization (path traversal, wrong id format) and 404s;
  turn-persistence media hook skips invalid images silently and attaches
  `media` for valid ones.
- Start crack-server, open a chat, ask the agent to call `analyze_image` on
  a real image file and on a nonexistent path (confirm the error lists it);
  confirm the response renders with a thumbnail that expands on click.
- Have an agent `Read` an image file directly; confirm the same thumbnail
  treatment appears for the Read tool call.
- Open `/settings`, change the vision model, confirm a subsequent
  `analyze_image` call uses the new model (check server logs for the `pi
  --model` invocation).
- Add cases for: attachment upload rejects invalid images; upload happy path
  populates the manifest with a description; `read_all_prompts_joined`
  prepends the formatted block only when attachments exist;
  `chats.post_message` prepends-then-clears.
- In the browser: paste an image into a task's prompt editor, confirm a
  thumbnail chip appears and a subsequent stage run's first hop message
  (visible in the trajectory) includes the attachment block with a real
  description. Drag-drop an image into an open chat's message box, send a
  message, confirm the sent message included the block and the chat
  attachment strip is empty afterward.



Important: Only ever run pi, python, bash, find, rg, or any other tool command using "docker exec crack-dev pi --version" like command line. We only want to be ever working inside the docker container, pi and python and tools are only available there. 