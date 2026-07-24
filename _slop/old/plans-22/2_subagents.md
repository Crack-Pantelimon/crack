# Plan 2 — Sub-agent tree: live refresh, titles, ordering/metrics, parallel cap, tool gating

**Scope:** the sub-agent control tree (right sidebar + inline cards), per-run titles, run metrics,
a max-3-parallel spawn cap, and correct tool/prompt gating at max depth. Touches:
`.pi/crack/server/src/crack_server/`: `chats.py` (sidebar/card render), `titles.py`, `routes_sub_agents.py`,
`sub_agents/{constants.py, runner.py, base.py}`, and the pi extension `.pi/extensions/crack/index.ts`,
plus the coder persona at `.pi/crack/sub_agents/coder/`.

**Run state shape (in `run.json`, from `runner.spawn`):** `run_id, persona, chat_id, parent_kind,
parent_id, depth, instructions, report_path, plan, phase, hops_completed, children[], turns[],
created_at, finished_at, title?`. Terminal phases: `done|error|stopped`.

**Tests:** `python -m pytest -q` from the server dir. Relevant existing tests: `test_sub_agents.py`,
`test_wait_join.py` (spawn/wait mechanics against `fake_pi.sh`).

**Do these sub-tasks in order** — later ones assume the `title`/metrics fields exist.

---

## Task 2.1 — Sidebar tree auto-refreshes when new sub-agents appear

**Problem.** `render_sidebar_tree` (chats.py ~680) only attaches the `hx-trigger="every 2s"` poll when
`active = chat_running or any_run_active`. If the page was loaded while the chat was idle with no runs,
`active` is `False`, the `#subagent-sidebar-tree` fragment has **no poll attrs**, and it never updates when
the agent later spawns sub-agents — requiring a full page reload (the reported bug).

**Fix (keep the fragment live while the chat page could change).** Change the gate so the sidebar keeps
polling whenever the chat is not in a settled terminal state — specifically poll when the chat phase is
anything other than a hard-idle-with-no-active-runs *and no pending work*. The robust, simple version:
**always emit the poll attrs while the chat exists**, and let the fragment stop itself only once the chat
is idle AND every run is terminal AND there is no queued/pending chat work.

Concretely, compute:
```python
chat_pending = bool(state.get("pending") or state.get("child_inbox"))
active = chat_running or any_run_active or chat_pending
```
That alone still freezes if the very first spawn happens after an idle load. So also **make the chat's
main status poller repaint the sidebar**: the chat page already long-polls `/chats/{id}/wait` and swaps
the content region. Add an out-of-band refresh: when the sidebar is *not* self-polling, have the chat
status fragment (`chat_status` route, chats.py ~750 tail / `render_chat_tail`) include
`hx-get=".../sidebar-tree"` via `hx-swap-oob`? That's heavier. **Preferred minimal fix:** always poll the
sidebar every 2s (drop the `active` gate for *starting* the poll), but when fully settled emit a slower
`every 5s` (or keep 2s — the sidebar render is cheap: it does NOT call `render_context_line`, only small
`run.json` reads). Given Plan 3 moves heavy IO off the loop and these routes are sync (`def`, threadpool),
constant 2s sidebar polling is acceptable.

Chosen implementation: replace the `if active:` guard so `poll_attrs` is **always** set to
`every 2s`. Keep the `active` computation only to pick the root dot class. Add a code comment that the
perf budget for this is handled in Plan 3 (poll routes run in the threadpool; reads are cheap).

**Verify:** unit-render `render_sidebar_tree(chat_id)` for a chat with zero runs and assert the output
contains `hx-trigger="every 2s"` (previously absent). Add a run, re-render, assert the new run's node
appears. `python -m pytest -q`. Manual: open a fresh chat, send a message that spawns sub-agents, watch
the right tree populate without reloading.

---

## Task 2.2 — Per-run title from the summarization model (replace the "coder" label)

**Goal.** Instead of showing the persona slug `coder` as the node/card label, generate a short title from
the run's `instructions` using the existing nano title model, and show that. Mirror the chat-title flow
`_maybe_generate_title` (chats.py ~1038) which calls `titles.agenerate_title`.

**Where to generate.** The sub-agent run loop runs in the worker. `titles.generate_title` is the **sync**
twin (for thread callers) — perfect for the worker. Generate the title once, lazily, at run start:
- In `SubAgentPersona._begin_run` (base.py ~156), before/after setting phase, if `state.get("title")` is
  empty, compute `title = titles.generate_title(state["instructions"], log_prefix=f"subagent-title/{run_id}")`
  inside a `try/except` (best-effort; on failure leave title empty and fall back to persona name).
  Persist it via `state_update`. Do this **once** (guard on `not state.get("title")`).
  - Because this is a blocking pi text call, wrap it so it doesn't stall the event loop: `_begin_run` is
    `async`, so run it via `await asyncio.to_thread(titles.generate_title, ...)` (import asyncio). This
    also aligns with Plan 3's async hygiene.
- Alternative (if you prefer generation off the hot path): generate in `runner.spawn` right after writing
  state. But `spawn` is sync and called from the async route; use the async route to `await asyncio.to_thread`.
  **Pick the `_begin_run` approach** — it keeps `spawn` fast and the title lands before the first hop
  renders.

**Display.** Add a helper `_run_label(state) -> str` in chats.py: `state.get("title") or state.get("persona") or "?"`.
Use it in:
- `_render_sidebar_node` (chats.py ~647): replace `esc(persona)` in the `tree-label` with `esc(_run_label(state))`.
  Keep the persona slug available as a `title="coder"` tooltip on the label.
- `_render_run_card` (chats.py ~552): replace the `<strong>{esc(persona)}</strong>` with the label; keep
  `depth · phase` line as-is.
- `run_page` (routes_sub_agents.py ~447): use the title in the `<h1>`/`<p>` if present.

**Verify:** with the `fake_pi.sh` shim, `titles.generate_title` returns the shim's canned text — assert a
spawned run gets a non-empty `title` in its `run.json` after `_begin_run`, and that `_render_sidebar_node`
shows it instead of `coder`. `python -m pytest -q` (extend `test_sub_agents.py`).

---

## Task 2.3 — Number + order runs by spawn time; show turns + alive time

**Decision (locked):** *turns* = LLM hops (`state["hops_completed"]`); *alive* = `created_at → finished_at`
(or `→ now` while running).

**Ordering.** Currently `_children_map` sorts `reverse=True` (newest first) and `_root_run_ids` too.
Requirement: number them by spawn order (oldest = #1). `run_id` is `"<ms_epoch>_<uuid>"` so lexical sort
== spawn-time sort. Change the sidebar ordering to **ascending** (oldest first) and assign 1-based indices.
- In `render_sidebar_tree`, sort `roots` ascending (or add a dedicated `_root_run_ids_ordered`). Pass an
  incrementing counter into `_render_sidebar_node`. Simplest: build an ordered list of all runs (roots +
  their children DFS) and a `{run_id: index}` map, thread it into the node renderer for the `#N` prefix.
  Since MAX_DEPTH=1 there is effectively one level, but keep the DFS general.

**Metrics helper.** Add to chats.py:
```python
def _run_turns(state: dict) -> int:
    return int(state.get("hops_completed", 0) or 0)

def _run_alive_str(state: dict) -> str:
    start = state.get("created_at")
    if not start: return ""
    end = state.get("finished_at") if state.get("phase") in _RUN_TERMINAL else time.time()
    mins = max(0, int((float(end) - float(start)) // 60))
    running = state.get("phase") not in _RUN_TERMINAL
    verb = "running for" if running else "ran for"
    return f"{verb} {mins} min"
```
(`import time` already present in chats.py? confirm — add if missing.)

**Display** in `_render_sidebar_node` `tree-meta` line (and optionally the card header):
`#{idx} · {turns} turns · {alive_str}`, e.g. `#2 · 4 turns · running for 6 min`. Keep the model badge.
Because the sidebar self-polls every 2s (Task 2.1), the "running for N min" ticks up live.

**Verify:** unit — construct two run states with known `created_at`/`finished_at`/`hops_completed`,
render the sidebar, assert `#1` appears before `#2` (spawn order), assert `ran for` for the terminal one
and `running for` for the active one, assert turn counts shown. `python -m pytest -q`.

---

## Task 2.4 — Max 3 sub-agents in parallel (const by MAX_DEPTH); block spawn until a slot frees

**Decision (locked):** when the agent calls `spawn_coder` while 3 of its children are already running, the
spawn **blocks** (long-poll, like `wait_join`) until a sibling finishes, then mints the run. One special
trajectory line notes the implicit wait; then the normal spawn line follows.

**Constant.** In `sub_agents/constants.py`, next to `MAX_DEPTH = 1`, add:
```python
MAX_PARALLEL_SUBAGENTS = 3   # max concurrently-running children per parent (main agent)
```
Export it from `sub_agents/__init__.py`.

**Counting active children.** A parent is either the chat (root, depth 0) or a run. With MAX_DEPTH=1 only
the chat spawns, but implement generally. Active = children whose `run.json` phase ∉ terminal. Add a
helper in `runner.py`:
```python
def _active_child_count(chat_id, parent_kind, parent_id) -> int:
    # roots for a chat parent; children[] for a run parent
    ...count runs with phase not in ("done","error","stopped")...
```
For a **chat** parent, active children = `_root_run_ids(chat_id)` filtered to non-terminal. To avoid
importing chats.py from runner (cycle), re-derive from `paths.list_run_ids` + `run.json` reads: a run is a
root-of-chat when `parent_kind == "chat" and parent_id == chat_id`.

**Blocking mechanism.** Mirror the `wait_join` server pattern (routes_sub_agents.py `api_wait_sub_agents`
+ `signals.event_for` + `runner.finish` → `signals.notify_parent`). Implementation:
1. In the spawn **route** `api_spawn_sub_agent` (routes_sub_agents.py ~78), before calling `runner.spawn`,
   if `_active_child_count(...) >= MAX_PARALLEL_SUBAGENTS`, long-poll:
   - Register/clear on the parent's `signals.event_for(parent_kind, parent_id)` (the same event
     `runner.finish` fires via `notify_parent`), wait up to `MAX_BLOCK_SECONDS` (reuse the 25s const),
     re-checking the count after each wake. This is bounded so the HTTP request doesn't hang forever.
   - **Extension timeout note:** the extension's spawn `fetch` currently uses a hard
     `AbortSignal.timeout(15000)` (index.ts ~455). A 25s server block would be aborted. So set the server
     spawn-block budget to **~10s** and return a `{"status": "slot_pending"}` JSON when still full; the
     extension must loop and re-POST until it gets a `run_id` (see extension change below). Emit the
     "waiting" note only on the **first** `slot_pending`.
2. Set `waited_for_slot: True` on the created run state (or return it in the spawn response) so the
   trajectory can show the note. Simplest UI path (no synthetic turns): when the spawn finally succeeds
   after having waited, prefix the tool's returned `text` with a line like
   `⏳ 3 sub-agents were already running — waited for a free slot.\n` and let Plan-1's `spawn_` render show
   it as part of the output. The extension builds the returned text, so pass a flag back in the spawn JSON
   (`"waited": true`) and have the extension prepend the note.

**Extension changes (`.pi/extensions/crack/index.ts` spawn `execute`, ~447):**
- Loop: POST spawn; if response JSON `status == "slot_pending"`, wait ~1s and retry (respecting `signal`),
  and on the first `slot_pending` remember to prepend the waiting note to the final text. Cap total wait
  with the tool `signal` (the model can cancel). Increase the per-request `AbortSignal.timeout` to ~12s so
  each server block (≤10s) completes.
- When the final success JSON carries `waited: true` (or you tracked a `slot_pending` earlier), prepend
  `⏳ waited for a free slot (max 3 parallel).` to the returned text.

**Server response contract:**
- `api_spawn_sub_agent` returns `{"status":"slot_pending"}` (HTTP 200) when the cap is hit after the block
  budget, else the existing `{run_id, report_path, status, "waited": <bool>}`.

**Verify:** extend `test_sub_agents.py`/`test_wait_join.py`: spawn 3 runs that stay "running" (fake_pi
never writes a report within the window), assert a 4th spawn returns `slot_pending`; then flip one child
to a terminal phase + `signals.notify_parent`, assert the retried spawn now mints a run with
`waited == True`. `python -m pytest -q`.

---

## Task 2.5 — At max depth: don't register spawn/wait tools; move prompt sections to `sub_agent_instructions.md`

**Two confirmed problems:**
1. `.pi/extensions/crack/index.ts` hardcodes `const MAX_DEPTH = 3` (line 23) but the server enforces
   `MAX_DEPTH = 1`. So a depth-1 sub-agent *sees* `spawn_coder`/`wait_join`, calls them, and the server
   rejects "depth 2 exceeds maximum 1" — wasted turns (visible in the chat).
2. The coder `system.md` always contains "You may spawn helper coder sub-agents (`spawn_coder`)…" and the
   "Coordinating sub-agents and the human" section, even when the agent can't spawn. These must only
   appear when spawning is actually available.

**Fix A — extension gates tool registration by depth.**
- Change `MAX_DEPTH` in index.ts to **1** to match the server (single source of truth is the server
  constant; hardcoding 1 in the extension is acceptable but add a comment pointing at
  `sub_agents/constants.py`. Optional: fetch it once from a new `GET /api/sub_agents/limits` returning
  `{max_depth, max_parallel}` — nicer, but a matching literal `1` is fine for this plan).
- Read `depth = Number.parseInt(process.env.CRACK_SUBAGENT_DEPTH ?? "0")` **at registration time** (env is
  set by `_subagent_env` in base.py). When `depth >= MAX_DEPTH`, **do not** `pi.registerTool` for
  `spawn_${slug}` and **do not** register `wait_join`. (Leave `todo`, `ask_user`, `analyze_image` always
  on.) This removes the tools entirely for leaf agents instead of failing them at call time. Update the
  header comment (index.ts ~1-10) which currently says the tools are always visible and depth is checked
  in execute().

**Fix B — split the coder system prompt.**
- Create `.pi/crack/sub_agents/coder/sub_agent_instructions.md` containing the two spawn/coordination
  sections currently in `system.md`:
  - the "You may spawn helper coder sub-agents (`spawn_coder`)…" bullet under "How to work", and
  - the whole "## Coordinating sub-agents and the human" section (spawn_coder→wait_join, ask_user).
  (Keep `ask_user` guidance only if leaf agents still have `ask_user` — they do. So **move only the
  spawn/wait_join lines** into the new file; keep the `ask_user` paragraph in `system.md`. Re-word so the
  new file is self-contained.)
- In `system.md`, replace the removed spawn/coordination lines with a single placeholder token on its own
  line: `{sub_agent_instructions}`.
- Add `sub_agent_instructions.md` to the coder persona `templates` list in
  `.pi/crack/sub_agents/coder/agent.py` (so it's editable via the personas page and copied by tests'
  `_seed_personas`).
- In `SubAgentPersona._fill_template` (base.py ~189): compute whether this run can spawn
  (`can_spawn = int(state.get("depth", 0)) < MAX_DEPTH`), load `sub_agent_instructions.md` and substitute
  it for `{sub_agent_instructions}` **only when `can_spawn`**; otherwise substitute the empty string.
  ```python
  from crack_server.sub_agents.constants import MAX_DEPTH
  sub_instr = ""
  if int(state.get("depth", 0)) < MAX_DEPTH:
      try: sub_instr = self.load_template("sub_agent_instructions.md")
      except RuntimeError: sub_instr = ""
  text = text.replace("{sub_agent_instructions}", sub_instr)
  ```
  Also strip a leftover blank line if the token was on its own line and sub_instr is empty (cosmetic).

**Verify:**
- Python: render `_fill_template("system.md", state)` for `depth=0` → contains "spawn_coder"; for
  `depth=1` (== MAX_DEPTH) → does **not** contain "spawn_coder"/"wait_join" and the `{sub_agent_instructions}`
  token is gone. `python -m pytest -q` (add to `test_sub_agents.py`).
- Extension: `cd .pi/extensions/crack && grep -q "MAX_DEPTH = 1" index.ts`; and confirm the registration
  branch is guarded by `depth < MAX_DEPTH`. If a TS test harness exists run it; otherwise a manual node
  type-check (`npx tsc --noEmit` if configured) — otherwise a grep assertion is the automatic gate.

---

## Plan 2 completion checklist (automatic)
From `.pi/crack/server`:
```
python -m pytest -q
grep -q "MAX_PARALLEL_SUBAGENTS" src/crack_server/sub_agents/constants.py
grep -q "sub_agent_instructions" src/crack_server/sub_agents/base.py
test -f ../sub_agents/coder/sub_agent_instructions.md
grep -q "{sub_agent_instructions}" ../sub_agents/coder/system.md
grep -q "MAX_DEPTH = 1" ../../extensions/crack/index.ts
```
Then manual: spawn a task that fans out >3 sub-agents; confirm (a) the right tree numbers them #1..#N
oldest-first with live "running for N min" + turn counts and real titles, (b) the 4th spawn shows the
"waited for a free slot" note, (c) leaf (depth-1) agents no longer attempt `spawn_coder`.


Use all uv, python, bash, etc. commands only under the following way: docker exec crack-dev bash -exc 'command .... ' . Do not run any code outside the container, you will not have the tools available at all. You can use rg, fzf, grep, inside the container only. You can use edit and read commands as normal. The container mounts the workspace dir at /workspace where your shells will spawn. 