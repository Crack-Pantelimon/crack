# crack-server: Explore v2 — hopped exploration, persistence, pretty UI, bug fixes

## Context

The `.pi/crack/server` "Explore" feature (a `pi` JSON-mode sub-agent that searches
the repo for code relevant to a task's prompts) shipped and works, but has real
problems in practice:

- It **always burns all 15 turns** — no early stop, no sense of "enough found".
- Turn output is rendered as verbose cards with **end-truncated paths** (uselessly
  hiding the filename), bash shown truncated, reads shown raw and unbounded.
- **Nothing is persisted**: reloading the task page shows an empty Explore section
  until you click Explore again (and pay for a fresh run).
- **Referenced-files resolution is broken**: the project root is `/workspace`, and
  the model emits paths like `workspace/src/lib.rs` or `/workspace/src/lib.rs`,
  which resolve to `/workspace/workspace/...` and fail — so valid files show as
  "Could not resolve".
- The **summary is shown as escaped plaintext**, not rendered markdown.
- **"Regenerate Title" is buggy**: it can wipe the whole title row (h1 + buttons),
  leaving only an input.

This round reworks Explore into a cheaper, early-stopping, hop-based agent with full
persistence and a compact table UI, fixes the path + title bugs, and renders the
summary as HTML. Decisions below were confirmed with the user (12 questions).

## Confirmed decisions

- **Hops:** up to 3 hops of ≤5 tool-turns each (15-turn hard ceiling, 300s wall cap).
  Between hops a cheap chat-only **retrospective gate** decides continue/stop.
- **Hop memory:** real `pi` sessions via `--session-id` + `--continue`, stored in an
  isolated per-task dir `…/<task>/explore/sessions/` (`--session-dir`).
- **Early stop:** model emits an `EXPLORATION_COMPLETE` **sentinel** when confident;
  the gate is the backstop. Truncation by cap still counts as valid if ≥1 real file
  was found.
- **Turn zero:** cheap **nano** model, as a separate pre-step, writes 2–10 questions +
  hallucinated example answers; stored and fed into hop 1.
- **Table:** one row **per action** (tool call / message): `Type | path-or-command
  (middle-truncated) | in/out size`. Shown **above** the summary.
- **Size stats:** honest **character counts** (pi JSON exposes no token counts).
- **Markdown:** `uv add markdown-it-py`; render the summary → HTML (raw-HTML disabled).
- **Referenced files:** show **only** paths that resolve to real files; drop the rest.
- **Stale results:** show a "prompts changed — Re-explore?" **banner but keep old
  results**; never auto-run on load.
- **Gate/turn-zero model:** **nano** (reuse `EXPLORE_SUMMARY_MODEL`).
- **sigmap:** **both** — auto-run `sigmap ask '<q>'` on turn-zero questions and inject
  the headers into hop 1, AND tell the model in the explore prompt it can run
  `sigmap ask '<q>'` then `read .context/query-context.md` itself.

## Files touched

- `.pi/crack/server/src/crack_server/app.py` — most changes (explore worker rewrite,
  rendering, path fix, title fix, markdown).
- `.pi/crack/server/src/crack_server/paths.py` — explore artefact/dir helpers,
  prompt-mtime helper.
- `.pi/crack/server/prompt_templates/` — rework `explore.md`; add `turn_zero.md`,
  `gate.md`; keep `explore_summary.md` (minor tweak).
- `.pi/crack/server/pyproject.toml` / `uv.lock` — add `markdown-it-py`.
- `.pi/crack/server/AGENTS.md` — document the new engine, artefacts, and gotchas.

## 1. Title "Regenerate" bug (fix first, independent)

**Root cause:** every title swap uses `outerHTML` on an element whose tag alternates
between `<input>` and `<span>` under one shared id `title-input-{id}`, and the input's
own `blur`/`change` auto-save targets `closest header` (re-rendering the entire header).
Clicking Regenerate while the input is focused fires the blur→full-header re-render and
the regenerate swap concurrently; combined with the tag-swapping-under-one-id OOB
target, the h1+buttons can get clobbered down to a lone input.

**Fix — stable slot + innerHTML swaps** (in `_render_task_header`,
`_render_title_input`, `_render_title_regen_pending`, `_render_title_regen_error`,
`title_regen_status`, and the OOB call sites in the prompt CRUD routes):

- Wrap the title control in a stable `<span id="title-slot-{id}" class="title-slot">`
  that is a **sibling** of the `<h1 id="title-h1-{id}">` and the Regenerate/Save
  buttons, and never changes tag.
- All title swaps target `#title-slot-{id}` with `hx-swap="innerHTML"` (pending, status
  result, error) — the h1 and buttons are outside the slot and can never be removed.
- Regenerate button: `hx-target="#title-slot-{id}" hx-swap="innerHTML"`.
- On completion, also update the heading text via an **OOB** swap into
  `#title-h1-{id}` so the big title reflects the new value (today it silently keeps the
  old h1 text).
- Change the input's auto-save trigger to target `#title-slot-{id}` (innerHTML) + OOB
  h1, not `closest header`, so a blur can never nuke the header.
- OOB placeholders from prompt CRUD target `#title-slot-{id}`.

Verify live via curl + browser: focus the title, click Regenerate, confirm h1 and both
buttons survive and the title updates when the job finishes.

## 2. Path-reference resolution fix (`_extract_path_refs`, `app.py`)

Normalize each candidate to a project-relative path before checking existence, trying
in order and taking the first that is an existing file **under** `project_root()`:

1. absolute path starting with `str(root)` → strip that prefix.
2. leading `root.name + "/"` (e.g. `workspace/…`) → strip it.
3. plain `root / candidate`.

Keep **only** resolved refs (drop unresolved entirely, per decision). Dedup by
`(rel_path, start, end)`. `_render_path_ref` no longer needs an "unresolved" branch.
Reuse the existing `_read_file_lines` for inline display, but apply the read-truncation
rules from §3 (200 lines / 10 000 chars) with a friendly marker.

## 3. Pretty turn display → compact actions table (`app.py` rendering)

Replace `_render_explore_turn` cards with a single compact table rendered **above** the
summary. Build a flat action list from `state["turns"]`; one `<tr>` per action:

- **Type** (left): `text` / `think` / `read` / `bash` / `sigmap` (derive `read`/`bash`
  from the bash command; flag `sigmap` when the command starts with `sigmap`).
- **Path / command** (middle): for reads, the path **middle-truncated** (keep head +
  full filename tail, e.g. `/workspace/…/plugins/sky_render.rs`) via a
  `_truncate_middle(s, max=60)` helper. For bash, the **full multiline command in a
  `<pre>`** (not truncated). For text/think, a short first-line snippet expandable to a
  `<details>`.
- **Size** (right): `in`/`out` **character counts** (assistant text length; tool
  input/output lengths), e.g. `in 240 / out 1.2k`.

Read-tool output shown inside an expandable `<details>` is truncated to **200 lines or
10 000 chars**, whichever first, with a marker like
`… [truncated at 200 lines — ask the agent to read specific line ranges if needed]`.
Bash commands are shown in full (multiline `<pre>`); bash **output** follows the same
200-line/10k truncation as reads.

## 4. Explore worker rewrite (`_run_explore_job`, `app.py`)

New flow (all persisted to `explore.json` after every step so polling/reload see live
progress):

1. `content = read_all_prompts_joined`; record `prompt_last_modified_at` = max prompt
   mtime (new `paths.prompts_last_modified(task_id)`).
2. **Turn zero (nano):** `_run_pi_text(turn_zero_prompt)` → 2–10 questions + example
   answers. Persist raw text to `…/explore/turn_zero.md` and `state["questions"]`.
   Parse individual questions (one per `Q:`-prefixed line).
3. **sigmap pre-run:** for up to ~6 questions, run `sigmap ask '<q>'` via subprocess
   (local, not rate-limited), then read `.context/query-context.md`; collect headers
   into a `sigmap_context` blob injected into the hop-1 prompt.
4. **Hops loop** (`hop = 1..3`, while total turns < 15 and elapsed < 300s):
   - Hop 1 prompt = `explore.md` with `{content}`, `{questions}`, `{sigmap_context}`;
     later hops send the **gate's follow-up** as a `--continue` message.
   - `pi --mode json -p --model EXPLORE_MODEL --tools bash,read
     --session-id explore-{task} --session-dir …/explore/sessions [--continue] "<msg>"`.
     Stream events; cap this hop at **5 `turn_end`s** → `proc.terminate()`.
   - Persist each finished turn (reuse `_apply_explore_event_to_turn` /
     `_persist_explore_turn`, extended to tag `hop`).
   - If assistant text contains `EXPLORATION_COMPLETE` → `stop_reason="sentinel"`, break.
   - If total turns ≥ 15 or elapsed ≥ 300s → `stop_reason="turn_cap"/"time_cap"`, break.
   - Else **gate (nano)** `gate.md` given questions + transcript-so-far: replies `DONE`
     (→ `stop_reason="gate"`, break) or a short list of what's still worth checking
     (→ next hop's `--continue` message).
5. **Summary (nano):** `explore_summary.md` → markdown; store to
   `…/explore/explore_summary.md` and `state["summary_md"]`.
6. `path_refs` = valid-only extraction over transcript+summary. Set `finished_at`,
   `explored_at = finished_at`, `status="done"`, and `found_files = len(path_refs)`.
   All exceptions → `status="error"` with message (partial turns preserved).

Session dir is isolated per task; `_start_explore_job` clears any stale
`…/explore/sessions/` before a fresh run so hops chain cleanly.

## 5. Persistence & reload (`paths.py` + `task_page`)

`explore.json` schema grows: `status, started_at, finished_at, explored_at,
prompt_last_modified_at, stop_reason, hops_completed, turns_completed, found_files,
questions, turns[] (+hop), path_refs[], summary_md, error`. Summary + turn-zero also
written as `.md` artefacts under `…/explore/` (per the user's artefact-dir request).

- New `paths.explore_dir(task_id)` → `…/<task>/explore/`; `write_explore_artefact(
  task_id, name, text)` → `…/explore/{name}.md`; `prompts_last_modified(task_id)`.
- `task_page` renders the Explore section **from stored `explore.json`** on load
  (turns table + summary + refs), instead of only an empty Explore button. If
  `explored_at < prompt_last_modified_at`, show a "prompts changed since last
  exploration — Re-explore?" banner above the (still-visible) old results. Never
  auto-run. Show an `explored X ago · N turns · M files · stop: <reason>` metadata line.

## 6. Prompt templates

- `explore.md` (rework): explorer instructions + `{questions}` + `{sigmap_context}`;
  tell it to use `bash`/`read`/`sigmap ask` (+ how to read `.context/query-context.md`);
  emphasize ≤5 tool-turns this hop, be concise, cite `path:line-range`, and **emit
  `EXPLORATION_COMPLETE` on its own line once it has enough** to answer the questions.
- `turn_zero.md` (new): "write 2–10 questions (as `Q:` lines) about what code we need to
  find for this task, then give a plausible hallucinated example answer to each."
- `gate.md` (new): given the questions and transcript so far, "list anything important
  still worth checking; if the questions are sufficiently answered, reply exactly
  `DONE`." Bias toward stopping.
- `explore_summary.md`: keep; ensure it asks for markdown with a trailing
  `path:start-end` bullet list.

## 7. Markdown rendering & dependency

`docker exec crack-dev bash -lc 'cd .pi/crack/server && uv add markdown-it-py'`.
Add `_render_markdown(md) -> str` using `MarkdownIt("commonmark")` with raw HTML
disabled; use it for the summary block (`class="explore-summary"`). Server auto-reloads.

## Verification (live, against http://localhost:9847 + `docker exec crack-dev`)

1. **Title bug:** focus the title input, click Regenerate — h1 + both buttons survive,
   pending spinner shows only in the slot, title (h1 + input) updates when done.
2. **Explore run** on a scratch task: poll `explore-status`; confirm turn-zero questions
   persist, hops chain via sessions, the run **early-stops** (sentinel or gate) before
   15 turns on a simple prompt, and `stop_reason` is recorded.
3. **Table:** actions render one row each, paths middle-truncated (filename visible),
   bash shown full multiline, reads truncated at 200 lines/10k with the marker; size
   column shows char counts.
4. **Refs:** a real repo file the agent cites resolves and shows inline lines; a
   `workspace/…`-prefixed path resolves correctly; bogus paths are absent.
5. **Summary** renders as HTML (headings/lists/code), not escaped text.
6. **Reload:** refresh the task page — turns/table/summary/refs restore from disk with
   no new `pi` traffic; edit a prompt, reload, confirm the stale banner appears while
   old results remain.
7. `docker logs crack-dev` shows per-hop logging (prompt, `+`command, per-turn
   summaries, gate decision, elapsed) and rate-limit waits. Clean up the scratch task.
