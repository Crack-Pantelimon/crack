# Prewalk refactor plan

Reimplement crack's sub-agent system around a single generic **coder** agent
that runs the [prewalk](https://stencil.so/blog/prewalk) architecture: a
frontier "planner" model explores, writes a todo list, and lands the first
edit; the moment that edit lands we **swap to a cheaper "implementer" model and
prune the hidden planning instruction from context**, so the cheap model
inherits a live trajectory (not a postcard) and never knows it was ever
planning. The same agent can also run in a plain non-plan mode on a single
model.

This document is the implementation plan. Round-1 design questions were
answered directly (see **Locked decisions**); the remaining rounds are folded
into **Open decisions** with a recommended answer each, so nothing is
hand-waved.

---

## 1. Locked decisions (from round 1)

1. **One unified core engine.** `chats.py` (top-level unscripted chat) and
   `sub_agents/base.py` (sub-agent runs) stop carrying two copies of the
   hop-loop / nudge / swap logic. A new shared `prewalk.py` core owns the
   state machine; both the chat worker and the sub-agent worker become thin
   adapters over it. A top-level chat is, semantically, a depth-0 coder run.
2. **Keep the generic persona loader, one directory.** `registry.py`'s
   directory-scanning discovery stays; the pi extension keeps looping over
   `.pi/crack/sub_agents/*/` to register `spawn_<slug>` tools. We simply delete
   `explorer/`, `tester/`, and `planner/`, leaving only `coder/`. Re-adding a
   second persona later stays a drop-in.
3. **Server-visible, plain-text todos.** The new `todo` tool always echoes the
   full current list as **text** in its result content (not only pi's
   structured `details`, which crack's event parser never captures). crack
   parses the latest `todo` tool_block output to (a) gate the model swap on
   "a todo list exists" and (b) build specific nudges that name the still-open
   items.
4. **Swap on the first `edit`/`write` after the todo exists.** Only the actual
   file-mutating tools (`edit`, `write`) trigger the frontier→cheap swap, and
   only once a todo list has been initialised. Prompts explicitly forbid
   editing files via `bash` (`cat >`, `sed -i`, heredocs, etc.); `bash` is for
   exploration, tests, verification, and renames only. `bash` calls never count
   as "the first edit."

---

## 2. Current architecture (what we're changing)

- **Personas** live under `.pi/crack/sub_agents/<slug>/` as
  `agent.py` (a `SubAgentPersona` subclass + `PERSONA = …`), `config.json`
  (`model`, `tool_description`, `tool_label`), and prompt templates
  (`system.md`, `nudge.md`, …). `registry.py` lazily discovers them.
- **Sub-agent run loop** — `sub_agents/base.py`:
  `run_start → _begin_run → _run_hop → _after_hop`. `_after_hop` decides
  nudge / continue / done / error by inspecting the last turn's `tool_blocks`
  and whether the report file exists. Multi-hop: it re-enqueues `run`
  (RESUME_MESSAGE) up to `MAX_HOPS = 5`, with `MAX_NUDGES = 3` empty-turn
  nudges. Fixed `DEFAULT_MODEL`; per-persona override via `config.json`.
- **Planner persona** — `sub_agents/planner.py` is a bespoke
  grill→Q&A→write state machine that writes a plan **document** to a path.
  This is exactly the `/plan`-style handoff the article argues against; it goes
  away.
- **Chat loop** — `chats.run_chat` pops pending exchanges and calls the shared
  `chat_engine.run_exchange`, which runs **one** `pi_runner.arun_agent_hop`
  per exchange (no swap, no todo, single `model` from `info["model"]`, changed
  anytime via a save-on-change dropdown). `chat_engine.run_exchange` is also
  used by the Finished stage's review chat.
- **The hop runner** — `pi_proc.arun_agent_hop` runs one `pi --mode json -p
  --model <M> --session-id X --session-dir …` subprocess, tails its JSON event
  stream to a durable `hop.jsonl`, and resumes the **same session** across hops
  via `--session-id`. It already accepts a fresh `model=` every call, a
  `sentinel` (stops the hop when assistant text matches a line), a
  `stop_check`, and a `waiting_check`. **It has no `--append-system-prompt`
  plumbing** — the gap prewalk's context-pruning needs.
- **The pi extension** — `.pi/extensions/crack/index.ts` registers
  `wait_join`, `ask_user`, `analyze_image`, and one `spawn_<slug>` per persona
  dir. Tools talk to crack-server over HTTP on `:9847`. **No `todo` tool
  today.** pi ships a reference `todo` extension (state reconstructed from the
  session tree via tool-result `details`) and a `plan-mode` extension (bash
  allowlist) we can borrow patterns from.
- **Transcript parsing** — `transcript.apply_event_to_turn` captures each tool
  call's `name`, `input`, and text `output` into `turn["tool_blocks"]`. It does
  **not** capture pi's structured `details` — hence decision 3's plain-text
  requirement.
- **Models & settings** — `models.py` caches `pi --list-models`;
  `routes_settings.py` currently exposes only a global **vision model**.
  `DEFAULT_CHAT_MODEL` is a constant.

---

## 3. Target architecture

### 3.1 The prewalk core (`crack_server/prewalk.py`, new)

A single module implementing the run state machine, consumed by both the chat
worker and the sub-agent worker. Responsibilities:

- Own the **phase** progression for a run: `planning → implementing → done`
  (plus `error` / `stopped`), or, in non-plan mode, straight `implementing`.
- Compile each hop's message and system-prompt-append from templates + run
  state.
- Call `pi_proc.arun_agent_hop` with the **current phase's model** and, while
  planning, the **hidden planning instruction as `--append-system-prompt`**.
- Detect the swap trigger in-stream, terminate the frontier pi cleanly, flip
  phase to `implementing`, and resume the same session on the cheap model
  **without** the append (the prune).
- Drive the todo-aware nudge loop during `implementing`.
- Decide terminal conditions (report written / budget exhausted / stopped).

Chat and sub-agent adapters supply: state file, session dir/id, toolset,
media dir, env, and "how to finish" (chat idles; sub-agent inboxes its report
to the parent). This preserves each caller's I/O while single-sourcing the
algorithm. (Full collapse of the two **state files** into one shape is a
non-goal for this pass — too invasive; the *algorithm* is unified, which is
what decision 1 buys us.)

### 3.2 The model swap mechanism (the heart of prewalk)

**Granularity.** A crack "hop" runs until the model stops on its own, so within
hop 1 the frontier model could do the whole task. We must stop it right after
the first qualifying edit. We do this **in-stream**, reusing the existing
kill+resume infrastructure:

1. During `planning`, the stream tailer (`pi_proc._process_stream_line`)
   watches JSON events. When it observes a `todo` toolCall it sets
   `todo_seen`. Once `todo_seen`, the **first `edit` or `write` toolCall**
   (let its tool result land) is the swap point: the tailer calls
   `terminate()` and the hop returns a new stop reason **`"swap"`**.
2. `prewalk.py` sees `reason == "swap"`, flips run phase to `implementing`,
   records `swapped_at`, and **resumes the same `--session-id`** with the
   implementer model — no `--append-system-prompt` this time.
3. The cheap model continues a session that already contains: exploration, a
   todo list mid-checkmark, and one landed edit — a free in-context example.

This reuses `kill_pid_file` / `_terminate_group` and the durable-session resume
that already survives reloads. A clean terminate (like `stop`) must be
distinguishable from a crash so the retry loop doesn't fight it — hence a
dedicated `"swap"` reason, not a kill masquerading as failure.

**Context pruning (why `--append-system-prompt`).** The hidden planning
instruction must vanish from the cheap model's context. If we deliver it as a
user turn, it persists in the session and resume can't remove it. Delivered as
a **system-prompt append**, it is a launch parameter, not a replayed message —
so simply omitting the flag on the implementer resume prunes it, *provided pi
rebuilds the system prompt from flags on `--session-id` resume rather than
replaying a stored one.* **This is the single most important thing to verify
before building** (see §8, V1). Fallback if pi persists the system prompt:
deliver the hidden instruction as the hop-1 user message and physically strip
that one message from the session `jsonl` at swap time (session surgery —
hackier but deterministic). Plan targets the append path; keeps surgery as the
documented fallback.

**Non-plan mode.** `plan=false` skips all of the above: phase starts at
`implementing`, single `model`, no append, no swap watch. Todo tool + nudges
still available (the model may use todos), but nothing is forced.

### 3.3 The `todo` tool

Add a `todo` tool to `.pi/extensions/crack/index.ts`, registered always (like
`wait_join`). Modeled on pi's reference `todo.ts`:

- State reconstructed from the session tree (branch-safe), stored in the tool
  result `details` **and** mirrored as plain text in `content` (decision 3).
- Actions: `write` (replace whole list — matches the article's "init a TODO
  list with a validation step for each item"), `toggle`/`check` (mark an item
  done), optionally `list`. Keep it minimal.
- Item count is capped in the **prompt** (the article warns GPT-class guides
  otherwise write 60-item lists and batch-complete them). Recommend cap ~12.
- Text output shape crack parses (stable, greppable):
  ```
  [x] #1 read auth signing path
  [ ] #2 add rate limiter to base.py
  [ ] #3 add regression test
  ```

crack reads the **latest** `todo` tool_block output from the run's turns to
know: does a list exist (swap gate)? which items are still open (nudge text)?
No separate server-side todo storage — it lives in the session, which is what
"take the todo out of the edit list" means.

### 3.4 Todo-aware nudging during implementation

Replaces the generic `MAX_NUDGES` empty-turn nudge for the `implementing`
phase. When the implementer stops with the report unwritten and open todos
remain, crack resumes with a nudge naming them:

> You still have open todo items: **#2 add rate limiter, #3 add regression
> test**. Continue — mark each done via the `todo` tool as you complete it, and
> write your report when the list is clear.

This is the article's "todo reminder that bugs it endlessly … free steering."
Keep a hop budget so a stuck run still terminates (see Open decision O3).

### 3.5 Prompts (all customizable)

Per decision "each aspect customizable," everything lives in editable files
under `coder/` (already served by the sub-agents template editor UI,
`routes_sub_agents.api_put_persona_template`):

- `system.md` — base task framing (both phases). Includes the **tool-hygiene
  rules**: use `edit` to change existing files, `write` to create new files,
  **never** edit files through `bash`; `bash` is for exploration / tests /
  verification / renames only.
- `plan_instruction.md` — the **hidden planning instruction**, injected via
  `--append-system-prompt` during `planning` only: "Plan deeply first. Explore
  the code, then capture the plan as a todo list (≤N items, each with a
  validation step) via the `todo` tool. Only once the plan is captured, begin
  editing. Stop planning the moment you make your first edit." Never shown to
  the implementer.
- `nudge.md` — todo-aware implementation nudge (`{open_todos}` placeholder).
- `report.md` context / `report_instructions` — unchanged concept: the run
  still ends by writing a report the parent consumes.

### 3.6 Models, settings, and the new-chat form

**Three global model settings** (in `routes_settings.py`, stored beside the
vision model): `plan_planner_model`, `plan_implementer_model`,
`nonplan_model`. Defaults seed everything.

**New-chat creation becomes a form** (today it's an instant POST). The form
shows:
- a **plan checkbox, default on**;
- when checked: **planner** + **implementer** model dropdowns;
- when unchecked: a single **model** dropdown.

Dropdowns pre-fill from the three global settings. On submit, the chosen
`plan` flag + model(s) are written to `info.json` and **locked** — the
save-on-change model dropdown in the running chat form is removed (or rendered
read-only). `create_chat()` and `paths.create_chat()` gain these fields.

**Sub-agents (spawned by an over-agent)** have no picker UI, so they use the
**global settings** for their models, with `plan` supplied by the spawn tool
(next). `coder/config.json` may still override globally per-persona, but the
per-chat lock is the authoritative path for top-level runs.

### 3.7 The `spawn_coder` tool gains a `plan` parameter

The over-agent controls prewalk per child. In `index.ts`, `spawn_<slug>`'s
params gain `plan?: boolean` (default true), and the description explains it:
"plan=true starts on a smarter model in planning mode building a todo list,
then hands off to a cheaper implementer once the first edit lands; plan=false
runs the whole task on one model. Use plan=true for non-trivial changes,
plan=false for small/mechanical edits." The flag flows to
`/api/chats/{id}/sub_agents/spawn`, onto run state, into the prewalk core.

---

## 4. State model changes

**Run state (`runner.spawn`) and chat `info.json`** gain:
- `plan: bool`
- `planner_model: str`, `implementer_model: str` (plan mode)
- `model: str` (non-plan mode; chat already has this)
- `prewalk_phase: "planning" | "implementing"` (runtime; drives which model
  the next hop uses — survives reloads)
- `todo_seen: bool` and `swapped_at: float | None` (swap bookkeeping)

**No new todo storage** — todos live in the pi session, surfaced through
`turns[*].tool_blocks`.

---

## 5. Component-by-component change list

**New**
- `crack_server/prewalk.py` — unified core state machine (§3.1).
- `.pi/crack/sub_agents/coder/plan_instruction.md` — hidden planner append.
- `todo` tool in `.pi/extensions/crack/index.ts`.

**Modified**
- `pi_proc.py` / `pi_runner.py` — add `append_system_prompt` param to
  `arun_agent_hop` + `_build_cmd` (`--append-system-prompt`); add in-stream
  **swap detection** (`todo_seen` + first edit/write) producing reason
  `"swap"`; thread a `swap_watch`/phase flag through `_HopParams`.
- `sub_agents/base.py` — gut the bespoke loop; delegate to `prewalk.py`.
  Keep persona metadata, state helpers, orphan/stop/retry plumbing.
- `chats.py` / `chat_engine.py` — route chat exchanges through `prewalk.py`;
  remove the save-on-change model dropdown; read locked models/plan from
  `info.json`. (Keep the Finished-stage review chat on a simple single-hop path
  — it is not a coder run; leave it on the current `run_exchange` or a
  `plan=false` shim. See Open decision O4.)
- `sub_agents/coder/{config.json,system.md,nudge.md}` — add planner/implementer
  model keys, tool-hygiene rules, todo-aware nudge.
- `routes_settings.py` + settings page — three global model dropdowns.
- `routes_chats.py` + home/new-chat UI — creation **form** (checkbox + model
  dropdowns), locked thereafter.
- `paths.create_chat` — persist plan flag + models.
- `routes_sub_agents.py` — `/spawn` accepts + stores `plan`.
- `transcript.py` — (only if needed) ensure `todo` tool_block text output is
  retained verbatim for nudge parsing (it already keeps `output`; verify no
  truncation of the list — see V3).

**Deleted**
- `.pi/crack/sub_agents/explorer/`, `tester/`, `planner/`.
- `sub_agents/planner.py` and its templates; planner-specific branches in
  `routes_sub_agents.py` / `chats.py` (grill rounds, `submit_answers`,
  `continue_to_write`, `awaiting_answers` UI). Keep `ask_user` — that is
  general and orthogonal to the planner.

---

## 6. Open decisions (rounds 2–3, with recommendations)

These are the remaining design questions I'd normally have grilled you on;
each carries my recommended answer so the plan is actionable. Flag any you want
to overturn.

**Round 2 — mechanism**

- **O1. Swap detection point: in-stream kill vs. hop-boundary.**
  *Recommend in-stream* (§3.2): kill the frontier pi right after the first
  edit's result, resume cheap. Alternative (simpler, worse): let hop 1 run to
  completion on the frontier model, swap only at the hop boundary — but with no
  turn cap the frontier often finishes the whole task, defeating prewalk. Cost
  of in-stream is one new stop reason + a few lines in the tailer; worth it.

- **O2. Context prune: `--append-system-prompt` vs. session surgery.**
  *Recommend append*, contingent on V1 confirming pi rebuilds the system prompt
  from flags on resume. If V1 fails, fall back to hop-1-user-message +
  strip-on-swap. Do not ship without V1 settled.

- **O3. Implementation hop budget & stuck-run termination.**
  *Recommend*: keep a per-run hop cap (e.g. `MAX_HOPS` unchanged at 5 counted
  across both phases) plus a todo-nudge cap (e.g. 3 consecutive nudges with no
  todo progress → error "made no progress on open todos"). Prevents the cheap
  model looping forever on an unreachable item.

- **O4. Does the Finished-stage review chat also become prewalk?**
  *Recommend no* — it is a Q&A review surface, not a coder. Leave it on the
  existing single-hop `run_exchange` (equivalently `plan=false`, single model).
  Unifying it buys nothing and risks the one place chats are used read-mostly.

**Round 3 — product & edges**

- **O5. Retry after a swapped run errors — which model resumes?**
  *Recommend*: retry resumes in the **current** `prewalk_phase` on that phase's
  model (a run that failed post-swap retries on the implementer; the planning
  instruction is already pruned and stays pruned). Never re-run planning on
  retry — the todo list + edits already exist in-session.

- **O6. Todo tool shape: `write` whole-list vs. incremental add/toggle.**
  *Recommend* a `write` (replace) + `toggle` pair, capped at ~12 items. Replace
  matches "init a todo list" cleanly and avoids drift between the model's
  mental list and stored state; toggle gives the cheap model its steering
  ritual. Skip `add`/`clear` to keep the surface tiny.

- **O7. What if the frontier model writes an edit *before* any todo?**
  *Recommend*: do **not** swap (swap gate requires `todo_seen`). Instead, on
  the next hop boundary, nudge it to capture the todo list first. This enforces
  the article's "gating on any edit alone is no good" finding and keeps the
  free in-context example (todo + edit) intact for the cheap model.

- **O8. Migration of in-flight runs / existing chats on deploy.**
  *Recommend*: treat any chat/run lacking the new fields as `plan=false` on a
  single `model` (back-compat default), so old chats keep working without a
  data migration. New creations use the form. Delete the retired persona dirs
  only after confirming no active runs reference them (they won't after this
  ships, since the tools disappear).

---

## 7. Sequencing (safe, incrementally testable)

1. **Plumbing, no behavior change.** Add `append_system_prompt` to the hop
   runner + `_build_cmd`; add the `"swap"` reason scaffold (unused). Land V1
   verification here.
2. **Todo tool.** Ship the `todo` tool in the extension; confirm crack captures
   its text output in `tool_blocks` (V3).
3. **prewalk.py core** behind a flag, exercised by the **sub-agent** path first
   (smaller blast radius than chats): coder persona runs plan mode end-to-end,
   swap fires, prune verified.
4. **Settings + new-chat form + lock**; wire the chat worker through
   `prewalk.py`.
5. **spawn_coder `plan` param.**
6. **Delete** explorer/tester/planner + planner state machine + dead UI.
7. Update tests (`test_sub_agents.py` et al.) throughout.

---

## 8. Verification tasks (must-do, in-container)

All via `docker exec crack-dev bash -exc '…'`.

- **V1 (blocking): does pi prune a system-prompt append on `--session-id`
  resume?** Run a hop with `--append-system-prompt SENTINEL --session-id T`,
  then resume the same session **without** the flag, and inspect the session
  `jsonl` + a probe prompt ("what were your system instructions?") to confirm
  SENTINEL is absent from the resumed context. Decides O2.
- **V2: does an extension/tailer see `edit`/`write`/`todo` toolCall events in
  the `--mode json` stream in time to terminate cleanly after the tool result?**
  Confirms the in-stream swap point is reachable.
- **V3: is the `todo` tool's full text list preserved in `tool_blocks` output**
  (no truncation that would drop open items) for nudge parsing.
- **V4: model swap round-trip** — a scripted plan-mode coder run swaps exactly
  once, on the first edit after a todo, and the implementer model id shows up in
  subsequent hop manifests.
- **V5: `pi --list-models`** ids for the three global defaults resolve (per the
  memory note: always verify model ids before wiring them).

---

## 9. Risks & mitigations

- **Prune fails (V1 negative)** → session surgery fallback (O2). Highest-value
  thing to settle first.
- **Swap never fires** (model edits without a todo, or via bash) → todo gate +
  prompt hygiene + O7 nudge; swap-watchdog: if planning exceeds a turn budget
  with no swap, force a hop-boundary swap so a run can't be stuck frontier-only.
- **Two engines drift again** → single `prewalk.py`; chat/sub-agent are
  adapters, not copies.
- **Cost regression** if the frontier model overruns planning → the whole point
  is bounding frontier tokens; the in-stream swap + turn-budget watchdog cap it.
- **Back-compat** for old chats → O8 default-to-nonplan.
