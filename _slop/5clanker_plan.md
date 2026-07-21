# Fix chat/streaming bugs, delete the rigid harness, add real-time status UI

## Context

The crack-pi server has two products fused together: a **rigid harness** (tasks →
staged pipeline: explore → plan → plan_review → implementation → impl_review →
finished) and **unscripted chats** (free-form pi sessions that spawn recursive
sub-agents). The user is abandoning the rigid harness entirely and concentrating
only on chats + sub-agents. On top of that, two bugs are actively "plaguing"
chats, and the chat/sub-agent UI needs real-time status affordances.

Confirmed decisions: **delete harness code *and* on-disk data**; render recursive
sub-agents as **fully inline nested transcripts** with colored borders.

### Bug root-causes found during exploration

1. **Live message duplication** (the "6× identical `Clanker:` / repeated think
   tag" report). It is *not* in persisted state — `chat.json` for the affected
   chat holds that text exactly once. It is a **streaming/delta bug**. The live
   append protocol (`app.js fetchChatDelta`) sends `after=<max msg index in DOM>`
   and appends server msgs with index > after via `beforeend`. This requires msg
   fragments to be **stable-indexed and append-only**. `chats.render_chat_answer`
   ([chats.py:123](.pi/crack/server/src/crack_server/chats.py#L123)) violates
   both: it joins *all* agent-turn texts of an exchange into **one combined
   `Clanker:` markdown block** that grows and gets re-indexed on every poll while
   the exchange streams. Each poll the block has a new (higher) index and more
   text, so `beforeend` appends *another* growing copy and never removes the
   earlier partial ones → N stacked duplicates.

2. **`agent_end willRetry` truncation** — already fixed earlier this session in
   [pi_proc.py:682](.pi/crack/server/src/crack_server/pi_proc.py#L682) (kept).

### Tool-result error signal
pi's `toolResult` messages carry a boolean **`isError`** field (verified live in
the hop jsonl and in pi's `messages.d.ts`). We currently drop it —
[transcript.apply_event_to_turn](.pi/crack/server/src/crack_server/transcript.py#L61)
merges only `output` into the tool block. Capturing `isError` gives us red/green
dots for free; a tool block with a `toolCall` but no result yet = pending.

---

## Phase A — Fix the streaming duplication bug

**Goal:** make chat live-msgs stable-indexed & append-only, matching how stages
already stream (persisted turns = stable indexed msgs; in-flight/partial lives in
the volatile `#chat-tail`, which is fully replaced each poll).

- Rework `chats.render_chat_answer` → emit **one stable msg fragment per persisted
  turn** (each turn's tool rows + that turn's own assistant text rendered inline),
  and **remove the single combined `Clanker:` block**. Reuse
  `render.render_turn_msgs` (already one-fragment-per-turn) with `include_text=True`
  instead of the current `include_text=False` + hand-rolled combined block.
- Chat turns are persisted incrementally into `chat.json` mid-exchange (worker
  calls `persist_turn` per turn), so per-turn rendering still streams live — each
  new persisted turn is a new higher index appended exactly once.
- Keep the running spinner / stop button in `render_chat_tail` only (already
  volatile). No `app.js` protocol change needed once indices are append-only.
- **Verify** with the reproduced chat: `1784588202230` renders the text once.

## Phase B — Delete the rigid harness (code + data)

**Extract shared helpers first** (chats + sub-agents depend on them), then delete
`stages/`:
- `stages/render.py` → new neutral `src/crack_server/render.py`. Keep the
  transcript/turn/tool/error/spinner/`model_select`/`render_exchanges` helpers
  (used by chats, `routes_sub_agents`). **Drop** the `Stage`-typed tail widgets
  (`render_retry_button`, `render_stop_button`, `render_running_tail`,
  `render_message_form`).
- `stages/qa.py` → new neutral `src/crack_server/questions.py`
  (`render_questions_form` + parse helpers; used by `sub_agents/planner.py` and
  the chat run tree).
- Update importers: [chats.py:40](.pi/crack/server/src/crack_server/chats.py#L40),
  [chats.py:191](.pi/crack/server/src/crack_server/chats.py#L191),
  [sub_agents/planner.py:11](.pi/crack/server/src/crack_server/sub_agents/planner.py#L11),
  [routes_sub_agents.py:12](.pi/crack/server/src/crack_server/routes_sub_agents.py#L12).

**Delete files:** `stages/` (s01–s06, base, steprun, render, qa, __init__),
`routes_stages.py`, `routes_tasks.py`.

**Rebuild the home page + title job:** the chats-only home (New Chat + recent
chats + Sub-agents/Settings links) moves into `routes_chats.py` as the `GET /`
route (reuse `chats.render_home_section`, drop task cards + "Harness Stages"
list). The task title-regen queue job (`TITLE_JOB_SLUG`, `_run_title_regen_worker`)
is harness-only — remove it; chats already self-title inline via
`chats._maybe_generate_title`.

**Trim shared modules:**
- `app.py`: drop `routes_stages`/`routes_tasks` includes + `TITLE_JOB` re-export.
- `worker.py`: drop the stage-dispatch branch, the `TITLE_JOB` branch, and the
  stage half of the orphan watchdog; keep chat + sub-agent + models dispatch and
  the per-run orphan check.
- `ui.py`: rewrite `_render_sidebar` (Home, Sub-agents, Settings, Chats — no
  Tasks/Harness Stages). Delete task-only renderers (`_render_title_*`,
  `render_file_row`, `_render_prompt_row`, `_render_prompts_section`). Keep
  `_render_base`, markdown/esc/time helpers.
- `paths.py`: remove all task/stage/plan/explore/impl/review/finished accessors
  (`tasks_dir`, `task_dir`, `create_task`, `list_task_ids`, prompt fns,
  `stage_*`, `title_regen_state`, the six stage state/dir/artefact fns).
  **Keep** chat/run/sub-agent accessors, `harness_dir` + `queue_*` +
  `models_cache_state` (shared infra), `hop_*`, media/attachments.
- `state.py`: drop `TASK_STATE_FILENAMES`/`task_state_mtimes` and the unused
  stage/split filename constants.
- Sweep for now-dead imports/references (`grep` for `stages`, `task_`, each
  deleted symbol) until `python -m pytest` import-collects clean.

**Delete on-disk data (destructive, authorized):** remove `.pi/crack/tasks/` and
the per-stage config JSONs `.pi/crack/harness/s0*.json`. **Preserve**
`.pi/crack/harness/queue/`, `.pi/crack/harness/models_list.json`,
`.pi/crack/harness/worker.lock` (shared), `.pi/crack/sub_agents/` (personas), and
`.pi/crack/unscripted_chats/`.

**Tests:** delete/trim harness-only tests; keep and green chat/sub-agent/pi_proc
tests. `tests/test_plan41.py` fake_pi shim stays.

## Phase C — Tool-call notification dots

- Capture status in `transcript.apply_event_to_turn`: on `toolResult` merge set
  `block["is_error"] = bool(message.get("isError"))`. A block with a `toolCall`
  and no result yet stays "pending".
- In `render._render_tool_action_row`, prepend a dot to the type cell:
  `<span class="tool-dot tool-dot--{ok|err|pending}">`. Classify: has `is_error
  is True` → err; has output/`is_error is False` → ok; else pending.
- CSS in `app.css`: `.tool-dot--ok{green}`, `--err{red}`, `--pending{` pulsating
  blue→white `@keyframes}`. (Pending only appears live; persisted history is all
  resolved.)

## Phase D — Recursive sub-agent inline transcripts with borders

Replace the flat `chats.render_run_tree`
([chats.py:189](.pi/crack/server/src/crack_server/chats.py#L189)) with a
**recursive** renderer:
- Build a tree of runs from `run.json` `parent_id`/`parent_kind`/`depth` (root =
  runs whose parent is the chat).
- Render each run as a bordered card
  `<div class="subagent-card phase-{running|awaiting|done|error|stopped}">`
  containing: header (persona, depth, phase, live status dot), the run's **full
  transcript** inline (`render.render_turn_msgs(run turns)` incl. tool dots),
  existing Stop/Retry/answer/planner forms, then its **child run cards nested
  recursively** inside the same bordered container.
- Border colors via CSS on the phase classes: **blue** = running-ish
  (`running`/`resuming`/`revising`/`awaiting_answers`), **dark green** = `done`,
  **orange** = `awaiting_user`. (Error/stopped = red-ish, existing behavior.)
- Keep the existing 2s htmx poll on `#subagent-run-tree` while any run is active.

## Phase E — Home + navbar real-time status dots (long-poll last 5)

- **Server status helper** in `chats.py`: `chat_status_dot(chat_id) -> dict` →
  `{"phase": <chatting|awaiting|idle|error>, "tool": <ok|err|pending|none>}`
  where `tool` = last exchange's last turn's last tool block status.
- **Dot markup:** a `<span class="chat-dot dot-{phase}" data-chat-id=...>` with a
  nested `<span class="chat-dot-inner tool-{color}">`. Outer symbol/animation by
  phase: pulsating blue = running (`chatting`), hollow circle = awaiting user
  input, check = idle/done, ✕ = error. Inner small dot colored by last tool.
  Render these placeholders in `_render_sidebar` chat links and
  `render_home_section` cards.
- **Long-poll endpoint** in `routes_chats.py`, mirroring the existing
  `chat_wait` mtime pattern ([routes_chats.py:50](.pi/crack/server/src/crack_server/routes_chats.py#L50)):
  - `GET /api/chats/dots` → JSON `{id: chat_status_dot(id)}` for the last 5 chats.
  - `GET /api/chats/dots/wait?since=<mtime>` → blocks (0.3s poll loop) until
    `max(chat_state_mtime)` over the last 5 chats exceeds `since`; returns
    `{since, changed, dots}`.
- **Client** in `app.js`: a `watchDots()` loop (started on every page, since the
  sidebar is global) long-polling `/api/chats/dots/wait`, updating each
  `[data-chat-id]` dot's classes in place. Reuse the `sleep`/fetch/`since`
  structure of the existing `watchChat` loop.

---

## Files touched (summary)

- **New:** `render.py`, `questions.py` (extracted, trimmed).
- **Heavily edited:** `chats.py` (Phase A answer render, Phase D recursive tree,
  Phase E status helper), `routes_chats.py` (home route + dots endpoints),
  `app.js` + `app.css` (dots, borders, watchDots), `ui.py` (sidebar), `paths.py`,
  `worker.py`, `state.py`, `app.py`, `transcript.py` (`is_error`),
  `sub_agents/planner.py`, `routes_sub_agents.py` (import swaps).
- **Deleted:** `stages/`, `routes_stages.py`, `routes_tasks.py`, harness-only
  tests; on-disk `.pi/crack/tasks/` + `.pi/crack/harness/s0*.json`.

## Verification

All commands run **inside the container** (`docker exec crack-dev bash -exc '…'`,
`uv run python -m pytest tests/`):

1. **Unit/regression:** `uv run python -m pytest tests/` green after harness-test
   removal (chat, sub-agent, pi_proc, plan41 willRetry suites pass).
2. **Import/collect clean:** no lingering `stages`/`task_` references (grep + a
   `python -c "import crack_server.app"` smoke import).
3. **Bug A live:** open chat `1784588202230`; the repeated assistant text renders
   **once**; send a fresh multi-turn message and watch the live stream — no
   stacked `Clanker:` duplicates; dots turn green as tools complete.
4. **Harness gone:** `GET /` shows the chats-only home; `/tasks/*` and `/stages/*`
   are 404; `.pi/crack/tasks/` absent; queue + models cache intact; a chat + a
   spawned sub-agent still run end-to-end.
5. **Sub-agent borders:** spawn a recursive sub-agent; confirm nested bordered
   cards recolor blue→green (and orange when a run awaits user input) in real time.
6. **Dots real-time:** with a chat running, the sidebar + home dots update within
   ~1s via the `/api/chats/dots/wait` long-poll (running pulses blue; inner dot
   tracks last tool color; ✕ on error).

## Notes / risks
- The `stages/render.py → render.py` extraction is the riskiest step (many
  importers); do it as a pure move + import-rewrite with tests green *before*
  deleting the stage pipeline files.
- Deleting on-disk data is destructive but authorized and git-tracked (the
  `.pi/crack/tasks/` and stage-config removals will show as deletions in
  `git status`).
