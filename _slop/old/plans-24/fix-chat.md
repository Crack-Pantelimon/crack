# Plan 24 — Fix regressed chat/pi behaviour after :O sandboxing

Status: ready to implement. Author investigation grounded against the live
`crack-dev` container (chats `1784731859181`, `1784732925686`) and the source at
`c013267`.

## Decisions locked (from the grilling)

1. **Base isolation = frozen git-tree snapshot per sandbox**, materialised from
   `git write-tree` (git-native, *no* `cp`, gitignored files naturally
   excluded). Concurrent top-level chats and hand-edits during a chat can no
   longer mutate each other's overlay lower or race host applies.
2. **Hard clean-git gate on the first message.** Refuse to send with a red error
   *until the host tree is clean*. The error shows the **top 10 lines of `git
   status`** rendered in a `<pre>` with ANSI terminal colours preserved, so the
   user recognises their own changes and deals with them in their terminal.
3. **Trajectory = full switch to a faithful projection of pi's session
   `ndjson`.** No server-rebuilt turn store in the render path. **Nothing is
   filtered away** — every event is shown; unknown event types render as a
   type-labelled row with an **Expand** button revealing the raw JSON.
4. **Continue-if-tools policy.** After crash-detection is fixed, a hop that ends
   with a *real* `agent_end` whose **last turn contained tool calls** auto-resumes
   the next hop instead of going idle.

---

## Issue 1 — Chats hang after every tool call (highest priority)

### Root cause (confirmed)

Chat `1784731859181`: every exchange is one hop that ends `reason=agent_end`
right after a single tool call, yet:

- the pi trajectory (`agent.hop.jsonl`) contains **no `agent_end` event at all**;
- the hop manifest records `"status": "crashed"`;
- the stream ends mid-`message_update` — the sandbox pi died mid-turn.

Three code facts turn a crash into a fake clean finish:

- `sink.reason` defaults to `"agent_end"` — [pi_proc.py:655](../../.pi/crack/server/src/crack_server/pi_proc.py#L655).
- In sandbox mode `proc is None`, so `returncode` is **always `None`**; the
  `failed` guard requires `returncode not in (0, None)` and therefore can *never*
  be true for a sandbox hop — [pi_proc.py:1226-1230](../../.pi/crack/server/src/crack_server/pi_proc.py#L1226).
- `_attempt_once` already computes `crashed` (`not terminated_by_us and not
  terminal`) for the manifest at [pi_proc.py:1048-1063](../../.pi/crack/server/src/crack_server/pi_proc.py#L1048)
  but **drops it from the return dict** ([pi_proc.py:1078-1086](../../.pi/crack/server/src/crack_server/pi_proc.py#L1078)).

Net effect: sandbox pi crashes → `terminal=False`, `reason="agent_end"`,
`persisted>0` → `_run_hop_with_retries` returns `"agent_end"`
([pi_proc.py:1239-1240](../../.pi/crack/server/src/crack_server/pi_proc.py#L1239))
→ `_run_prewalk_loop` sees `agent_end`, `open_todos` is empty (a non-plan
nemotron chat keeps no todo list) → **breaks → chat goes idle.** The user hits
"continue", the next hop crashes again after a tool call, ad infinitum.

### Fix 1a — treat a crash as a retryable failure

- `_attempt_once` returns `crashed` in its result dict (the value it already
  computed for the manifest). Also stop defaulting a non-terminal end to
  `agent_end`: when the stream ended without a terminal event, set
  `sink.reason = "crashed"`.
- `_run_hop_with_retries`: `failed = res["crashed"] or (… returncode not in (0,
  None))`. A crashed sandbox hop now follows the retry path (hard-backoff,
  session-resume with `RESUME_MESSAGE`, error row recorded, budget/streak caps
  intact). Because turns persisted before the crash count as progress, the retry
  *resumes* the session rather than replaying the prompt.
- Guard the `elif res["persisted"] > 0 or res["reason"] != "agent_end"` clean-exit
  branch so `crashed` never slips through it.

### Fix 1b — stop corrupting the hop output file (why it crashes)

The reused `agent.hop.jsonl` is physically corrupt: **null-byte runs** and a
**truncated partial JSON line** (`":2,"hash":"d05"…` with the head sheared off).
That is the signature of two writers on one path — a detached prior-hop/attempt
pi still holding the fd at a high offset while a new attempt `write_bytes(b"")`
truncates it ([pi_proc.py:953](../../.pi/crack/server/src/crack_server/pi_proc.py#L953)),
producing a sparse file zero-filled up to the stale offset. This also poisons the
`stderr_tail`/`output_tail` (the null-byte "detail" rows we saw), which is why
the real provider error never surfaces (Issue 1c).

- **Per-attempt unique output files.** Give each attempt its own hop output path
  (e.g. `agent.hop.<hop>.<attempt>.jsonl`, or an `attempt` suffix) so a lingering
  detached writer can never share a path with a live attempt. The manifest keeps
  pointing at the current attempt's file; re-attach/offset logic reads the path
  from the manifest, so it keeps working. Old files are swept on teardown.
- Before truncating/reusing any path, hard-assert the prior detached
  session/pid for it is dead (extend `_sweep_detached_pids` to *block* reuse
  until reaped, or just avoid reuse via the unique-path rule above — preferred).
- Harden the reader: when `_process_stream_line` sees a line that is all NUL or
  decodes to mostly NUL, drop it rather than filing it as stderr detail.

### Fix 1c — surface pi's real error text

Today every failure is the harness's generic `"pi returned only empty turns"`;
the provider 500/`finalError` is lost.

- With 1b fixed, the redirected stdout/stderr tail is clean again, so genuine
  provider errors captured on the stream are readable in `detail`.
- Additionally extract pi's **structured** error signals into the error row: the
  stream's `auto_retry_end.finalError` (already partially handled via
  `ended_in_error` — [pi_proc.py:786-787](../../.pi/crack/server/src/crack_server/pi_proc.py#L786))
  and any `type:"error"`/error-role events in the session ndjson. Prefer the
  structured message over the generic string when present, so the error card in
  the UI shows *what pi actually said* (bad gateway / 500 / rate-limit body).
- Keep `detail` NUL-scrubbed (defensive, pairs with 1b).

### Fix 1d — continue-if-tools policy (locked decision)

In `_run_prewalk_loop` ([chat_engine.py:220-227](../../.pi/crack/server/src/crack_server/chat_engine.py#L220)),
after a natural end, resume the next hop when **either** there are open todos
**or** the last persisted turn of this exchange has non-empty `tool_blocks`.
Reuse the existing nudge/`RESUME_MESSAGE` machinery and the `MAX_CHAT_NUDGES` /
`max_hops` bounds so a chatty tool-caller can't loop unbounded. A text-only
`agent_end` (no tools in the last turn) still settles to idle.

---

## Issue 2 — Trajectory desync: render from pi's session ndjson

### Root cause

The UI renders `chat.json exchanges[].turns`, which the server rebuilds
incrementally by tailing the stream and persisting at turn boundaries
(`TurnPersister`). A crash/reboot loses the in-flight turn and anything past
`persist_offset`; the reused `agent.hop.jsonl` is not a durable archive. pi's
durable, complete trajectory lives in `sessions/*.jsonl` (schema:
`{type:"message", message:{role,content,…}}` + `{type:"custom", …}` +
`session`/`model_change`/`thinking_level_change`), one file per session-resume,
parentId-linked.

### Fix (full switch, faithful — locked decision)

- New module `trajectory_view.py`: read the chat's `sessions/*.jsonl` in
  chronological order (filename timestamp), parse every line, and project to an
  ordered list of view rows:
  - `message` → role-aware rows (assistant text, thinking, toolCall,
    toolResult), merged by `toolCallId` like `apply_event_to_turn` does today.
  - `session` / `model_change` / `thinking_level_change` → thin annotation rows
    (model badge, thinking-level change) — these *replace* the current
    `persister.current_model` stamping for display.
  - **Any unrecognised `type`** → a generic row: show the `type` label + an
    **Expand** control (`<details>`) that reveals the pretty-printed raw JSON.
    Nothing is dropped.
- `render.py` renders from these projected rows instead of `turns`. Keep the
  append-only, per-row fragment identity so htmx `beforeend` polling stays
  stable (key rows by pi event `id`, which is stable and unique).
- Retire `exchanges[].turns` from the **render** path. Keep a minimal persisted
  record only for what the projection cannot derive: which user prompt started
  each exchange, recorded errors, ask_user Q&A, and sub-agent spawn linkage
  (the inline run-card anchor still keys off `spawn` tool output text). Map pi
  sessions → exchanges via the session boundary / resume message.
- Media thumbnails (`attach_media_to_blocks`) move into the projection step
  (read/analyze_image tool rows), so image chips keep working.
- Model-switch "handover" display is now *derived from real `model_change`
  events*, not from adjacent-turn model guesses — this also removes the bogus
  handover in Issue 4.

### Risks / notes

- Sub-agent inline cards and ask_user cards are harness constructs, not pi
  events; keep them as sidecar overlays merged into the projected stream by
  timestamp.
- Performance: parse is O(session size) per render; cache by `(file, size,
  mtime)` so the 2 s poll doesn't re-parse unchanged sessions.

---

## Issue 3 — Patch snags from a dirty / shared base

### Root cause

The sandbox overlay **lower is the live host tree**
(`CRACK_HOST_REPO_ROOT=/home/p/VIDOEGAME/crack`,
[sandbox.py:145-157](../../.pi/crack/server/src/crack_server/sandbox.py#L145)).
The finalize diff is `git apply`'d onto that same live tree
([patch.py:637](../../.pi/crack/server/src/crack_server/patch.py#L637),
`_apply_git` → `--3way`/`--reject`). Chat `1784731859181` created
`_slop/prompt-fix-chat.md` inside its sandbox; the host already had that
untracked file (dirty), so the apply failed `already exists in working
directory` and the failure was fed back to the chat as a user message
(`enqueue_chat_apply_failure`), which the model then thrashed on.

### Fix 3a — hard clean-git gate on first message

- New helper in `git_utils.py`: `host_worktree_dirty()` → runs `git status
  --porcelain` (including untracked) on `CRACK_HOST_REPO_ROOT`; and
  `host_status_colored(limit=10)` → `git -c color.status=always status | head`.
- In `post_message` ([chats.py:934](../../.pi/crack/server/src/crack_server/chats.py#L934)),
  when `pre_first` and sandboxing is enabled and the host tree is dirty: **do
  not enqueue**. Return the chat fragment with a red error block containing the
  first 10 lines of colourised `git status` inside a `<pre>` (ANSI → HTML spans;
  a tiny SGR→span converter, or `aha`-style mapping — keep it dependency-free).
- Front-end: the send button on a brand-new chat should surface this inline
  (htmx swap of the composer region) and stay red until the user retries on a
  clean tree. No auto-stash, no bypass.
- Applies to top-level chats. Sub-agents fork from the parent's frozen tree, not
  the host, so they are unaffected.

### Fix 3b — frozen snapshot base (also fixes Issue 5)

See Issue 5 — the frozen base makes the finalize apply land on a *known* tree,
and combined with 3a the host is clean at fork time, so the common conflict class
disappears. Remaining genuine conflicts (host advanced during a long chat) still
route to the agent via `enqueue_chat_apply_failure`, unchanged.

---

## Issue 4 — Model/plan UI bug (composer default + bogus handover)

### Root cause (confirmed)

`info.json` for `1784731859181` correctly locked `plan:false, model:nemotron…`.
But non-plan first-exchange model resolution in
[chats.py:1255-1260](../../.pi/crack/server/src/crack_server/chats.py#L1255)
reads:

```python
model = (cur_exchange.get("model")
         or info.get("implementer_model")   # ← plan-mode field, defaulted to composer-2.5
         or info.get("model")               # ← the actually-locked non-plan model
         or DEFAULT_CHAT_MODEL)
```

For the first exchange, `post_message` deliberately clears the per-exchange
`model` ([chats.py:974](../../.pi/crack/server/src/crack_server/chats.py#L974)),
so `cur_exchange.get("model")` is `None` and **`implementer_model` (composer)
shadows the locked `model` (nemotron)**. Exchange 0 ran composer, later
continuations (which carry a per-exchange `model`) ran nemotron → the UI shows a
spurious model "handover."

### Fix

- Reorder the non-plan branch so the locked non-plan `model` wins:
  `cur_exchange.get("model") or info.get("model") or DEFAULT_CHAT_MODEL`. The
  `implementer_model` fallback belongs only to a **graduated plan** chat — gate
  it on `info.get("graduated")`, not on plain non-plan mode.
- Harden the route's config inference. `config_shown =
  bool(planner_model or implementer_model)`
  ([routes_chats.py:126-127](../../.pi/crack/server/src/crack_server/routes_chats.py#L126))
  is fragile: the editor is *always* shown before the first message, so send an
  explicit `config=1` hidden field from `render_chat_config_editor` and lock
  `plan = bool(plan_checkbox)` whenever `config` is present. Removes the
  "defaults silently win when plan is off" failure mode entirely.
- With Issue 2's projection, the handover display derives from real
  `model_change` events, so even a correct single-model chat shows no handover.
- Verify the config editor's selects actually submit inside the send `<form>`
  (they do today via `hidden` attribute, but confirm after the `config=1` change).

---

## Issue 5 — Concurrent top-level chats + hand edits (the architecture question)

### The hazard

Every sandbox mounts the **same live host tree** as its overlay lower and every
finalize `git apply`s onto it. With two concurrent chats (or a human editing
during a chat):

- overlayfs explicitly treats *lower changed while mounted* as undefined —
  chat B can see torn/inconsistent reads when chat A (or the human) writes the
  host tree;
- chat B's baseline was captured from the original tree; after A applies, B's
  finalize diff conflicts against a moved host;
- two finalize applies can interleave on the host working tree.

### Chosen scheme — frozen git-tree snapshot per sandbox

At sandbox creation, snapshot the host **once** and give each sandbox its own
immutable base, materialised git-natively (no `cp`, ignored files excluded):

1. **Snapshot** (host, at `ensure_sandbox` / chat start): `tree=$(git write-tree)`
   after the clean-git gate guarantees a clean index == HEAD. Record `tree` in
   the chat/run artifact dir. (Clean gate ⇒ this equals `HEAD^{tree}`; using
   `write-tree` still works and is future-proof if we ever relax the gate.)
2. **Materialise the base directory** from that tree without dragging ignored
   junk: `git archive <tree> | tar -x -C <base>` **or** `GIT_INDEX_FILE=…
   git read-tree <tree> && git checkout-index -a --prefix=<base>/`. Store under
   the harness volume: `overlays/<conv>/base` (tracked files only — small).
3. **Mount** `<base>` as the `:O` overlay lower for `/workspace`
   (replacing `_host_repo()` in [sandbox.py:147](../../.pi/crack/server/src/crack_server/sandbox.py#L147)).
   Upper/work stay per-conv as today.
4. **Ignored-but-needed dirs** (the main risk — see below) are mounted
   separately, exactly as `target`/`/root` already are.
5. **Finalize** is unchanged in spirit: diff sandbox vs its frozen base, `git
   apply --3way` onto the live host. Because every chat froze its *own* base and
   the host was clean at fork, cross-chat corruption is gone; a genuine
   host-moved conflict still routes to the agent.

Baseline capture (`patch.capture_baseline`) can key off this frozen tree
directly instead of re-`add -A`/`write-tree` inside the sandbox — the frozen
`tree` *is* the base tree, so `base_tree` = the snapshot id (one fewer in-sandbox
git round-trip, and it's guaranteed to match the lower).

### The one real risk to resolve during implementation

A tracked-only lower **drops gitignored dirs the sandbox needs at runtime**.
Today they come "for free" from the live-tree lower.

- **The Python venv (the acute one) is now solved without any new mount.** The
  crack-pi-server package was switched **from uv to Poetry** with
  `POETRY_VIRTUALENVS_PATH=/workspace/target/python-venvs/` (set in
  `_docker/Dockerfile`), so its virtualenv lives *inside the existing
  `crack-dev-target-dir` volume* (mounted `/workspace/target`, already a `:O`
  overlay in every sandbox and already gitignored). uv was abandoned here
  because it cannot relocate its per-project `.venv` out of the source tree
  (symlinks get clobbered/recreated), which is exactly what the frozen-tree base
  must exclude. `poetry install` runs at crack-dev boot (`_cont_start.sh`),
  `poetry run pytest` powers the self-mod gate, and the sandbox inherits the venv
  read-through the target overlay. **No additional volume is created.**
- **Other ignored dirs**: audit `_sandbox_common.sh` / `_sandbox_start.sh` for
  any remaining implicit host-path assumptions (e.g. `.pi/crack/harness/
  mcp-http/` runtime logs, caches). Anything genuinely needed at runtime should
  live under the existing `target` volume too — **do not add new volumes** and do
  **not** carry `_data` bulk caches into the fork. `target` (build/cache) and
  `/root` remain their own `:O` volumes as today.

### Rejected alternatives (recorded)

- *Live-tree lower + serialize/global-lock*: kills concurrency and hand-editing,
  and still leaves overlay-lower-mutation undefined if the human edits mid-chat.
- *Git worktree per chat*: most VCS-correct, but a larger change to the mount
  model and cross-container `.git` locking; not chosen now, revisit if the
  frozen-tree apply-conflict rate proves high.

---

## Implementation order

1. **1a + 1b + 1c** (crash detection, unique hop files, real-error capture) —
   restores basic pi usability; smallest blast radius, unblocks live testing.
2. **4** (model precedence + `config=1`) — trivial, removes the wrong-model run.
3. **3a** (clean-git gate) — cheap, and a prerequisite invariant for 5.
4. **5** (frozen snapshot base + ignored-dir mounts) — the architectural change;
   gate behind the clean-git invariant.
5. **1d** (continue-if-tools) — after crashes are gone, so we can tell a real
   settle from a crash.
6. **2** (trajectory projection + faithful unknown-event rendering) — largest UI
   change; do last so its verification isn't confounded by the pi crashes.

## Verification (live, via `docker exec` for pi and browser-MCP for UI)

- **Regression/unit** (`python -m pytest` in the server dir): crash→retry in
  sandbox mode; non-plan model precedence; clean-git gate refusal; frozen-tree
  base materialisation excludes ignored, includes `.venv` mount; trajectory
  projection incl. an injected unknown event type → Expand row.
- **Live guided chat** (nemotron-3-ultra, plan off): start a fresh chat, confirm
  the composer/handover is gone, drive it through several **MCP tool calls +
  a small edit to a patch**, and confirm it **no longer hangs** after each tool
  (auto-continues), that a forced provider error shows the **real** error text,
  and the trajectory in the UI matches `sessions/*.jsonl` exactly.
- **Dirty-git gate**: with an untracked file present, confirm the first message
  is refused with the colourised 10-line `git status` `<pre>`.
- **Concurrency**: run two top-level chats at once, each touching overlapping
  files, plus a hand edit on the host; confirm neither corrupts the other, both
  finalize, and conflicts (if any) route to the agent rather than silently
  losing work.
- Clean up all test chats/overlays/sandboxes afterwards; leave the working tree
  with only intended changes (nothing committed — per convention).

## Prerequisite already landed — uv → Poetry for crack-pi-server

Done ahead of the frozen-base work (unblocks Issue 5's venv risk):
`pyproject.toml` converted to Poetry (PEP 621 + `poetry-core`), `poetry.lock`
generated, `uv.lock` removed; `_docker/Dockerfile` installs Poetry and sets
`POETRY_VIRTUALENVS_PATH=/workspace/target/python-venvs/`; `_cont_start.sh` uses
`poetry install` / `poetry run crack-server`; the self-mod gate uses `poetry run
pytest`. Full suite (143) green under Poetry. **Requires a `crack-dev` image
rebuild + restart** for the new ENV/Poetry to take effect (`_docker/build.sh` +
`_docker/run.sh`); commit `poetry.lock`.

## Files touched (anticipated)

- `pi_proc.py` — crash return flag + reason, unique hop output paths, NUL
  scrubbing, structured-error extraction.
- `chat_engine.py` — continue-if-tools; consume `crashed` reason.
- `chats.py` — non-plan model precedence; clean-git gate in `post_message`;
  frozen-base baseline wiring.
- `routes_chats.py` — `config=1` explicit lock.
- `git_utils.py` — `host_worktree_dirty` + colourised status.
- `sandbox.py` — frozen-tree snapshot/materialise; lower = frozen base. **No new
  volumes** — the venv already rides the existing `target` `:O` volume via
  Poetry (`POETRY_VIRTUALENVS_PATH`), so no extra ignored-dir mounts are needed.
- `patch.py` — baseline keyed off frozen tree.
- `trajectory_view.py` (new) + `render.py`, `chats.py` render path — pi-ndjson
  projection with faithful unknown-event rendering.
- `static/app.js` / `app.css` — Expand control for unknown events; red gate
  message styling.
- `_docker/_sandbox_start.sh` / `_sandbox_common.sh` — audit host-path
  assumptions under the frozen lower.
- **(landed)** `pyproject.toml`, `poetry.lock` (new), `uv.lock` (removed),
  `_docker/Dockerfile`, `_docker/_cont_start.sh`, `patch.py:run_sandbox_tests`,
  `.pi/crack/server/README.md` — uv → Poetry migration.
