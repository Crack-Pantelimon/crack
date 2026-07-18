# crack-pi-server — working notes

Small FastAPI + htmx + pico.css app. `src/crack_server/app.py` is a thin
routing layer (task/prompt CRUD, title regen, delegation); `paths.py` holds all
filesystem access; `pi_runner.py` the shared `pi` subprocess machinery (rate
limiting, single-shot calls, the JSON-mode hop runner); `models.py` the
`pi --list-models` cache; `chats.py` the unscripted chats (free-form pi
sessions outside the pipeline, state under `.pi/crack/unscripted_chats/`,
worker jobs dispatched via the `__chat__` pseudo-slug); and `stages/` the
pipeline stages (auto-discovered `sNN_*.py` modules with a module-level
`STAGE = <Stage>()` — see `stages/base.py`). `static/app.css` / `static/app.js`
hold the few bits of real CSS/JS (linked from `_render_base`).

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
# (note: add/change/delete of prompt files now triggers a background title regeneration)
curl -s -X PUT http://localhost:9847/api/tasks/<task_id>/prompts/prompt.md -d "content=updated"
curl -s -X DELETE http://localhost:9847/api/tasks/<task_id>/prompts/prompt.md
curl -s -X DELETE http://localhost:9847/api/tasks/<task_id>

# background title regeneration (returns a polling placeholder)
curl -s -X POST http://localhost:9847/api/tasks/<task_id>/regenerate-title
curl -s http://localhost:9847/tasks/<task_id>/title-regen-status

# explore the prompt content against the repository (polling)
curl -s -X POST http://localhost:9847/api/tasks/<task_id>/explore
curl -s http://localhost:9847/tasks/<task_id>/explore-status

# plan stage (auto-starts after a successful explore; manual Re-plan here)
curl -s -X POST http://localhost:9847/api/tasks/<task_id>/plan
curl -s http://localhost:9847/tasks/<task_id>/plan-status
curl -s -X POST http://localhost:9847/api/tasks/<task_id>/plan/answers -d "q1=yes&q2=freeform"

# stage config screen + models cache
curl -s http://localhost:9847/stages/plan
curl -s -X POST http://localhost:9847/api/stages/plan/parts/draft/model -d "model=nvidia/nemotron-3-ultra-550b-a55b"
curl -s "http://localhost:9847/stages/plan/template-row/draft.md?editing=true"
curl -s -X PUT http://localhost:9847/api/stages/plan/templates/draft.md -d "content=..."
curl -s http://localhost:9847/api/models
```

Clean up any task directories you create while testing (`DELETE /api/tasks/<id>`
or `rm -rf .pi/crack/tasks/<id>`) — don't leave scratch tasks behind in
`.pi/crack/tasks/`.

### Testing the `pi` CLI itself

The "Regenerate Title" button and the Explore feature shell out to the `pi` CLI
(`pi_runner.run_pi_text` and the stage workers in `stages/`), which is only installed *inside*
`crack-dev` — it won't be on the host `PATH`. Explore also depends on the new tools
installed in the container (`rg`, `fd`/`fdfind`, `fzf`, `bat`/`batcat`, `eza`, `zoxide`,
`jq`). Before debugging a failure, confirm the binaries are available:

```bash
docker exec crack-dev /bin/bash -exc "pi --version"
docker exec crack-dev /bin/bash -exc "rg --version; fd --version; fzf --version; bat --version; eza --version; zoxide --version; jq --version"

# same non-interactive form the title endpoint uses (model, no session/tools, print+exit)
docker exec crack-dev /bin/bash -exc "pi --model nvidia/nemotron-3-nano-30b-a3b -p --no-session --no-tools 'Say hello in 3 words'"
```

Note `pi` has no `run` subcommand and no `--prompt-file` flag — that mismatch
was the cause of the original "regenerate title does nothing" bug. The prompt
text goes in as a plain positional argument, not a file. Since the app's own
server process already runs inside `crack-dev` (see above), pi calls
run as plain `subprocess.run(...)`/`Popen(...)` (via `pi_runner.py`), no `docker exec` wrapper needed —
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
  (`POST /api/tasks/<id>/regenerate-title`) now starts a background job: the
  title is generated from the combined prompt content, then auto-saved to
  `info.json` the first time `GET /tasks/<id>/title-regen-status` observes the
  `"done"` state. There is no more "draft until Save" behavior.
- `.pi/crack/tasks/<task_id>/title_regen.json` — transient background job state
  for title regeneration (`running`/`done`/`saved`/`error`).
- `.pi/crack/tasks/<task_id>/explore.json` — full persisted state of the last
  Explore run: `status, started_at, finished_at, explored_at,
  prompt_last_modified_at, stop_reason, hops_completed, turns_completed,
  found_files, questions, turns[] (each tagged with hop), path_refs[]
  (valid-only: {rel_path, start, end}), summary_md, error`. The task page
  renders the Explore section from this file, so a reload restores the whole
  run with zero new `pi` traffic.
- `.pi/crack/tasks/<task_id>/explore/` — Explore artefact dir:
  `turn_zero.md` and `explore_summary.md` (raw model outputs), plus
  `sessions/` holding the per-task pi session (`explore-<task_id>`) used to
  chain hops. `S01Explore.start` wipes `sessions/` before each fresh run.
- `.pi/crack/tasks/<task_id>/plan.json` — Plan stage state machine:
  `phase` (`draft_running`/`awaiting_answers`/`resuming`/`final_running`/
  `done`/`error`), `round` (1-based), `rounds[]` (each `{questions, answers}`),
  `lay_of_the_land`, `final_md`, `error`, timestamps, and the
  `explore_summary` snapshot the plan was built from.
- `.pi/crack/tasks/<task_id>/plan/` — Plan artefact dir: `draft.md`,
  `round_N_questions.json` / `round_N_answers.json`, `final_plan.md`, plus
  `sessions/` holding the per-task pi session (`plan-<task_id>`) resumed
  across draft steps. `S02Plan.start` wipes `sessions/` before each fresh run.
- `.pi/crack/harness/models_list.json` — cache of `pi --list-models`
  (`{fetched_at, models[]}`), refreshed when older than 24h or via
  `GET /api/models?force=true`; on fetch failure the stale cache (or a
  two-model fallback list) is used.
- `.pi/crack/harness/<slug>.json` — per-stage config, currently just
  `{"models": {part_key: model_id}}` overrides written by the model dropdowns
  on `/stages/<slug>`; `Stage.model_for(part)` falls back to the Part's
  `default_model`.
- `prompt_templates/<slug>/*.md` — per-stage prompt templates, editable from
  `/stages/<slug>` (view/edit-in-place rows, same pattern as task prompts).
  `title.md` stays at the template root — title regen is not a stage.
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

## Background jobs and htmx polling

"Regenerate Title" and every pipeline stage run `pi` in a background
`threading.Thread` because almost every route in this app is a sync `def`
(FastAPI runs them in a threadpool; the plan-answers route is `async def`
only so it can read dynamic form field names via `await request.form()`).
State is persisted to per-task JSON files (`title_regen.json`, `explore.json`,
`plan.json`), so the browser polls for progress rather than blocking the
request.

**Stages** (`stages/` package) are the extensible pipeline concept: each
`sNN_<slug>.py` module defines a `Stage` subclass instance as module-level
`STAGE`; `stages/__init__.py` auto-discovers them into `REGISTRY` (order from
the filename). The home page ("# Harness Stages"), the task page (one
`<section>` per stage via `stage.render_section(task_id)`), and
`/stages/<slug>` all iterate the registry — adding a stage is a new file plus
a `prompt_templates/<slug>/` dir, no app.py changes. Each stage declares
`parts` (model + template per piece); models are overridable per part from the
config screen. A stage's background work is step-driven: each kick
(`start(task_id)`, an answers POST) writes its JSON state and starts one
background step, so no thread blocks waiting on a human.

The polling pattern is standard htmx: the server returns a wrapper element
that carries `hx-trigger="every 1.5s" hx-get=".../status" hx-swap="outerHTML"`
targeting itself. While the job is `"running"` the response still contains
those attributes, so polling continues. Once the response omits them (done or
error), htmx stops automatically. No custom JavaScript is required.

Important implementation details:

- **Title swaps never touch the h1 or buttons.** The header layout is
  `#title-h1-{id}`, `#title-slot-{id}` (a stable `<span>`), and the
  Regenerate/Save buttons as siblings inside `.title-row`. Every dynamic title
  update (input auto-save on blur/change, the Save form, regenerate
  pending/done/error) targets `#title-slot-{id}` with `hx-swap="innerHTML"`
  and updates the h1 via an out-of-band swap (`_render_title_h1(...,
  oob=True)`). Prompt CRUD routes emit an OOB placeholder carrying the slot id
  + `hx-swap-oob="innerHTML"`. Never reintroduce `hx-target="closest header"`
  or outerHTML swaps of elements whose tag changes — that combination was the
  bug that could clobber the whole title row down to a lone input.
- Explore runs in **hops**: up to `EXPLORE_MAX_HOPS` (3) pi invocations of at
  most `EXPLORE_TURNS_PER_HOP` (5) *counted* turns each, chained through one
  pi session (`--session-id explore-<task> --session-dir …/explore/sessions`).
  The worker counts turns and terminates the subprocess at the cap
  because `pi --mode json` has no `--max-turns` flag; the session file is
  written incrementally, so a SIGTERM'd session still resumes cleanly.
  - Turn counting follows `pi_runner.count_turn_groups`: a consecutive streak
    of tool-calling turns increments the cap counter only once (turn budget is
    for model reasoning, not file reads); every non-empty turn is still
    persisted to the trajectory. Content-less turns (empty model responses)
    are neither persisted nor counted; a hop where *every* turn is empty is
    retried, then reported as stop reason `empty`, which stages turn into an
    error instead of a fake "done" (this was the missing-trajectory bug: an
    unauthenticated model returned 60 empty responses that all counted).
  - `run_agent_hop(tools=None)` omits `--tools` entirely → all built-in +
    extension tools (incl. the `mcp` proxy tool from pi-mcp-adapter); the
    stage allowlists name `mcp` explicitly for the same reason.
  - **`--session-id` alone resumes an existing session** — do NOT add
    `--continue`, pi rejects the combination (`Error: --session-id cannot be
    combined with --continue`).
  - Early stop: the explorer is told to emit `EXPLORATION_COMPLETE` on its own
    line when confident (the worker strips the sentinel from displayed text);
    between hops a nano **gate** call (`gate.md`) replies `DONE` or a short
    follow-up list that becomes the next hop's message.
  - The nano gate sometimes mimics the transcript and emits fake tool calls or
    bare commands instead of DONE/bullets — `_gate_reply_is_junk` detects that
    and treats it as DONE (bias toward stopping) rather than feeding garbage
    into the next hop.
  - Stop reasons recorded in `explore.json`: `sentinel`, `gate`, `hop_cap`,
    `turn_cap`, `time_cap`.
- Turn zero, gate, and summary all use the cheap nano model
  (the `turn_zero`/`gate`/`summary` parts) with the ~10k-char input
  limit — `pi_runner.fit_nano_transcript` tail-truncates transcripts to fit (recent
  turns matter most; the blind hard cut in `run_pi_text` would chop them).
- Explore's summary is rendered as HTML via markdown-it-py
  (`MarkdownIt("commonmark", {"html": False})` — raw HTML escaped).
- Title regen auto-saves on the first status poll that sees `"done"`.
- Prompt create/update/delete all kick off a title regen, but update only
  does so when the new content differs from the old.
- **Gotcha that caused "Regenerate Title runs but the page never updates":**
  the pending/polling fragment (`_render_title_regen_pending`) must itself
  carry `hx-trigger="every 1.5s" hx-get=".../title-regen-status"` (targeting
  the slot) — it's easy to write a pending span that just *looks* busy
  (spinner, disabled input) without actually being a self-polling wrapper, in
  which case the background job completes correctly (visible in `docker logs
  crack-dev`) but the browser never asks for the result. Any new polling
  fragment (Explore included) needs the polling attributes on the wrapper
  element itself, not just on the button that started the job.

### Models, providers, and rate limits

Every model currently in use is hosted behind the **nvidia** provider
(`--model nvidia/<id>`, no separate `--provider` flag needed — pi parses the
`provider/id` prefix from `--model` directly):

- `TITLE_MODEL` (in `pi_runner.py`) = `nvidia/nemotron-3-nano-30b-a3b`
  (small/cheap model for the title call — a single-shot tool-less
  `pi_runner.run_pi_text` call)
- Stage part defaults live in each stage's `parts` list (`stages/s01_explore.py`,
  `stages/s02_plan.py`): nano for Explore's turn-zero/gate/summary, ultra
  (`nvidia/nemotron-3-ultra-550b-a55b`) for the tool-using agents and the final
  plan. Per-part overrides are stored in `harness/<slug>.json` and resolved via
  `Stage.model_for(part_key)` — the dropdowns on `/stages/<slug>` take effect on
  the next run without a restart. The dropdown options come from the
  `harness/models_list.json` cache (`models.py`); a saved value is always kept
  as an option even if missing from the cache.

`google/diffusiongemma-26b-a4b-it` was requested at one point but does not
exist in `pi --list-models` under any provider (confirmed after `pi update`)
— `nvidia/nemotron-3-nano-30b-a3b` was chosen instead as the nvidia-hosted
replacement for the title/summary role.

Rate limiting (`RateLimiter` in `pi_runner.py`) is a simple thread-safe
minimum-interval gate, applied via `pi_runner.wait_for_rate_limit(model)` right
before every `pi` subprocess is launched (`run_pi_text` and the streaming hop
runner both call it):

- `_nvidia_limiter` — 40 calls/minute, shared across *all* models above,
  since they're all nvidia-hosted.
- `_model_limiters[TITLE_MODEL]` — an additional 30 calls/minute budget
  specific to that model (also used for Explore's summary call, since it's
  the same model id).
- `TITLE_MAX_INPUT_CHARS` (10,000, a ~4k-token approximation) truncates the
  prompt text before it's sent, applied to both the title call and the
  Explore-summary call.

These limiters only govern the individual `pi` subprocesses this server
launches directly (title regen, Explore's initial launch, Explore's summary
call) — they cannot throttle API calls made *inside* a single already-running
multi-turn Explore process, since `pi` manages that loop internally.

## Explore feature (stage s01)

The Explore section on each task page is stage `s01_explore.py` — a **hopped,
early-stopping** exploration agent that persists everything to disk. Its prompt
templates live in `prompt_templates/explore/` (editable via `/stages/explore`),
and a successful run **auto-starts the Plan stage** (plus the Plan section has a
manual Re-plan button).

1. **Turn zero** (nano, tool-less): reads the concatenated prompts and writes 2–10
   `Q:` questions plus speculative example answers (`turn_zero.md` template; raw
   output stored in `…/explore/turn_zero.md`).
2. **sigmap pre-run** (local, not rate-limited): `sigmap ask '<q>'` for up to 6
   questions, collecting `.context/query-context.md` headers into a context blob
   injected into the hop-1 prompt. The explorer may also run `sigmap ask` itself.
3. **Hops** (`agent` part, ultra by default, `bash,read` tools): up to 3 hops × 5 turns, chained
   through a per-task pi session. Between hops the nano **gate** decides
   DONE/continue; the explorer can also end the run itself with the
   `EXPLORATION_COMPLETE` sentinel. Hard ceilings: 15 turns total, 300 s wall.
4. **Summary** (nano, `explore_summary.md`): markdown overview + trailing
   `path:start-end` bullet list, rendered to HTML (raw HTML escaped) and stored in
   `…/explore/explore_summary.md` + `explore.json["summary_md"]`.

UI: the turns render as one compact **actions table** (one row per
think/text/read/bash/sigmap action; paths middle-truncated with the filename kept,
bash commands in full multiline `<pre>`, outputs truncated at 200 lines/10 000 chars,
honest in/out **character** counts — pi JSON exposes no token counts). **Referenced
files** lists only paths that resolve to real files under the project root
(`workspace/…` and `/workspace/…` forms are normalized in `pi_runner.resolve_path_ref`;
unresolvable candidates are dropped). When prompts are newer than
`explored_at`, a "Prompts changed since last exploration — Re-explore?" banner is
shown above the kept old results; nothing ever auto-runs on page load.

If the Explore run fails (e.g., `pi` rate-limit), the error is surfaced in
`#explore-content` and the turns/references gathered so far are still shown.

## Plan feature (stage s02)

The Plan section (stage `s02_plan.py`) turns an explored task into a structured
implementation plan through an agent-driven Q&A loop, persisted as a step state
machine in `plan.json` (no thread ever blocks on the human):

1. **draft_running** — the draft agent (`draft` part, ultra by default,
   `bash,read` tools, pi session `plan-<task_id>` resumed across steps) reads
   the prompts + explore summary, writes a "lay of the land", then emits either
   ≤5 clarifying questions (a fenced ` ```questions ` JSON block of
   `{id, text, type: single|multiple|open, options?[]}`) or the
   `READY_TO_PLAN` sentinel. If a hop cap cuts it off mid-sweep, the session is
   resumed with a "wrap up now" message (≤3 hops per step).
2. **awaiting_answers** — the Plan section renders an inline form (radios /
   checkboxes / textareas keyed by question id) with **no polling** — it waits
   on the human. `POST /api/tasks/<id>/plan/answers` records
   `round_N_answers.json`, sets `resuming`, and kicks the follow-up step
   (`draft_followup.md` template).
3. Rounds are agent-driven, **hard-capped at 3** (`MAX_ROUNDS`): reaching the
   cap (or `READY_TO_PLAN`) moves to `final_running`.
4. **final_running** — a fresh, tool-less single-shot call (`final` part,
   `final_plan.md` template) whose only context is the original prompt, the
   explore summary, the lay of the land, and all answered Q&A. The template
   mandates the report structure (build/check instructions, problem statement,
   per-path changes with code samples, a NOT-to-change list, automatic + manual
   verification, overview) and the read-only-phase reminder. Output goes to
   `…/plan/final_plan.md` + `plan.json["final_md"]`.
5. **done** — the section renders the final markdown plus a Re-plan button;
   **error** shows the message plus Re-plan.

Gotcha: if the draft agent replies with neither a valid questions block nor
`READY_TO_PLAN`, the step goes straight to `final_running` (logged as a
warning) rather than failing — the alternative is trapping the user in an
unanswerable form.

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
