# crack-server: UI cleanup, auto-retitle, and "Explore" code-search agent

## Context

`.pi/crack/server` is a small FastAPI + htmx app (`app.py` + `paths.py`) for
managing per-task prompt files, already extended once to shell out to the
`pi` CLI for title generation. This round adds several related pieces:

1. Add-Prompt form: stack filename above content (currently side-by-side),
   and collapse the whole section behind a `<details>` disclosure.
2. "Regenerate Title" should also **save** the title, and title regen should
   fire automatically whenever prompt file content actually changes —
   confirmed with the user this happens **in the background** (save
   returns instantly, title updates via polling) rather than blocking the
   request, since a `pi` call takes ~2-5s.
3. A new **Explore** feature: a second `pi` sub-agent run in JSON-streaming
   mode (`--mode json`), given the concatenated prompt content, allowed to
   use `bash`/`read` (and thus `rg`, once installed) to search the repo,
   capped at 10 turns, streamed to the browser turn-by-turn via polling,
   followed by a final summarization call. File-path-looking strings in its
   output are regex-matched against real files under the project root and
   rendered as collapsibles with the referenced line range shown inline.
4. All three `pi` prompt templates move to external files under
   `.pi/crack/server/prompt_templates/`.
5. The dev container gets more CLI tools (ripgrep, fzf, bat, eza, fd-find,
   zoxide, jq — lazygit skipped, not apt-available on Debian). Dockerfile
   updated for future rebuilds, **and** installed live into the running
   `crack-dev` container via `docker exec ... apt-get install`, no rebuild.

I verified directly against the running container:
- `pi --mode json -p --tools bash,read,rg --model <id> "<prompt>"` streams
  newline-delimited JSON events: `session`, `agent_start`, `turn_start`,
  `message_start`/`message_update`/`message_end` (assistant text/thinking,
  `toolCall` content blocks, `toolResult` role messages),
  `tool_execution_start/update/end`, `turn_end`, `agent_end`,
  `agent_settled`. `rg` gets invoked *through* the `bash` tool, not as a
  separate named tool — pi has no distinct `rg` tool.
- No `--max-turns` flag exists, so the 10-turn cap must be enforced by the
  caller: count `turn_end` events and terminate the subprocess once 10 are
  seen.
- `apt-cache policy` for the new packages came back empty in the current
  container because `apt-get update` hasn't been run there yet — the
  implementation step must run `apt-get update` first and confirm exact
  package/binary names (Debian sometimes ships `bat`→`/usr/bin/bat` directly
  rather than Ubuntu's `batcat` rename; verify rather than assume, and only
  add a compat symlink if the binary actually lands under a different name).

## 1. Prompt templates → external files

New dir `.pi/crack/server/prompt_templates/`:
- `title.md` — today's `PROMPT_TITLE_TEMPLATE` body, moved verbatim
  (`{content}` placeholder).
- `explore.md` — new. Given `{content}` (concatenated prompt files),
  instructs the agent to explore the repo with its tools and, but does not
  strictly require ending with paths itself — see `explore_summary.md`.
- `explore_summary.md` — new. Given `{content}` and `{transcript}` (the
  rendered turn-by-turn record of the explore run), asks for a short
  overview plus a trailing bullet list of `path:start-end` references.

`app.py` gets `TEMPLATES_DIR = Path(__file__).resolve().parent.parent.parent / "prompt_templates"`
(three `.parent`s from `src/crack_server/app.py` lands on `.pi/crack/server/`)
and `_load_template(name: str) -> str` that reads `{TEMPLATES_DIR}/{name}.md`
fresh on every call (matches the existing no-caching philosophy, and lets
template edits hot-apply). `PROMPT_TITLE_TEMPLATE` constant is deleted.

## 2. Add Prompt form: stack fields, collapse section

In `task_page()`, replace the current `<section class="add">` (flex row
with `Filename` and `Content` side by side) with:

```html
<details class="add">
  <summary style="font-size: 0.95rem; cursor: pointer;">Add Prompt</summary>
  <form hx-post=... hx-target="#prompt-list" hx-swap="innerHTML" hx-on::after-request="this.reset()">
    <label>Filename (optional) <input ...></label>
    <label>Content <textarea ...></textarea></label>
    <button type="submit">Add Prompt</button>
  </form>
</details>
```

Dropping the `display:flex` wrapper is enough to stack the two labels
vertically (pico renders block-level `<label>`s stacked by default).
`<details>/<summary>` needs no JS — pico styles it natively, closed by
default, satisfying "collapsed under smaller title font, only when expanded
show the form and Add button."

## 3. Unified background "regen" pattern (title + explore)

Both auto-retitle and Explore need the same shape: kick off a background
`pi` subprocess, let the browser poll for progress, stop polling on
completion. Implement once, reuse twice.

**State storage** — new `paths.py` helpers, following the existing
`read_info`/`write_info` pattern (atomic-ish write via tmp file + `os.replace`):
- `read_title_regen_state(task_id) -> dict` / `write_title_regen_state(task_id, state)`
  → `task_dir/title_regen.json`
- `read_explore_state(task_id) -> dict` / `write_explore_state(task_id, state)`
  → `task_dir/explore.json`
- `read_all_prompts_joined(task_id) -> str` — factor the existing
  "glob prompts, read each, join with `\n\n---\n\n`" loop (currently
  inlined in `api_regenerate_task_title`) out into `paths.py` since it's
  filesystem access and will now be called from three places.

**Job runner** — plain `threading.Thread(daemon=True)`, not asyncio: every
route in this app is a sync `def` (FastAPI already runs those in a
threadpool), so a plain thread matches the existing style and needs no new
async infrastructure.

**`pi` helper refactor** — rename `_run_pi_title_generation(prompt)` to a
generic `_run_pi_text(prompt: str, log_prefix: str) -> str` (same body:
logs full prompt/command/timeout/elapsed/output summary, `subprocess.run`
with `--print --no-session --no-tools`), but make it raise plain
`RuntimeError` instead of `HTTPException` — it now only ever runs inside a
background thread (no request context to turn into an HTTP error). Reused
by both title regen and the explore summarization call.

**htmx polling pattern** (new to this codebase, but standard htmx): a
polling fragment is a wrapper element carrying its own
`hx-trigger="every 1.5s" hx-get=".../status" hx-swap="outerHTML"`, targeting
itself. The status endpoint returns a fresh copy of that same wrapper
(still polling) while running, or the terminal markup with **no**
`hx-trigger`/`hx-get` once done/error — htmx simply stops polling once the
polling attributes are gone from the DOM. No custom JS needed.

## 4. Auto-retitle wiring

- `_render_title_input` (terminal state, unchanged) and a new
  `_render_title_regen_pending(task_id) -> str` (a `<span id="title-input-{id}">`
  wrapping a disabled input + pico `aria-busy` spinner, with the polling
  attrs above) **share the same `id`** so both self-polling and
  out-of-band swaps can target it.
- `_start_title_regen_job(task_id)`: reads current state, no-ops if already
  `"running"` (prevents duplicate jobs), else writes `{"status": "running", ...}`
  and starts the thread. Worker: `paths.read_all_prompts_joined`,
  `_load_template("title")`, `_run_pi_text(...)`, writes `{"status": "done", "title": ...}`
  or `{"status": "error", "error": ...}`.
- `POST /api/tasks/{id}/regenerate-title`: now just calls
  `_start_title_regen_job` and returns `_render_title_regen_pending` (was
  synchronous before).
- New `GET /tasks/{id}/title-regen-status`: reads state, returns pending
  wrapper (running) or `_render_title_input(...)` with the new title AND
  **writes it via `paths.write_info`** (title regen now auto-saves, per
  requirement 2 — no more "draft until Save" behavior) on first observing
  `"done"`.
- `api_create_prompt`, `api_update_prompt` (only if new content != old,
  read-before-write to compare), `api_delete_prompt`: after the filesystem
  change, call `_start_title_regen_job(task_id)`, then append
  `_render_title_regen_pending(task_id)` **with `hx-swap-oob="true"`
  added** to the normal response fragment, so the header's title area picks
  up the pending/polling state even though these routes target
  `#prompt-list` / `closest article`, not the header.

## 5. Explore feature

**UI** — new section in `task_page()`, after the (now-collapsed) Add Prompt
`<details>`:
```html
<section class="explore" id="explore-section">
  <h2>Explore</h2>
  <button hx-post="/api/tasks/{id}/explore" hx-target="#explore-section" hx-swap="innerHTML">Explore</button>
</section>
```
Explore always operates on the full current prompt content (same
`paths.read_all_prompts_joined` used for titles) — no separate input.

**Endpoints**:
- `POST /api/tasks/{id}/explore`: guards against a run already `"running"`
  (returns current status instead of double-starting), else writes initial
  `explore.json` state and starts the worker thread; returns the polling
  wrapper (turns-so-far, empty on first call).
- `GET /tasks/{id}/explore-status`: renders current state — see below.

**Worker** (`_run_explore_job(task_id)`):
1. `content = paths.read_all_prompts_joined(task_id)`; prompt =
   `_load_template("explore").format(content=content)`.
2. `cmd = ["pi", "--mode", "json", "-p", "--no-session", "--model", PI_MODEL, "--tools", "bash,read", prompt]`
   — log full prompt + `+`-prefixed `shlex.join`'d command + timeout, same
   style as the existing title-gen logging.
3. `subprocess.Popen(cmd, stdout=PIPE, text=True)`, iterate stdout lines,
   `json.loads` each, accumulate per-turn content (assistant thinking/text,
   `toolCall`/`toolResult` pairs) keyed by a running turn counter that
   increments on `turn_end`. After each `turn_end`: append the finished
   turn to `state["turns"]`, re-scan cumulative text for path references
   (see below), persist state to disk (so polling always sees fresh
   progress) and log a one-line INFO summary of that turn.
4. Stop condition: once 10 `turn_end`s have been processed,
   `proc.terminate()` and stop reading. Also enforce an overall wall-clock
   timeout (`PI_EXPLORE_TIMEOUT_SECONDS`, e.g. 300s) checked each loop
   iteration as a best-effort guard (not preemptive against a fully hung
   process with zero output — acceptable for a local dev tool).
5. Final summarization: fresh, separate `pi` call via `_run_pi_text`
   (tool-less, single-shot) using `_load_template("explore_summary")`
   fed `content` + a rendered plaintext transcript of `state["turns"]`.
   Store as `state["summary"]`, re-scan it for path references too, mark
   `state["status"] = "done"`. Any exception along the way →
   `state["status"] = "error"`, `state["error"] = str(e)`.

**Path-reference detection** (`_extract_path_refs(text: str) -> list[dict]`):
regex like `` `?([A-Za-z0-9_][A-Za-z0-9_./-]*\.[A-Za-z]{1,10})`?(?::(\d+)(?:-(\d+))?)? ``
over the cumulative turn text (+ summary). For each match, resolve against
`paths.project_root()`, require the resolved path both exists as a file
**and** stays under project root (blocks `../` escapes) before accepting
it as valid; dedupe by `(path, start, end)`. Invalid-looking candidates are
kept too (so the UI can show "referenced but not found") but marked
`valid: False`.

**Rendering** (`GET /tasks/{id}/explore-status`):
- One `<article>` per turn: role-tagged, showing assistant text (and a
  collapsed `<details>` for thinking, if present) plus one line per tool
  call (`🔧 bash: rg -l foo` collapsed with its output) — "friendly
  format" per the request, not raw JSON.
- Path references section at the bottom: one `<details>` per ref, `open`
  when valid (showing `path:start-end` in the summary and the actual file
  lines — read via a small `_read_file_lines(root, rel_path, start, end)`
  helper, clamped to file length, capped at ~200 lines shown), closed with
  just the raw text when not resolvable.
- If `state["status"] == "done"`: summary text rendered above the turns;
  wrapper omits polling attrs (stops polling). `"error"`: error message,
  no polling attrs. `"running"`: wrapper keeps `hx-trigger="every 1.5s"`.

## 6. Docker: new CLI tools

`_docker/Dockerfile`: add `ripgrep fzf bat eza fd-find zoxide jq` to the
existing `apt-get install -y --no-install-recommends \` list (next to
`tmux`/`htop`). Follow with a guarded symlink step (only fires if the
binary landed under a different name than expected):
```dockerfile
RUN (command -v bat >/dev/null || ln -sf /usr/bin/batcat /usr/local/bin/bat) ; \
    (command -v fd  >/dev/null || ln -sf /usr/bin/fdfind /usr/local/bin/fd) ; true
```

**Live install** (implementation step, run for real via `docker exec`, not
during planning): `docker exec crack-dev /bin/bash -exc "apt-get update"`
first, then `apt-cache policy ripgrep fzf bat eza fd-find zoxide jq` to
confirm every package actually resolves on this Debian release before
`apt-get install -y ...`. If `eza` isn't in the base repo, fall back to
`cargo install eza` (rust toolchain is already in the image) rather than
skipping it. Then re-run `pi --help`-style sanity checks (`rg --version`,
`fd --version` or `fdfind --version`, etc.) to confirm the explore feature
can actually shell out to them.

## Files touched

- `.pi/crack/server/prompt_templates/title.md` (new)
- `.pi/crack/server/prompt_templates/explore.md` (new)
- `.pi/crack/server/prompt_templates/explore_summary.md` (new)
- `.pi/crack/server/src/crack_server/paths.py` — add state read/write
  helpers + `read_all_prompts_joined`
- `.pi/crack/server/src/crack_server/app.py` — template loader, add-prompt
  markup, title-regen background wiring + new status route, explore
  feature (worker, two routes, rendering helpers), constants
  (`PI_EXPLORE_MAX_TURNS = 10`, `PI_EXPLORE_TIMEOUT_SECONDS`)
- `_docker/Dockerfile` — new apt packages + compat symlinks
- `.pi/crack/server/AGENTS.md` — document the new endpoints, the
  background-job/polling pattern, and the explore feature once built (this
  file already tracks curl examples and gotchas for future agents)

## Verification

The server is already live-reloading at `http://localhost:9847` — use curl
against a scratch task the same way prior work in this file did:
1. Create a task, add 1-2 prompt files, confirm the title auto-regenerates
   in the background (`GET .../title-regen-status` transitions
   running→done and `info.json`'s title actually changes on disk).
2. Confirm the Add Prompt section renders collapsed (`<details>` closed)
   with filename stacked above content when opened.
3. Click Explore (or `POST /api/tasks/{id}/explore`), poll
   `explore-status` a few times, confirm turns accumulate, tool calls show
   up, it stops at 10 turns or fewer, a summary appears, and at least one
   path reference resolves to a real file with the right lines shown.
4. `docker exec crack-dev /bin/bash -exc "pi --version"` still works
   (already documented); after live-installing packages, also check
   `rg --version`, `fzf --version`, `bat --version` (or `batcat`),
   `eza --version`, `fd --version` (or `fdfind`), `zoxide --version`,
   `jq --version` inside the container.
5. `docker logs crack-dev` shows the new explore logging (full prompt,
   `+`-prefixed command, per-turn summaries, final elapsed time) — same
   diagnosability bar as the existing title-regen logging.
6. Clean up the scratch task afterward (`DELETE /api/tasks/<id>`).
