# Fix retries, error visibility, pi-process survival & attachment UI

## Context

The multimodal plan (`_slop/multimodal-plan-sonet.md`) was implemented by another
model and mostly works, but four problems remain. All work happens **inside the
`crack-dev` docker container** (`docker exec crack-dev …` only — `pi`, `python`,
`uv`, tests, browsers live there; the repo is mounted at `/workspace`).

The concrete failing case: task `1784548497158_download_and_parse_news` is stuck
with `status: error`, `error: pi exited -9 after 1 attempts`, `turns_completed: 1`.

### Root causes (confirmed by reading the code + the stuck `explore.json`)

1. **Retries give up after 1 attempt on an unexpected kill.** In
   [`_run_hop_with_retries`](.pi/crack/server/src/crack_server/pi_proc.py#L617-L629),
   a *hard* (non-transient) failure that occurs **after ≥1 turn was persisted**
   hits `if res["persisted"] > 0: break` and raises immediately — so a
   SIGKILL (`-9`) mid-hop yields "after 1 attempts" instead of the intended
   4–5 backed-off retries. `-9` is also never classified as retryable (only
   text-matched transients are). The stuck task has `turns_completed: 1`, which
   is the exact fingerprint.

2. **`-9` is not OOM — it's the reload killing pi.** No OOM in `dmesg`. The
   worker runs *inside* the uvicorn server with **`reload=True`**
   ([`worker.py:12-15`](.pi/crack/server/src/crack_server/worker.py#L12-L15)); a
   source edit restarts the whole process. Two code paths then kill the
   in-flight pi: [`_attempt_once`'s `except asyncio.CancelledError:
   _kill_process_group(proc)`](.pi/crack/server/src/crack_server/pi_proc.py#L525-L529)
   on shutdown (SIGKILL), and
   [`_kill_orphaned_agents()`](.pi/crack/server/src/crack_server/worker.py#L139-L159)
   on the *next* startup (SIGTERM→SIGKILL any surviving `*.agent.pid`). pi's
   stdout is also a **pipe to the worker**, so even without an explicit kill the
   pipe breaks on restart. We want pi to **survive worker/server reload to
   finality** and the worker to **re-attach** to it.

3. **Errors aren't durable trajectory rows.** The error is only a scalar
   (`state["error"]`/`error_detail`) shown as a *volatile tail card*
   ([`render_error_msg`](.pi/crack/server/src/crack_server/stages/render.py#L300-L312))
   that's cleared on retry — only the latest error, gone after re-run. We want
   **every failed attempt** recorded as a timestamped row, mixed into the
   trajectory table sorted by time, shown in the UI but **never fed to the bot**.

4. **Attachment UI gaps.** Paste/drop shows **no loading spinner** while the
   image uploads + is described by the vision model
   ([`uploadAttachment`](.pi/crack/server/src/crack_server/static/app.js#L331-L347)
   only touches the DOM after the slow response). And a **sent message shows no
   thumbnails** — attachments are woven in as *text only* (`attachments.format_block`),
   the manifest is cleared, and
   [`render_user_prompt_msg`](.pi/crack/server/src/crack_server/stages/render.py#L168-L205)
   has no image path.

### Decisions (from the user)

- Record **every failed attempt** as an error row.
- Persist error rows in an **in-state `errors` key** (resets on a full fresh
  re-run, survives resume — mirrors how `turns` behaves; never reaches the bot
  because agent context comes from the pi session dir + `turns`, not `errors`).
- **Backoff schedule: exactly `1s, 3s, 9s, 27s`** before reattempts 2–5
  (5 attempts/streak, ~40s total). The streak **resets whenever pi persists a
  new turn** ("succeeded in sending a message" → back to 1s, 4 more tries).
- **Global cap of 20 errors** per task/chat; past it, stop auto-retrying, land
  in error, and show the normal "Continue from last error" UI **plus** a
  "failed more than 20 times — something is wrong" label. Each manual continue
  grants another 20.
- Make pi processes **survive a code-change auto-reload** and re-attach.

---

## Workstream A — Retry every error, with the new backoff + 20-cap

**`ratelimit.py`** — add the exact schedule and helpers:
- `HARD_RETRY_DELAYS = [1.0, 3.0, 9.0, 27.0]` (before streak reattempts 2–5).
- `MAX_TOTAL_ERRORS = 20`.
- `async def _async_hard_backoff_sleep(streak: int)` — sleeps
  `HARD_RETRY_DELAYS[min(streak-1, len-1)]` (streak is 1-based no-progress count;
  a progress-reset streak of 0 → next delay index 0 = 1s).
- Keep `is_transient` but treat `-9`/`137` and any hard failure the same as a
  retryable failure now (classification no longer gates *whether* we retry, only
  *which* backoff — transients keep `TRANSIENT_RETRY_DELAYS`).

**`pi_proc.py` — rewrite `_run_hop_with_retries`** ([L569-L629](.pi/crack/server/src/crack_server/pi_proc.py#L569-L629)):
- Track `consecutive_no_progress` (streak) instead of the current
  break-on-persisted logic. Per iteration:
  - **Success** (clean exit, terminated_by_us, or persisted a real turn) →
    return the reason (unchanged happy path).
  - **Failure / empty** → call the new `record_error` callback (Workstream B),
    which returns the new total error count.
    - If this attempt **persisted a new turn** (progress): reset
      `consecutive_no_progress = 0`, set `attempt_message = RESUME_MESSAGE`.
    - Else: `consecutive_no_progress += 1`.
    - If `total_errors >= MAX_TOTAL_ERRORS` → raise `PiError(..., over_budget=True)`.
    - If `consecutive_no_progress > len(HARD_RETRY_DELAYS)` (5 tries, no
      progress) → raise `PiError(...)` (streak exhausted).
    - Else backoff-sleep and continue (resuming when turns were persisted, else
      replaying the original message).
- **Delete** the `if res["persisted"] > 0: break` give-up. Partial progress now
  *resumes and keeps retrying* on the reset streak.
- Add `over_budget: bool` to `PiError` so the stage can set the special flag.
- Apply the same "record each failed attempt + reset-on-progress" treatment
  to the one-off retry loop in
  [`arun_pi_text`](.pi/crack/server/src/crack_server/pi_proc.py#L149-L197)
  (no persisted turns there, so it's just: record each attempt; the loop already
  runs the full budget). Wire `record_error` through both entrypoints alongside
  the existing `record_prompt`.

**Stage side** — `retry_from_error`
([`s01_explore.py:482`](.pi/crack/server/src/crack_server/stages/s01_explore.py#L482-L500)
and the base default): when re-continuing, set
`state["error_budget"] = len(state.get("errors", [])) + MAX_TOTAL_ERRORS` and
clear the over-budget flag, so each manual continue grants another 20. The retry
driver reads the current budget via a passed callable (default 20).

---

## Workstream B — Durable, timestamped error rows in the trajectory

**Timestamps on turns.** Add `"at": time.time()` to
[`make_turn`](.pi/crack/server/src/crack_server/stages/steprun.py#L91-L99) and to
Explore's own
[`_persist_explore_turn`](.pi/crack/server/src/crack_server/stages/s01_explore.py#L82-L105)
dict, so error rows can be interleaved by time. (`user_prompt` entries already
carry `"at"`.)

**Error recorder.** New helper in `steprun.py`:
```python
def error_recorder(state: JsonState, key: str = "errors", subpath=None) -> Callable[[dict], int]:
    # appends {"kind":"error","at":..., "message","detail","rc","attempt","phase"}
    # to state[...][key], returns the new total count (for the 20-cap check).
```
Wire it into each stage's hop call next to `record_prompt` (Explore's
`_run_hop`, and the s02–s06 call sites / `TurnPersister` owners). pi_proc calls
`record_error(entry)` on **every** failed attempt (hard, transient, empty,
timeout) and uses its return value for the cap.

**Rendering — mix error rows in by timestamp.** In
[`render.py`](.pi/crack/server/src/crack_server/stages/render.py):
- Add `render_error_row(entry)` → a `.stage-msg` error row (reuse the
  `render_error_msg` markup: `⚠ message` + collapsible `detail`, plus attempt #
  and a relative time).
- Change `render_turn_msgs(turns, errors=None, include_text=True)`: when
  `errors` is given, build a merged list of `(at, kind, payload)` from both
  `turns` and `errors`, **sort by `at`**, and emit each (turn → actions table
  msg, `user_prompt` → prompt msg, error → error row). Turns without `at`
  (legacy) keep list order. Errors always sort at/after the turns they follow,
  so the append-only `wrap_status` delta-swap stays consistent.
- Each stage's `render_msgs` passes `state.get("errors", [])` (one added arg).
  Representative: `s01_explore.py:532`, plus s02/s04/s05, chats via
  `render_exchanges`, sub-agents via `render_turn_msgs`.

**"Something is wrong" label.** In the error tail
([`s01_explore.py:572-573`](.pi/crack/server/src/crack_server/stages/s01_explore.py#L572-L573)
and shared `render_error_msg` callers): when `state.get("error_over_budget")` /
`len(errors) >= error_budget`, prepend a prominent banner ("Failed more than 20
times — something is likely wrong") above the existing "Continue from last
error" button. Add `.stage-error--fatal` styling in
[`app.css`](.pi/crack/server/src/crack_server/static/app.css).

**Lifecycle correctness.** Fresh-start state dicts (e.g.
[`s01_explore.py:224-240`](.pi/crack/server/src/crack_server/stages/s01_explore.py#L224-L240))
add `"errors": []` and `"error_budget": 20`. The `record_errors` context
manager and `post_user_message`/`retry_from_error` clear the scalar
`error`/`error_detail` (as today) but **never** clear `errors`.

---

## Workstream C — pi survives worker/server reload & re-attaches (largest lift)

**Goal:** a reload must not kill in-flight pi; the restarted worker re-attaches
to the live pi and drives it to completion. pi reads its prompt from `argv`
(not stdin — [`_build_cmd`](.pi/crack/server/src/crack_server/pi_proc.py#L470-L477)),
so only its **stdout event stream** needs to become durable and re-attachable —
**a plain append-only file, not a unix socket** (simpler, reopenable, survives
either end dying).

Feasible because the container runs `docker run --init` (`_docker/run.sh`), so
**tini is PID 1**: a pi we stop killing reparents to tini, keeps running, and is
reaped cleanly. The reload path is `watchfiles → SIGTERM the uvicorn child →
lifespan `finally` cancels the worker task → `_dispatch` `CancelledError` →
`_kill_process_group` SIGKILL` (`main.py` has `reload=True`,
`reload_dirs=[src]`). Removing that SIGKILL is the linchpin.

**Hop manifest + durable output.** New per-hop files alongside the session dir
(new `paths.py` helpers, mirroring `stage_pid_file`):
- `…/<stage>/hop.jsonl` — pi's stdout+stderr, **redirected to this file**
  (open in append; replace the `stdout=PIPE` in
  [`_attempt_once`](.pi/crack/server/src/crack_server/pi_proc.py#L508-L513)).
- `…/<stage>/hop.json` — manifest: `{pid, started_at, output_path, offset,
  session_id, model, tools, message, hop, timeout, status}` where `status ∈
  {running, done, crashed}` and `offset` is bytes of `hop.jsonl` already
  consumed/persisted.

**Tail-based streaming.** Replace `async for raw in proc.stdout` in
[`_stream_events`](.pi/crack/server/src/crack_server/pi_proc.py#L390-L450) with
a loop that reads new lines from `hop.jsonl` starting at `offset`, persists
turns, and **advances `offset` in the manifest after each persisted turn**. The
same routine serves a freshly-spawned pi *and* a re-attached one (start at the
stored offset). Liveness = `os.kill(pid, 0)` + the existing active-time
watchdog; terminal = an `agent_end`/`agent_settled`/sentinel event **or** the
pid disappearing.

**Do not kill on reload.**
- [`_attempt_once`'s `except asyncio.CancelledError`](.pi/crack/server/src/crack_server/pi_proc.py#L525-L529):
  **stop killing** — persist `offset`, mark manifest `status:"running"` (a.k.a.
  detached), and re-raise so the worker unwinds. pi keeps writing to `hop.jsonl`.
- [`_kill_orphaned_agents()`](.pi/crack/server/src/crack_server/worker.py#L139-L159):
  replace with `recover_detached_hops()` — for each hop manifest whose `pid` is
  **alive**, leave it running (it will be re-attached); only `kill_pid_file` +
  clean up manifests whose pid is **dead** and which have no terminal event
  (then let session-resume handle them).

**Re-attach instead of double-spawn.** At the top of
[`arun_agent_hop`](.pi/crack/server/src/crack_server/pi_proc.py#L632-L699)
(before spawning), check for a live hop manifest for this `session_id`/`pid_file`:
- **Live pid** → re-attach: tail `hop.jsonl` from `offset` to completion,
  persisting new turns, derive the reason — **no new pi**. This prevents two pi
  processes writing the same `--session-dir` (which would corrupt the session).
- **Dead/absent** → spawn fresh (or resume the session), writing a new manifest.

The queue already re-pends the in-flight job on restart
([`reclaim_orphans`](.pi/crack/server/src/crack_server/queue.py#L202-L222)) and
Explore already reconstructs its surrounding loop state from disk and resumes
([`_run_job` B5 path](.pi/crack/server/src/crack_server/stages/s01_explore.py#L322-L325)).
So re-pickup → `_run_job` resumes → `arun_agent_hop` finds the live manifest →
re-attaches. `reclaim_orphans` must **not** re-dispatch a job whose hop is still
live in a way that spawns a competitor — the manifest-check in `arun_agent_hop`
is the guard (single process, atomic claim), but confirm no path spawns a second
pi for the same session while one is alive.

**STOP still kills** (user-initiated `kill_pid_file` via
[`request_stop`](.pi/crack/server/src/crack_server/stages/base.py#L305-L323)) —
unchanged and desired.

**Scope note:** apply the detached-hop model to the **agent-hop path** only (the
long-running, turn-persisting calls). Leave one-off `arun_pi_text` (turn_zero /
gate / summary / vision, all <120s and idempotent on re-pickup) pipe-based for
now — call this out for the user.

---

## Workstream D — Attachment UI: paste spinner + sent-message thumbnails

**Bug A — loading spinner.** In
[`uploadAttachment`](.pi/crack/server/src/crack_server/static/app.js#L331-L347):
before `fetch`, insert a placeholder chip (`.attachment-chip.loading` with an
`aria-busy` spinner) into the strip; on resolve, `replaceWith` the returned real
chip; on error, remove the placeholder + alert. Add `.attachment-chip.loading`
spinner CSS in [`app.css`](.pi/crack/server/src/crack_server/static/app.css)
(reuse the `aria-busy` idiom already used by `render_spinner`).

**Bug B — thumbnails on the sent message.** Persist the staged attachments as a
`media` list on the recorded `user_prompt` entry, then render them:
- **Chats** ([`post_message`](.pi/crack/server/src/crack_server/chats.py#L394-L429)):
  before clearing the manifest, capture the entries and stash a
  `media: [{url, src, description}]` list onto the exchange (e.g.
  `exchange["media"]`). In
  [`render_exchanges`](.pi/crack/server/src/crack_server/stages/render.py#L262-L292),
  pass that media into the `render_user_prompt_msg` entry.
- **Tasks**: task attachments are persistent; attach their `media` list to each
  stage's recorded `user_prompt` entry (thread it through the
  `record_prompt`/`prompt_recorder` that already builds that entry, reading the
  same manifest `read_all_prompts_joined` uses).
- Extend
  [`render_user_prompt_msg`](.pi/crack/server/src/crack_server/stages/render.py#L168-L205)
  to render an `_render_media_thumbs`-style strip from `entry.get("media")`
  (reuse the existing `.tool-thumb` + `#img-lightbox` component — one more call
  site, no new CSS/JS).

---

## Verification (all via `docker exec crack-dev …`)

- **Unit tests:** `docker exec crack-dev bash -lc 'cd /workspace/.pi/crack/server
  && python -m pytest'` (per the repo convention — `python -m pytest`, not bare
  `pytest`). Add cases for: retry backoff uses `[1,3,9,27]`; streak resets on a
  progressing attempt; the 20-cap raises `over_budget`; `record_error` appends a
  timestamped row and returns the running total; `render_turn_msgs` interleaves
  error rows by `at`; re-attach picks up a live hop manifest and tails from
  `offset` without spawning a second pi; `_attempt_once` no longer kills pi on
  `CancelledError`.
- **Retry/error rows, live:** unstick the real task — `docker exec crack-dev bash
  -lc '…'` to hit `retry_from_error`, or start a fresh explore, and confirm in
  the UI (`http://localhost:9847/tasks/<id>/view/explore`) that failed attempts
  now appear as interleaved error rows and retries actually fire (watch the
  worker log for the `1/3/9/27s` sleeps).
- **Reload survival:** start an explore hop, then `touch` a server source file to
  trigger the reload while pi is mid-turn; confirm (a) no new `-9` in the trajectory,
  (b) pi keeps running (`docker exec crack-dev pgrep -af '\bpi\b'`), and (c) the
  restarted worker re-attaches and the hop completes.
- **20-cap:** force repeated failures and confirm the run stops at 20 with the
  "something is wrong" banner + "Continue from last error"; clicking it grants
  another 20.
- **Attachments:** paste an image into a chat/task box → a loading chip appears
  immediately, then becomes the described thumbnail; send the message → the sent
  user bubble shows the expandable thumbnail(s).

## Risks / notes

- Workstream C is the biggest and riskiest change (durable output + re-attach +
  removing two kill paths). It can land after A/B/D, which already fix the
  "gives up after 1 attempt" and visibility problems. Recommend A+B+D first,
  then C.
- If the `-9` persists after C, it confirms an external killer (then revisit OOM
  despite the empty dmesg, e.g. container memory limits on the 550B ULTRA model).
